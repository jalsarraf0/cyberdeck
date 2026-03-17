use std::fmt;
use std::fs;
use std::time::SystemTime;

use crate::models::LocalKey;

/// Severity of a health finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Severity {
    Good,
    Info,
    Warn,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Good => write!(f, "GOOD"),
            Severity::Info => write!(f, "INFO"),
            Severity::Warn => write!(f, "WARN"),
            Severity::Critical => write!(f, "CRIT"),
        }
    }
}

/// A single finding from a key health audit.
#[derive(Clone, Debug)]
pub struct Finding {
    pub severity: Severity,
    pub key_name: String,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.key_name, self.message)
    }
}

/// Audit a set of local keys and return findings sorted by severity (worst first).
pub fn audit_keys(keys: &[LocalKey]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for key in keys {
        audit_algorithm(key, &mut findings);
        audit_age(key, &mut findings);
        audit_private_key(key, &mut findings);
    }

    if keys.is_empty() {
        findings.push(Finding {
            severity: Severity::Info,
            key_name: "(none)".to_string(),
            message: "No local SSH keys found. Generate one with 'g' in the Keys tab.".to_string(),
        });
    }

    // Sort by severity descending (Critical first)
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));
    findings
}

fn audit_algorithm(key: &LocalKey, findings: &mut Vec<Finding>) {
    let algo_lower = key.algorithm.to_ascii_lowercase();

    if (algo_lower.contains("dsa") || algo_lower.contains("dss")) && !algo_lower.contains("ecdsa") {
        findings.push(Finding {
            severity: Severity::Critical,
            key_name: key.name.clone(),
            message: format!(
                "DSA key detected ({}). DSA is deprecated and considered insecure. \
                 Generate an ed25519 key as replacement.",
                key.algorithm
            ),
        });
    } else if algo_lower.contains("rsa") {
        // Try to extract key size from fingerprint output (ssh-keygen -l shows "BITS HASH ...")
        let severity = estimate_rsa_severity(&key.fingerprint);
        let msg = match severity {
            Severity::Critical => format!(
                "RSA key with potentially weak key size ({}). \
                 RSA keys should be at least 3072 bits. Prefer ed25519.",
                key.algorithm
            ),
            Severity::Warn => format!(
                "RSA key ({}). Consider migrating to ed25519 for better security and performance.",
                key.algorithm
            ),
            _ => format!(
                "RSA key ({}) — acceptable but ed25519 is preferred.",
                key.algorithm
            ),
        };
        findings.push(Finding {
            severity,
            key_name: key.name.clone(),
            message: msg,
        });
    } else if algo_lower.contains("ecdsa") {
        findings.push(Finding {
            severity: Severity::Info,
            key_name: key.name.clone(),
            message: format!(
                "ECDSA key ({}). Acceptable, though ed25519 is generally preferred.",
                key.algorithm
            ),
        });
    } else if algo_lower.contains("ed25519") {
        findings.push(Finding {
            severity: Severity::Good,
            key_name: key.name.clone(),
            message: format!(
                "ed25519 key ({}) — excellent choice. Modern, fast, and secure.",
                key.algorithm
            ),
        });
    } else {
        findings.push(Finding {
            severity: Severity::Warn,
            key_name: key.name.clone(),
            message: format!(
                "Unknown key algorithm: {}. Verify this is acceptable.",
                key.algorithm
            ),
        });
    }
}

fn estimate_rsa_severity(fingerprint: &str) -> Severity {
    // The fingerprint string from ssh-keygen -l is "BITS HASH COMMENT (TYPE)"
    // We don't have direct access to bit size here, so we use a heuristic:
    // If fingerprint is "-" (failed to compute), assume warning.
    if fingerprint == "-" {
        return Severity::Warn;
    }
    // We can't reliably determine RSA key size from just the fingerprint,
    // so default to Warn to encourage migration to ed25519.
    Severity::Warn
}

fn audit_age(key: &LocalKey, findings: &mut Vec<Finding>) {
    let path = std::path::Path::new(&key.public_key_path);
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };

    let Ok(modified) = metadata.modified() else {
        return;
    };

    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return;
    };

    let days = age.as_secs() / 86400;

    if days > 730 {
        findings.push(Finding {
            severity: Severity::Warn,
            key_name: key.name.clone(),
            message: format!(
                "Key is ~{} days old (>{} years). Consider rotating for security hygiene.",
                days,
                days / 365
            ),
        });
    } else if days > 365 {
        findings.push(Finding {
            severity: Severity::Info,
            key_name: key.name.clone(),
            message: format!(
                "Key is ~{} days old (>1 year). Rotation recommended annually.",
                days
            ),
        });
    }
}

fn audit_private_key(key: &LocalKey, findings: &mut Vec<Finding>) {
    if key.private_key_path.is_none() {
        findings.push(Finding {
            severity: Severity::Warn,
            key_name: key.name.clone(),
            message: "Public key exists but no matching private key found. \
                      This key cannot be used for authentication from this host."
                .to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LocalKey;

    fn test_key(name: &str, algorithm: &str) -> LocalKey {
        LocalKey {
            name: name.to_string(),
            algorithm: algorithm.to_string(),
            comment: String::new(),
            fingerprint: "-".to_string(),
            public_key_path: "/tmp/nonexistent.pub".to_string(),
            private_key_path: Some("/tmp/nonexistent".to_string()),
        }
    }

    #[test]
    fn ed25519_rated_good() {
        let key = test_key("mykey", "ssh-ed25519");
        let findings = audit_keys(&[key]);
        assert!(findings.iter().any(|f| f.severity == Severity::Good));
    }

    #[test]
    fn dsa_rated_critical() {
        let key = test_key("oldkey", "ssh-dss");
        let findings = audit_keys(&[key]);
        assert!(findings.iter().any(|f| f.severity == Severity::Critical));
    }

    #[test]
    fn rsa_rated_warn() {
        let key = test_key("rsaKey", "ssh-rsa");
        let findings = audit_keys(&[key]);
        assert!(findings.iter().any(|f| f.severity == Severity::Warn));
    }

    #[test]
    fn orphan_public_key_warned() {
        let key = LocalKey {
            name: "orphan".to_string(),
            algorithm: "ssh-ed25519".to_string(),
            comment: String::new(),
            fingerprint: "-".to_string(),
            public_key_path: "/tmp/nonexistent.pub".to_string(),
            private_key_path: None,
        };
        let findings = audit_keys(&[key]);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("no matching private key"))
        );
    }

    #[test]
    fn empty_keys_gives_info() {
        let findings = audit_keys(&[]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
    }
}
