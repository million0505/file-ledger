use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub algorithm: String,
    pub generated_unix_seconds: u64,
    pub files: Vec<Entry>,
    pub skipped: Vec<Notice>,
    pub errors: Vec<Notice>,
}

pub fn portable_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && !path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != 1 || self.algorithm != "sha256" {
            return Err("unsupported manifest format or hash algorithm".into());
        }
        let mut paths = BTreeSet::new();
        for file in &self.files {
            if !portable_relative(&file.path) || !paths.insert(&file.path) {
                return Err("manifest contains an unsafe or repeated path".into());
            }
            if file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err("manifest contains a noncanonical SHA-256 digest".into());
            }
        }
        Ok(())
    }
}
