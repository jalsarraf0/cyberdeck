use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};

use crate::keys::{generate_ed25519_key, import_private_key, scan_local_keys};
use crate::models::{AppConfig, AuthMethod, LocalKey, TargetProfile};
use crate::ssh_ops::{
    exchange_public_key, fetch_remote_authorized_keys, run_remote_command, test_connection,
};
use crate::storage::{load_config, save_config};

const MAX_LOG_LINES: usize = 200;
const MAX_CONSOLE_LINES: usize = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tab {
    Keys,
    Targets,
    Exchange,
    Console,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Keys, Tab::Targets, Tab::Exchange, Tab::Console];

    fn index(self) -> usize {
        match self {
            Tab::Keys => 0,
            Tab::Targets => 1,
            Tab::Exchange => 2,
            Tab::Console => 3,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Keys => "KEYS",
            Tab::Targets => "TARGETS",
            Tab::Exchange => "EXCHANGE",
            Tab::Console => "SSH CONSOLE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Theme {
    Cyberpunk,
    Synth,
    Matrix,
    Ember,
    Glacier,
}

impl Theme {
    const ALL: [Theme; 5] = [
        Theme::Cyberpunk,
        Theme::Synth,
        Theme::Matrix,
        Theme::Ember,
        Theme::Glacier,
    ];

    fn title(self) -> &'static str {
        match self {
            Theme::Cyberpunk => "Cyberpunk",
            Theme::Synth => "Synth",
            Theme::Matrix => "Matrix",
            Theme::Ember => "Ember",
            Theme::Glacier => "Glacier",
        }
    }

    fn config_value(self) -> &'static str {
        match self {
            Theme::Cyberpunk => "cyberpunk",
            Theme::Synth => "synth",
            Theme::Matrix => "matrix",
            Theme::Ember => "ember",
            Theme::Glacier => "glacier",
        }
    }

    fn from_config_value(value: Option<&str>) -> Theme {
        match value.unwrap_or("").trim().to_lowercase().as_str() {
            "synth" => Theme::Synth,
            "matrix" => Theme::Matrix,
            "ember" => Theme::Ember,
            "glacier" => Theme::Glacier,
            _ => Theme::Cyberpunk,
        }
    }

    fn next(self) -> Theme {
        let idx = Theme::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Theme::ALL[(idx + 1) % Theme::ALL.len()]
    }

    fn prev(self) -> Theme {
        let idx = Theme::ALL.iter().position(|t| *t == self).unwrap_or(0);
        if idx == 0 {
            Theme::ALL[Theme::ALL.len() - 1]
        } else {
            Theme::ALL[idx - 1]
        }
    }
}

#[derive(Clone, Debug)]
enum Modal {
    None,
    AddTarget(TargetForm),
    GenerateKey(GenerateKeyForm),
    ImportKey(ImportKeyForm),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthFormKind {
    Password,
    KeyFile,
}

#[derive(Clone, Debug)]
struct TargetForm {
    field: usize,
    name: String,
    host: String,
    port: String,
    user: String,
    auth_kind: AuthFormKind,
    password: String,
    private_key: String,
    passphrase: String,
}

impl Default for TargetForm {
    fn default() -> Self {
        Self {
            field: 0,
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            user: String::new(),
            auth_kind: AuthFormKind::Password,
            password: String::new(),
            private_key: String::new(),
            passphrase: String::new(),
        }
    }
}

impl TargetForm {
    fn fields_len(&self) -> usize {
        match self.auth_kind {
            AuthFormKind::Password => 6,
            AuthFormKind::KeyFile => 7,
        }
    }

    fn next_field(&mut self) {
        self.field = (self.field + 1) % self.fields_len();
    }

    fn prev_field(&mut self) {
        if self.field == 0 {
            self.field = self.fields_len() - 1;
        } else {
            self.field -= 1;
        }
    }

    fn set_auth_kind(&mut self, kind: AuthFormKind) {
        self.auth_kind = kind;
        if self.field >= self.fields_len() {
            self.field = self.fields_len() - 1;
        }
    }

    fn active_field_mut(&mut self) -> Option<&mut String> {
        match (self.auth_kind, self.field) {
            (_, 0) => Some(&mut self.name),
            (_, 1) => Some(&mut self.host),
            (_, 2) => Some(&mut self.port),
            (_, 3) => Some(&mut self.user),
            (AuthFormKind::Password, 5) => Some(&mut self.password),
            (AuthFormKind::KeyFile, 5) => Some(&mut self.private_key),
            (AuthFormKind::KeyFile, 6) => Some(&mut self.passphrase),
            _ => None,
        }
    }

    fn lines(&self) -> Vec<(String, String, bool)> {
        let mut rows = vec![
            ("Name".to_string(), self.name.clone(), false),
            ("Host (ip/domain)".to_string(), self.host.clone(), false),
            ("Port".to_string(), self.port.clone(), false),
            ("User".to_string(), self.user.clone(), false),
            (
                "Auth".to_string(),
                match self.auth_kind {
                    AuthFormKind::Password => "password".to_string(),
                    AuthFormKind::KeyFile => "key_file".to_string(),
                },
                false,
            ),
        ];

        match self.auth_kind {
            AuthFormKind::Password => {
                rows.push(("Password".to_string(), mask_secret(&self.password), true));
            }
            AuthFormKind::KeyFile => {
                rows.push((
                    "Private Key Path".to_string(),
                    self.private_key.clone(),
                    false,
                ));
                rows.push((
                    "Key Passphrase (optional)".to_string(),
                    mask_secret(&self.passphrase),
                    true,
                ));
            }
        }

        rows
    }

    fn to_profile(&self) -> Result<TargetProfile> {
        let name = self.name.trim();
        let host = self.host.trim();
        let user = self.user.trim();
        if name.is_empty() {
            return Err(anyhow!("target name cannot be empty"));
        }
        if host.is_empty() {
            return Err(anyhow!("host cannot be empty"));
        }
        if host.chars().any(char::is_whitespace) {
            return Err(anyhow!("host cannot contain whitespace"));
        }
        if user.is_empty() {
            return Err(anyhow!("user cannot be empty"));
        }
        if user.chars().any(char::is_whitespace) {
            return Err(anyhow!("user cannot contain whitespace"));
        }

        let port = self
            .port
            .trim()
            .parse::<u16>()
            .map_err(|_| anyhow!("port must be a valid number"))?;

        let auth = match self.auth_kind {
            AuthFormKind::Password => {
                if self.password.is_empty() {
                    return Err(anyhow!("password cannot be empty"));
                }
                AuthMethod::Password {
                    password: self.password.clone(),
                }
            }
            AuthFormKind::KeyFile => {
                let private_key = self.private_key.trim();
                if private_key.is_empty() {
                    return Err(anyhow!("private key path cannot be empty"));
                }
                AuthMethod::KeyFile {
                    private_key: private_key.to_string(),
                    passphrase: if self.passphrase.trim().is_empty() {
                        None
                    } else {
                        Some(self.passphrase.clone())
                    },
                }
            }
        };

        Ok(TargetProfile {
            name: name.to_string(),
            host: host.to_string(),
            port,
            user: user.to_string(),
            auth,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct GenerateKeyForm {
    field: usize,
    name: String,
    comment: String,
    passphrase: String,
}

impl GenerateKeyForm {
    fn fields_len(&self) -> usize {
        3
    }

    fn next_field(&mut self) {
        self.field = (self.field + 1) % self.fields_len();
    }

    fn prev_field(&mut self) {
        if self.field == 0 {
            self.field = self.fields_len() - 1;
        } else {
            self.field -= 1;
        }
    }

    fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.name,
            1 => &mut self.comment,
            _ => &mut self.passphrase,
        }
    }

    fn lines(&self) -> Vec<(String, String, bool)> {
        vec![
            ("Key Name".to_string(), self.name.clone(), false),
            ("Comment".to_string(), self.comment.clone(), false),
            (
                "Passphrase (optional)".to_string(),
                mask_secret(&self.passphrase),
                true,
            ),
        ]
    }
}

#[derive(Clone, Debug, Default)]
struct ImportKeyForm {
    field: usize,
    name: String,
    private_key: String,
    passphrase: String,
}

impl ImportKeyForm {
    fn fields_len(&self) -> usize {
        3
    }

    fn next_field(&mut self) {
        self.field = (self.field + 1) % self.fields_len();
    }

    fn prev_field(&mut self) {
        if self.field == 0 {
            self.field = self.fields_len() - 1;
        } else {
            self.field -= 1;
        }
    }

    fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.name,
            1 => &mut self.private_key,
            _ => &mut self.passphrase,
        }
    }

    fn lines(&self) -> Vec<(String, String, bool)> {
        vec![
            (
                "Display Name (optional)".to_string(),
                self.name.clone(),
                false,
            ),
            (
                "Private Key Path".to_string(),
                self.private_key.clone(),
                false,
            ),
            (
                "Passphrase (optional)".to_string(),
                mask_secret(&self.passphrase),
                true,
            ),
        ]
    }
}

struct App {
    tab: Tab,
    modal: Modal,
    config: AppConfig,
    local_keys: Vec<LocalKey>,
    selected_key: usize,
    selected_target: usize,
    remote_keys: Vec<String>,
    selected_remote_key: usize,
    logs: VecDeque<String>,
    console_input: String,
    console_output: VecDeque<String>,
    console_editing: bool,
    theme: Theme,
}

impl App {
    fn new() -> Result<Self> {
        let config = load_config()?;
        let theme = Theme::from_config_value(config.theme.as_deref());
        set_active_theme(theme);
        let local_keys = scan_local_keys()?;
        let mut app = Self {
            tab: Tab::Keys,
            modal: Modal::None,
            config,
            local_keys,
            selected_key: 0,
            selected_target: 0,
            remote_keys: Vec::new(),
            selected_remote_key: 0,
            logs: VecDeque::new(),
            console_input: String::new(),
            console_output: VecDeque::new(),
            console_editing: false,
            theme,
        };
        app.log("SYSTEM BOOT: Cyberdeck cyber deck ready.");
        app.log(format!("Theme loaded: {}", app.theme.title()));
        app.log("Tip: press 1-4 to switch tabs, F2 to change theme, q to quit.");
        Ok(app)
    }

    fn log<S: Into<String>>(&mut self, msg: S) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(msg.into());
    }

    fn console_push<S: Into<String>>(&mut self, msg: S) {
        if self.console_output.len() >= MAX_CONSOLE_LINES {
            self.console_output.pop_front();
        }
        self.console_output.push_back(msg.into());
    }

    fn clamp_selection(&mut self) {
        if self.local_keys.is_empty() {
            self.selected_key = 0;
        } else {
            self.selected_key = self.selected_key.min(self.local_keys.len() - 1);
        }

        if self.config.targets.is_empty() {
            self.selected_target = 0;
        } else {
            self.selected_target = self.selected_target.min(self.config.targets.len() - 1);
        }

        if self.remote_keys.is_empty() {
            self.selected_remote_key = 0;
        } else {
            self.selected_remote_key = self.selected_remote_key.min(self.remote_keys.len() - 1);
        }
    }

    fn refresh_local_keys(&mut self) {
        match scan_local_keys() {
            Ok(keys) => {
                self.local_keys = keys;
                self.clamp_selection();
                self.log("Local key cache refreshed.");
            }
            Err(err) => self.log(format!("ERROR: failed to refresh keys: {err}")),
        }
    }

    fn save_config(&mut self) {
        if let Err(err) = save_config(&self.config) {
            self.log(format!("ERROR: failed to save config: {err}"));
        }
    }

    fn cycle_theme(&mut self, forward: bool) {
        self.theme = if forward {
            self.theme.next()
        } else {
            self.theme.prev()
        };
        set_active_theme(self.theme);
        self.config.theme = Some(self.theme.config_value().to_string());
        self.save_config();
        self.log(format!("Theme switched to {}", self.theme.title()));
    }

    fn selected_target(&self) -> Option<&TargetProfile> {
        self.config.targets.get(self.selected_target)
    }

    fn selected_key(&self) -> Option<&LocalKey> {
        self.local_keys.get(self.selected_key)
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        let modal = std::mem::replace(&mut self.modal, Modal::None);
        match modal {
            Modal::None => self.on_key_main(key),
            Modal::AddTarget(mut form) => {
                let close = self.on_key_target_modal(key, &mut form);
                if !close {
                    self.modal = Modal::AddTarget(form);
                }
                false
            }
            Modal::GenerateKey(mut form) => {
                let close = self.on_key_generate_modal(key, &mut form);
                if !close {
                    self.modal = Modal::GenerateKey(form);
                }
                false
            }
            Modal::ImportKey(mut form) => {
                let close = self.on_key_import_modal(key, &mut form);
                if !close {
                    self.modal = Modal::ImportKey(form);
                }
                false
            }
        }
    }

    fn on_key_main(&mut self, key: KeyEvent) -> bool {
        if self.console_editing && self.tab == Tab::Console {
            return self.on_key_console_edit(key);
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('1') => self.tab = Tab::Keys,
            KeyCode::Char('2') => self.tab = Tab::Targets,
            KeyCode::Char('3') => self.tab = Tab::Exchange,
            KeyCode::Char('4') => self.tab = Tab::Console,
            KeyCode::F(2) => self.cycle_theme(true),
            KeyCode::Left => self.tab = prev_tab(self.tab),
            KeyCode::Right => self.tab = next_tab(self.tab),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('r') if self.tab != Tab::Console => self.refresh_current_tab(),
            _ => self.handle_tab_action(key),
        }
        false
    }

    fn on_key_console_edit(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.console_editing = false;
                self.log("Console input mode disabled.");
            }
            KeyCode::Enter => {
                let command = self.console_input.trim().to_string();
                if command.is_empty() {
                    self.log("Console command is empty.");
                } else {
                    self.run_console_command(&command);
                }
            }
            KeyCode::Backspace => {
                self.console_input.pop();
            }
            KeyCode::Char(c) => {
                self.console_input.push(c);
            }
            _ => {}
        }
        false
    }

    fn on_key_target_modal(&mut self, key: KeyEvent, form: &mut TargetForm) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.log("Add target canceled.");
                return true;
            }
            KeyCode::Tab | KeyCode::Down => form.next_field(),
            KeyCode::BackTab | KeyCode::Up => form.prev_field(),
            KeyCode::Left if form.field == 4 => form.set_auth_kind(AuthFormKind::Password),
            KeyCode::Right if form.field == 4 => form.set_auth_kind(AuthFormKind::KeyFile),
            KeyCode::Char(' ') if form.field == 4 => {
                let next = if form.auth_kind == AuthFormKind::Password {
                    AuthFormKind::KeyFile
                } else {
                    AuthFormKind::Password
                };
                form.set_auth_kind(next);
            }
            KeyCode::Enter => {
                let at_end = form.field + 1 >= form.fields_len();
                if at_end {
                    match form.to_profile() {
                        Ok(profile) => {
                            if self
                                .config
                                .targets
                                .iter()
                                .any(|target| target.name.eq_ignore_ascii_case(&profile.name))
                            {
                                self.log(format!(
                                    "ERROR: target name '{}' already exists.",
                                    profile.name
                                ));
                                return false;
                            }
                            if self.config.targets.iter().any(|target| {
                                target.port == profile.port
                                    && target.user == profile.user
                                    && target.host.eq_ignore_ascii_case(&profile.host)
                            }) {
                                self.log(format!(
                                    "ERROR: target {}@{} already exists.",
                                    profile.user,
                                    profile.endpoint()
                                ));
                                return false;
                            }
                            self.config.targets.push(profile.clone());
                            self.selected_target = self.config.targets.len().saturating_sub(1);
                            self.save_config();
                            self.log(format!(
                                "Added target '{}' ({})",
                                profile.name,
                                profile.endpoint()
                            ));
                            return true;
                        }
                        Err(err) => self.log(format!("ERROR: {err}")),
                    }
                } else {
                    form.next_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(value) = form.active_field_mut() {
                    value.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(value) = form.active_field_mut() {
                    value.push(c);
                }
            }
            _ => {}
        }
        false
    }

    fn on_key_generate_modal(&mut self, key: KeyEvent, form: &mut GenerateKeyForm) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.log("Generate key canceled.");
                return true;
            }
            KeyCode::Tab | KeyCode::Down => form.next_field(),
            KeyCode::BackTab | KeyCode::Up => form.prev_field(),
            KeyCode::Enter => {
                let at_end = form.field + 1 >= form.fields_len();
                if at_end {
                    let pass = if form.passphrase.is_empty() {
                        None
                    } else {
                        Some(form.passphrase.as_str())
                    };
                    match generate_ed25519_key(form.name.trim(), form.comment.trim(), pass) {
                        Ok(new_key) => {
                            self.log(format!(
                                "Generated key '{}' at {}",
                                new_key.name, new_key.public_key_path
                            ));
                            self.refresh_local_keys();
                            return true;
                        }
                        Err(err) => self.log(format!("ERROR: failed to generate key: {err}")),
                    }
                } else {
                    form.next_field();
                }
            }
            KeyCode::Backspace => {
                form.active_field_mut().pop();
            }
            KeyCode::Char(c) => {
                form.active_field_mut().push(c);
            }
            _ => {}
        }
        false
    }

    fn on_key_import_modal(&mut self, key: KeyEvent, form: &mut ImportKeyForm) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.log("Import key canceled.");
                return true;
            }
            KeyCode::Tab | KeyCode::Down => form.next_field(),
            KeyCode::BackTab | KeyCode::Up => form.prev_field(),
            KeyCode::Enter => {
                let at_end = form.field + 1 >= form.fields_len();
                if at_end {
                    let name = form.name.trim();
                    let passphrase = if form.passphrase.is_empty() {
                        None
                    } else {
                        Some(form.passphrase.as_str())
                    };
                    let user_name = if name.is_empty() { None } else { Some(name) };
                    match import_private_key(user_name, form.private_key.trim(), passphrase) {
                        Ok(imported) => {
                            self.log(format!(
                                "Imported key '{}' from {}",
                                imported.name,
                                imported.private_key_path.as_deref().unwrap_or("(unknown)")
                            ));
                            self.refresh_local_keys();
                            return true;
                        }
                        Err(err) => self.log(format!("ERROR: failed to import key: {err}")),
                    }
                } else {
                    form.next_field();
                }
            }
            KeyCode::Backspace => {
                form.active_field_mut().pop();
            }
            KeyCode::Char(c) => {
                form.active_field_mut().push(c);
            }
            _ => {}
        }
        false
    }

    fn refresh_current_tab(&mut self) {
        match self.tab {
            Tab::Keys => self.refresh_local_keys(),
            Tab::Targets => {
                self.log("Target list loaded from local config.");
            }
            Tab::Exchange => {
                self.fetch_remote_keys();
            }
            Tab::Console => {
                self.log("Console ready.");
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match self.tab {
            Tab::Keys => move_index(&mut self.selected_key, self.local_keys.len(), delta),
            Tab::Targets | Tab::Console => {
                move_index(&mut self.selected_target, self.config.targets.len(), delta)
            }
            Tab::Exchange => {
                if !self.remote_keys.is_empty() {
                    move_index(&mut self.selected_remote_key, self.remote_keys.len(), delta);
                } else {
                    move_index(&mut self.selected_target, self.config.targets.len(), delta);
                }
            }
        }
    }

    fn handle_tab_action(&mut self, key: KeyEvent) {
        match self.tab {
            Tab::Keys => self.handle_keys_actions(key),
            Tab::Targets => self.handle_targets_actions(key),
            Tab::Exchange => self.handle_exchange_actions(key),
            Tab::Console => self.handle_console_actions(key),
        }
    }

    fn handle_keys_actions(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('g') => {
                self.modal = Modal::GenerateKey(GenerateKeyForm::default());
                self.log("Generate-key modal opened.");
            }
            KeyCode::Char('i') => {
                self.modal = Modal::ImportKey(ImportKeyForm::default());
                self.log("Import-key modal opened.");
            }
            _ => {}
        }
    }

    fn handle_targets_actions(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('a') => {
                self.modal = Modal::AddTarget(TargetForm::default());
                self.log("Add-target modal opened.");
            }
            KeyCode::Char('d') => {
                if self.config.targets.is_empty() {
                    self.log("No target selected to delete.");
                    return;
                }
                let removed = self.config.targets.remove(self.selected_target);
                self.clamp_selection();
                self.save_config();
                self.log(format!("Deleted target '{}'.", removed.name));
            }
            KeyCode::Char('t') => {
                self.test_selected_target();
            }
            _ => {}
        }
    }

    fn handle_exchange_actions(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('x') => self.exchange_selected_key(),
            KeyCode::Char('f') => self.fetch_remote_keys(),
            _ => {}
        }
    }

    fn handle_console_actions(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('e') => {
                self.console_editing = true;
                self.log("Console input mode enabled (Esc to stop).");
            }
            KeyCode::Enter => {
                self.console_editing = true;
                self.log("Console input mode enabled (Esc to stop).");
            }
            KeyCode::Char('c') => {
                self.console_output.clear();
                self.log("Console output cleared.");
            }
            KeyCode::Char('r') => {
                let command = self.console_input.trim().to_string();
                self.run_console_command(&command);
            }
            _ => {}
        }
    }

    fn test_selected_target(&mut self) {
        let Some(target) = self.selected_target().cloned() else {
            self.log("No target selected.");
            return;
        };

        self.log(format!(
            "Testing SSH link to {}@{}...",
            target.user,
            target.endpoint()
        ));
        match test_connection(&target) {
            Ok(()) => self.log(format!("Link OK: {}", target.endpoint())),
            Err(err) => self.log(format!("ERROR: link failed: {err}")),
        }
    }

    fn exchange_selected_key(&mut self) {
        let Some(target) = self.selected_target().cloned() else {
            self.log("No target selected.");
            return;
        };

        let Some(key) = self.selected_key().cloned() else {
            self.log("No local key selected.");
            return;
        };

        self.log(format!(
            "Exchanging key '{}' to {}@{}...",
            key.name,
            target.user,
            target.endpoint()
        ));

        match exchange_public_key(&target, Path::new(&key.public_key_path)) {
            Ok(()) => {
                self.log("Key exchange completed.");
                self.fetch_remote_keys();
            }
            Err(err) => self.log(format!("ERROR: key exchange failed: {err}")),
        }
    }

    fn fetch_remote_keys(&mut self) {
        let Some(target) = self.selected_target().cloned() else {
            self.log("No target selected.");
            return;
        };

        self.log(format!(
            "Fetching authorized_keys from {}...",
            target.endpoint()
        ));
        match fetch_remote_authorized_keys(&target) {
            Ok(keys) => {
                self.remote_keys = keys;
                self.clamp_selection();
                self.log(format!("Fetched {} remote key(s).", self.remote_keys.len()));
            }
            Err(err) => self.log(format!("ERROR: remote key fetch failed: {err}")),
        }
    }

    fn run_console_command(&mut self, command: &str) {
        let Some(target) = self.selected_target().cloned() else {
            self.log("No target selected.");
            return;
        };

        if command.trim().is_empty() {
            self.log("No command to execute.");
            return;
        }

        self.console_push(format!("$ {}", command));
        self.log(format!(
            "Running remote command on {}@{}...",
            target.user,
            target.endpoint()
        ));

        match run_remote_command(&target, command) {
            Ok(result) => {
                for line in result.stdout.lines() {
                    self.console_push(line.to_string());
                }
                for line in result.stderr.lines() {
                    self.console_push(format!("[stderr] {line}"));
                }
                self.console_push(format!("[exit {}]", result.exit_status));
                self.log("Remote command completed.");
            }
            Err(err) => {
                self.console_push(format!("[error] {err}"));
                self.log(format!("ERROR: remote command failed: {err}"));
            }
        }
    }
}

pub fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_loop(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut app = App::new()?;

    loop {
        terminal.draw(|frame| draw(frame, &app))?;

        if !event::poll(Duration::from_millis(120))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if app.on_key(key) {
                break;
            }
        }
    }

    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    set_active_theme(app.theme);
    let area = frame.area();

    let bg_block = Block::default().style(Style::default().bg(bg()));
    frame.render_widget(bg_block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, layout[0]);
    draw_tabs(frame, layout[1], app.tab);
    draw_status_bar(frame, layout[2], app);

    match app.tab {
        Tab::Keys => draw_keys_tab(frame, layout[3], app),
        Tab::Targets => draw_targets_tab(frame, layout[3], app),
        Tab::Exchange => draw_exchange_tab(frame, layout[3], app),
        Tab::Console => draw_console_tab(frame, layout[3], app),
    }

    draw_log_panel(frame, layout[4], app);
    draw_footer(frame, layout[5], app.tab, app.console_editing);

    match &app.modal {
        Modal::None => {}
        Modal::AddTarget(form) => draw_target_modal(frame, form),
        Modal::GenerateKey(form) => draw_generate_modal(frame, form),
        Modal::ImportKey(form) => draw_import_modal(frame, form),
    }
}

fn draw_header(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let title = Paragraph::new(Text::from(vec![
        Line::from(vec![Span::styled(
            "Cyber Terminal - By Snake",
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "Secure SSH Key Exchange and Remote Ops",
            Style::default().fg(neon_cyan()),
        )]),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(panel_bg()))
            .border_style(Style::default().fg(neon_purple()))
            .border_type(BorderType::Rounded),
    );

    frame.render_widget(title, area);
}

fn draw_tabs(frame: &mut ratatui::Frame<'_>, area: Rect, active: Tab) {
    let titles: Vec<Line<'_>> = Tab::ALL
        .iter()
        .map(|tab| Line::from(Span::styled(tab.title(), Style::default().fg(neon_lime()))))
        .collect();

    let tabs = Tabs::new(titles)
        .select(active.index())
        .style(Style::default().fg(muted_text()).bg(panel_bg()))
        .highlight_style(
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider("  ")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(neon_purple()))
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(panel_bg())),
        );

    frame.render_widget(tabs, area);
}

fn draw_status_bar(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let active_target = app
        .selected_target()
        .map(|t| format!("{}@{}", t.user, t.endpoint()))
        .unwrap_or_else(|| "none".to_string());

    let status = Line::from(vec![
        Span::styled("LOCAL ", Style::default().fg(muted_text())),
        Span::styled(
            app.local_keys.len().to_string(),
            Style::default()
                .fg(neon_cyan())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  TARGETS ", Style::default().fg(muted_text())),
        Span::styled(
            app.config.targets.len().to_string(),
            Style::default()
                .fg(neon_cyan())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  REMOTE KEYS ", Style::default().fg(muted_text())),
        Span::styled(
            app.remote_keys.len().to_string(),
            Style::default()
                .fg(neon_cyan())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ACTIVE ", Style::default().fg(muted_text())),
        Span::styled(
            active_target,
            Style::default()
                .fg(neon_lime())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  THEME ", Style::default().fg(muted_text())),
        Span::styled(
            app.theme.title(),
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let widget = Paragraph::new(status)
        .alignment(Alignment::Center)
        .style(Style::default().bg(panel_bg()))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(neon_purple()))
                .style(Style::default().bg(panel_bg())),
        );
    frame.render_widget(widget, area);
}

fn draw_keys_tab(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let chunks = if area.width < 115 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area)
    };

    let key_items: Vec<ListItem<'_>> = if app.local_keys.is_empty() {
        vec![ListItem::new("No local .pub keys found in ~/.ssh")]
    } else {
        app.local_keys
            .iter()
            .map(|k| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", k.name),
                        Style::default()
                            .fg(neon_cyan())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(k.algorithm.clone(), Style::default().fg(neon_lime())),
                ]))
            })
            .collect()
    };

    let mut key_state = ListState::default();
    if !app.local_keys.is_empty() {
        key_state.select(Some(app.selected_key));
    }

    let keys_list = List::new(key_items)
        .highlight_style(
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD)
                .bg(selection_bg()),
        )
        .block(cyber_block("Local Keys"));

    frame.render_stateful_widget(keys_list, chunks[0], &mut key_state);

    let detail_text = if let Some(key) = app.selected_key() {
        Text::from(vec![
            Line::from(vec![
                Span::styled("Name: ", label_style()),
                Span::styled(&key.name, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Algorithm: ", label_style()),
                Span::styled(&key.algorithm, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Fingerprint: ", label_style()),
                Span::styled(&key.fingerprint, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Comment: ", label_style()),
                Span::styled(
                    if key.comment.is_empty() {
                        "(none)"
                    } else {
                        key.comment.as_str()
                    },
                    value_style(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Public: ", label_style()),
                Span::styled(&key.public_key_path, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Private: ", label_style()),
                Span::styled(
                    key.private_key_path.as_deref().unwrap_or("(missing)"),
                    value_style(),
                ),
            ]),
            Line::raw(""),
            Line::styled(
                "Actions",
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled("g = generate key", Style::default().fg(neon_lime())),
            Line::styled(
                "i = import existing private key",
                Style::default().fg(neon_lime()),
            ),
            Line::styled("r = refresh key list", Style::default().fg(neon_lime())),
        ])
    } else {
        Text::from(vec![
            Line::styled("No key selected.", Style::default().fg(muted_text())),
            Line::raw(""),
            Line::styled(
                "Actions",
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled("g = generate key", Style::default().fg(neon_lime())),
            Line::styled(
                "i = import existing private key",
                Style::default().fg(neon_lime()),
            ),
            Line::styled("r = refresh key list", Style::default().fg(neon_lime())),
        ])
    };

    let details = Paragraph::new(detail_text)
        .wrap(Wrap { trim: false })
        .block(cyber_block("Key Details"));
    frame.render_widget(details, chunks[1]);
}

fn draw_targets_tab(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let chunks = if area.width < 115 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area)
    };

    let target_items: Vec<ListItem<'_>> = if app.config.targets.is_empty() {
        vec![ListItem::new("No targets configured")]
    } else {
        app.config
            .targets
            .iter()
            .map(|t| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", t.name),
                        Style::default()
                            .fg(neon_cyan())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(t.endpoint(), Style::default().fg(neon_lime())),
                ]))
            })
            .collect()
    };

    let mut target_state = ListState::default();
    if !app.config.targets.is_empty() {
        target_state.select(Some(app.selected_target));
    }

    let list = List::new(target_items)
        .highlight_style(
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD)
                .bg(selection_bg()),
        )
        .block(cyber_block("Targets"));

    frame.render_stateful_widget(list, chunks[0], &mut target_state);

    let detail_text = if let Some(target) = app.selected_target() {
        let auth = match &target.auth {
            AuthMethod::Password { .. } => "password",
            AuthMethod::KeyFile { .. } => "key file",
        };

        Text::from(vec![
            Line::from(vec![
                Span::styled("Name: ", label_style()),
                Span::styled(&target.name, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Endpoint: ", label_style()),
                Span::styled(target.endpoint(), value_style()),
            ]),
            Line::from(vec![
                Span::styled("User: ", label_style()),
                Span::styled(&target.user, value_style()),
            ]),
            Line::from(vec![
                Span::styled("Auth: ", label_style()),
                Span::styled(auth, value_style()),
            ]),
            Line::raw(""),
            Line::styled(
                "Actions",
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled("a = add target", Style::default().fg(neon_lime())),
            Line::styled("d = delete target", Style::default().fg(neon_lime())),
            Line::styled("t = test connection", Style::default().fg(neon_lime())),
            Line::styled("r = refresh view", Style::default().fg(neon_lime())),
        ])
    } else {
        Text::from(vec![
            Line::styled("No target selected.", Style::default().fg(muted_text())),
            Line::raw(""),
            Line::styled(
                "Actions",
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled("a = add target", Style::default().fg(neon_lime())),
            Line::styled("d = delete target", Style::default().fg(neon_lime())),
            Line::styled("t = test connection", Style::default().fg(neon_lime())),
        ])
    };

    let details = Paragraph::new(detail_text)
        .wrap(Wrap { trim: false })
        .block(cyber_block("Target Details"));

    frame.render_widget(details, chunks[1]);
}

fn draw_exchange_tab(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let top = if area.width < 120 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0])
    };

    let key_items: Vec<ListItem<'_>> = if app.local_keys.is_empty() {
        vec![ListItem::new("No local keys")]
    } else {
        app.local_keys
            .iter()
            .map(|k| ListItem::new(format!("{} [{}]", k.name, k.algorithm)))
            .collect()
    };

    let mut key_state = ListState::default();
    if !app.local_keys.is_empty() {
        key_state.select(Some(app.selected_key));
    }

    frame.render_stateful_widget(
        List::new(key_items)
            .highlight_style(
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
                    .bg(selection_bg()),
            )
            .block(cyber_block("Local Keys (pick one)")),
        top[0],
        &mut key_state,
    );

    let target_items: Vec<ListItem<'_>> = if app.config.targets.is_empty() {
        vec![ListItem::new("No targets")]
    } else {
        app.config
            .targets
            .iter()
            .map(|t| ListItem::new(format!("{} -> {}@{}", t.name, t.user, t.endpoint())))
            .collect()
    };

    let mut target_state = ListState::default();
    if !app.config.targets.is_empty() {
        target_state.select(Some(app.selected_target));
    }

    frame.render_stateful_widget(
        List::new(target_items)
            .highlight_style(
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
                    .bg(selection_bg()),
            )
            .block(cyber_block("Targets (pick one)")),
        top[1],
        &mut target_state,
    );

    let bottom = if area.width < 120 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(rows[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(74), Constraint::Percentage(26)])
            .split(rows[1])
    };

    let remote_items: Vec<ListItem<'_>> = if app.remote_keys.is_empty() {
        vec![ListItem::new("No remote keys fetched yet (press f)")]
    } else {
        app.remote_keys
            .iter()
            .map(|k| ListItem::new(k.clone()))
            .collect()
    };

    let mut remote_state = ListState::default();
    if !app.remote_keys.is_empty() {
        remote_state.select(Some(app.selected_remote_key));
    }

    let remote_block = List::new(remote_items)
        .block(cyber_block("Remote authorized_keys"))
        .highlight_style(
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD)
                .bg(selection_bg()),
        );

    frame.render_stateful_widget(remote_block, bottom[0], &mut remote_state);

    let selected_key = app
        .selected_key()
        .map(|k| format!("{} [{}]", k.name, k.algorithm))
        .unwrap_or_else(|| "none".to_string());
    let selected_target = app
        .selected_target()
        .map(|t| format!("{}@{}", t.user, t.endpoint()))
        .unwrap_or_else(|| "none".to_string());

    let summary = Paragraph::new(Text::from(vec![
        Line::from(vec![
            Span::styled("Selected Key", label_style()),
            Span::raw(": "),
            Span::styled(selected_key, value_style()),
        ]),
        Line::from(vec![
            Span::styled("Target", label_style()),
            Span::raw(": "),
            Span::styled(selected_target, value_style()),
        ]),
        Line::raw(""),
        Line::styled(
            "Actions",
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled("x = exchange key", Style::default().fg(neon_lime())),
        Line::styled("f = fetch keys", Style::default().fg(neon_lime())),
    ]))
    .wrap(Wrap { trim: false })
    .block(cyber_block("Exchange Status"));
    frame.render_widget(summary, bottom[1]);
}

fn draw_console_tab(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let cols = if area.width < 120 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area)
    };

    let target_items: Vec<ListItem<'_>> = if app.config.targets.is_empty() {
        vec![ListItem::new("No targets")]
    } else {
        app.config
            .targets
            .iter()
            .map(|t| ListItem::new(format!("{}@{}", t.user, t.endpoint())))
            .collect()
    };

    let mut target_state = ListState::default();
    if !app.config.targets.is_empty() {
        target_state.select(Some(app.selected_target));
    }

    let target_list = List::new(target_items)
        .block(cyber_block("Console Target"))
        .highlight_style(
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD)
                .bg(selection_bg()),
        );
    frame.render_stateful_widget(target_list, cols[0], &mut target_state);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(cols[1]);

    let session_state = app
        .selected_target()
        .map(|t| {
            format!(
                "target {}@{} | output lines {}",
                t.user,
                t.endpoint(),
                app.console_output.len()
            )
        })
        .unwrap_or_else(|| "target none | output lines 0".to_string());
    let session_widget = Paragraph::new(session_state)
        .style(Style::default().fg(neon_lime()).bg(panel_bg()))
        .alignment(Alignment::Center)
        .block(cyber_block("Session State"));
    frame.render_widget(session_widget, right[0]);

    let output = if app.console_output.is_empty() {
        "No command output yet.".to_string()
    } else {
        app.console_output
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };

    let output_widget = Paragraph::new(output)
        .block(cyber_block("SSH Command Output"))
        .style(Style::default().fg(neon_cyan()))
        .wrap(Wrap { trim: false });
    frame.render_widget(output_widget, right[1]);

    let input_style = if app.console_editing {
        Style::default().fg(neon_pink()).bg(input_active_bg())
    } else {
        Style::default().fg(muted_text()).bg(panel_bg())
    };

    let input_widget = Paragraph::new(app.console_input.as_str())
        .style(input_style)
        .wrap(Wrap { trim: false })
        .block(cyber_block(
            "Command (e/Enter to edit, Enter or r to run, c clear)",
        ));
    frame.render_widget(input_widget, right[2]);

    let hints = Paragraph::new(if app.console_editing {
        "Editing mode: type command, Enter executes, Esc exits edit mode"
    } else {
        "Select target with Up/Down. Press e or Enter to type, r to run."
    })
    .style(Style::default().fg(neon_lime()))
    .alignment(Alignment::Center);
    frame.render_widget(hints, right[3]);
}

fn draw_log_panel(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let lines: Vec<Line<'_>> = app
        .logs
        .iter()
        .rev()
        .take(6)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| {
            let style = if line.starts_with("ERROR:") {
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
            } else if line.contains("completed")
                || line.contains("Link OK")
                || line.contains("Generated")
            {
                Style::default().fg(neon_lime())
            } else {
                Style::default().fg(muted_text())
            };
            Line::styled(line, style)
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(cyber_block("Event Log"))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut ratatui::Frame<'_>, area: Rect, tab: Tab, console_editing: bool) {
    let text = match tab {
        Tab::Keys => {
            "KEYS  g generate  i import key  r refresh  F2 theme  arrows navigate  1-4 tabs  q quit"
        }
        Tab::Targets => {
            "TARGETS  a add  d delete  t test  F2 theme  arrows navigate  1-4 tabs  q quit"
        }
        Tab::Exchange => {
            "EXCHANGE  x push selected key  f fetch remote keys  F2 theme  arrows navigate  1-4 tabs  q quit"
        }
        Tab::Console => {
            if console_editing {
                "CONSOLE  typing mode active  Enter run  Esc stop editing  F2 theme  q quit"
            } else {
                "CONSOLE  e edit command  r run command  c clear output  F2 theme  arrows pick target  q quit"
            }
        }
    };

    let footer = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(neon_lime()).bg(panel_bg()));

    frame.render_widget(footer, area);
}

fn draw_target_modal(frame: &mut ratatui::Frame<'_>, form: &TargetForm) {
    let area = centered_rect(76, 70, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        cyber_block("Add Target").style(Style::default().bg(panel_bg())),
        area,
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .margin(1)
        .split(area);

    let lines = form
        .lines()
        .into_iter()
        .enumerate()
        .map(|(idx, (label, value, _secret))| {
            let highlight = idx == form.field;
            let label_style = if highlight {
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(neon_cyan())
            };
            let value_style = if highlight {
                Style::default()
                    .fg(neon_lime())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted_text())
            };
            Line::from(vec![
                Span::styled(format!("{label:<24}"), label_style),
                Span::styled(value, value_style),
            ])
        })
        .collect::<Vec<_>>();

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(body, inner[0]);

    let hint = Paragraph::new(
        "Tab/Up/Down move fields | Left/Right on Auth changes mode | Enter next/save | Esc cancel",
    )
    .style(Style::default().fg(neon_lime()))
    .alignment(Alignment::Center);
    frame.render_widget(hint, inner[1]);
}

fn draw_generate_modal(frame: &mut ratatui::Frame<'_>, form: &GenerateKeyForm) {
    let area = centered_rect(68, 58, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        cyber_block("Generate New Key").style(Style::default().bg(panel_bg())),
        area,
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .margin(1)
        .split(area);

    let lines = form
        .lines()
        .into_iter()
        .enumerate()
        .map(|(idx, (label, value, _secret))| {
            let highlight = idx == form.field;
            let label_style = if highlight {
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(neon_cyan())
            };
            let value_style = if highlight {
                Style::default()
                    .fg(neon_lime())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted_text())
            };
            Line::from(vec![
                Span::styled(format!("{label:<24}"), label_style),
                Span::styled(value, value_style),
            ])
        })
        .collect::<Vec<_>>();

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(body, inner[0]);

    let hint = Paragraph::new("Tab/Up/Down move fields | Enter next/save | Esc cancel")
        .style(Style::default().fg(neon_lime()))
        .alignment(Alignment::Center);
    frame.render_widget(hint, inner[1]);
}

fn draw_import_modal(frame: &mut ratatui::Frame<'_>, form: &ImportKeyForm) {
    let area = centered_rect(72, 62, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        cyber_block("Import Existing Key").style(Style::default().bg(panel_bg())),
        area,
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .margin(1)
        .split(area);

    let lines = form
        .lines()
        .into_iter()
        .enumerate()
        .map(|(idx, (label, value, _secret))| {
            let highlight = idx == form.field;
            let label_style = if highlight {
                Style::default()
                    .fg(neon_pink())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(neon_cyan())
            };
            let value_style = if highlight {
                Style::default()
                    .fg(neon_lime())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted_text())
            };
            Line::from(vec![
                Span::styled(format!("{label:<24}"), label_style),
                Span::styled(value, value_style),
            ])
        })
        .collect::<Vec<_>>();

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(body, inner[0]);

    let hint = Paragraph::new("Tab/Up/Down move fields | Enter next/save | Esc cancel")
        .style(Style::default().fg(neon_lime()))
        .alignment(Alignment::Center);
    frame.render_widget(hint, inner[1]);
}

fn cyber_block(title: &str) -> Block<'_> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(neon_pink())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(neon_cyan()))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(panel_bg()))
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn move_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }

    if delta < 0 {
        if *index == 0 {
            *index = len - 1;
        } else {
            *index -= 1;
        }
    } else if delta > 0 {
        *index = (*index + 1) % len;
    }
}

fn next_tab(tab: Tab) -> Tab {
    Tab::ALL[(tab.index() + 1) % Tab::ALL.len()]
}

fn prev_tab(tab: Tab) -> Tab {
    if tab.index() == 0 {
        Tab::ALL[Tab::ALL.len() - 1]
    } else {
        Tab::ALL[tab.index() - 1]
    }
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        "".to_string()
    } else {
        "*".repeat(value.chars().count())
    }
}

#[derive(Clone, Copy)]
struct ThemePalette {
    bg: Color,
    panel_bg: Color,
    accent_primary: Color,
    accent_secondary: Color,
    accent_success: Color,
    accent_border: Color,
    text_muted: Color,
    selection_bg: Color,
    input_active_bg: Color,
}

fn theme_store() -> &'static RwLock<Theme> {
    static STORE: OnceLock<RwLock<Theme>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(Theme::Cyberpunk))
}

fn set_active_theme(theme: Theme) {
    if let Ok(mut guard) = theme_store().write() {
        *guard = theme;
    }
}

fn active_theme() -> Theme {
    if let Ok(guard) = theme_store().read() {
        *guard
    } else {
        Theme::Cyberpunk
    }
}

fn palette() -> ThemePalette {
    match active_theme() {
        Theme::Cyberpunk => ThemePalette {
            bg: Color::Rgb(8, 8, 14),
            panel_bg: Color::Rgb(15, 18, 26),
            accent_primary: Color::Rgb(255, 33, 156),
            accent_secondary: Color::Rgb(0, 245, 255),
            accent_success: Color::Rgb(132, 255, 0),
            accent_border: Color::Rgb(167, 77, 255),
            text_muted: Color::Rgb(165, 178, 198),
            selection_bg: Color::Rgb(27, 14, 39),
            input_active_bg: Color::Rgb(31, 19, 45),
        },
        Theme::Synth => ThemePalette {
            bg: Color::Rgb(14, 8, 24),
            panel_bg: Color::Rgb(24, 14, 36),
            accent_primary: Color::Rgb(255, 109, 245),
            accent_secondary: Color::Rgb(109, 225, 255),
            accent_success: Color::Rgb(255, 196, 81),
            accent_border: Color::Rgb(173, 120, 255),
            text_muted: Color::Rgb(206, 197, 235),
            selection_bg: Color::Rgb(40, 24, 59),
            input_active_bg: Color::Rgb(52, 31, 74),
        },
        Theme::Matrix => ThemePalette {
            bg: Color::Rgb(3, 10, 4),
            panel_bg: Color::Rgb(9, 19, 10),
            accent_primary: Color::Rgb(97, 255, 132),
            accent_secondary: Color::Rgb(46, 202, 92),
            accent_success: Color::Rgb(182, 255, 102),
            accent_border: Color::Rgb(36, 148, 70),
            text_muted: Color::Rgb(140, 191, 151),
            selection_bg: Color::Rgb(18, 39, 22),
            input_active_bg: Color::Rgb(23, 51, 29),
        },
        Theme::Ember => ThemePalette {
            bg: Color::Rgb(20, 10, 8),
            panel_bg: Color::Rgb(30, 16, 12),
            accent_primary: Color::Rgb(255, 120, 78),
            accent_secondary: Color::Rgb(255, 181, 96),
            accent_success: Color::Rgb(255, 229, 122),
            accent_border: Color::Rgb(214, 94, 58),
            text_muted: Color::Rgb(227, 188, 168),
            selection_bg: Color::Rgb(52, 25, 18),
            input_active_bg: Color::Rgb(61, 29, 21),
        },
        Theme::Glacier => ThemePalette {
            bg: Color::Rgb(7, 13, 22),
            panel_bg: Color::Rgb(16, 26, 39),
            accent_primary: Color::Rgb(141, 214, 255),
            accent_secondary: Color::Rgb(104, 184, 255),
            accent_success: Color::Rgb(131, 243, 211),
            accent_border: Color::Rgb(88, 139, 214),
            text_muted: Color::Rgb(187, 210, 232),
            selection_bg: Color::Rgb(28, 43, 64),
            input_active_bg: Color::Rgb(37, 56, 81),
        },
    }
}

fn bg() -> Color {
    palette().bg
}

fn panel_bg() -> Color {
    palette().panel_bg
}

fn neon_cyan() -> Color {
    palette().accent_secondary
}

fn neon_pink() -> Color {
    palette().accent_primary
}

fn neon_lime() -> Color {
    palette().accent_success
}

fn neon_purple() -> Color {
    palette().accent_border
}

fn muted_text() -> Color {
    palette().text_muted
}

fn selection_bg() -> Color {
    palette().selection_bg
}

fn input_active_bg() -> Color {
    palette().input_active_bg
}

fn label_style() -> Style {
    Style::default()
        .fg(neon_pink())
        .add_modifier(Modifier::BOLD)
}

fn value_style() -> Style {
    Style::default().fg(neon_cyan())
}

#[cfg(test)]
mod tests {
    use super::{AuthFormKind, TargetForm, move_index};

    #[test]
    fn target_form_rejects_whitespace_in_host_or_user() {
        let mut form = TargetForm {
            name: "dev".to_string(),
            host: "bad host".to_string(),
            port: "22".to_string(),
            user: "tester".to_string(),
            ..Default::default()
        };
        assert!(form.to_profile().is_err());

        form.host = "127.0.0.1".to_string();
        form.user = "bad user".to_string();
        assert!(form.to_profile().is_err());
    }

    #[test]
    fn target_form_builds_key_auth_profile() {
        let form = TargetForm {
            name: "dev".to_string(),
            host: "127.0.0.1".to_string(),
            port: "22".to_string(),
            user: "tester".to_string(),
            auth_kind: AuthFormKind::KeyFile,
            private_key: "~/.ssh/id_ed25519".to_string(),
            passphrase: String::new(),
            ..Default::default()
        };
        let profile = form.to_profile().expect("profile");
        assert_eq!(profile.endpoint(), "127.0.0.1:22");
    }

    #[test]
    fn move_index_wraps_at_edges() {
        let mut index = 0;
        move_index(&mut index, 3, -1);
        assert_eq!(index, 2);

        move_index(&mut index, 3, 1);
        assert_eq!(index, 0);
    }
}
