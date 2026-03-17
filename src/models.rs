use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthMethod {
    Password {
        password: String,
    },
    KeyFile {
        private_key: String,
        passphrase: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
}

impl TargetProfile {
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub targets: Vec<TargetProfile>,
    #[serde(default)]
    pub theme: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LocalKey {
    pub name: String,
    pub algorithm: String,
    pub comment: String,
    pub fingerprint: String,
    pub public_key_path: String,
    pub private_key_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: i32,
}
