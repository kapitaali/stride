//! Editor configuration: `editor.toml` parsing with sane defaults.
//!
//! The defaults mirror the RIDE gateway convention (localhost:4502) and
//! point at the sibling rust-apl build outputs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Full editor configuration (all fields optional in the file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Host of the interpreter gateway (default `127.0.0.1`).
    #[serde(default = "default_host")]
    pub gateway_host: String,
    /// Port of the interpreter gateway (RIDE default `4502`).
    #[serde(default = "default_port")]
    pub gateway_port: u16,
    /// Path to the interpreter executable to run if gateway is not listening.
    #[serde(default = "default_gateway_executable")]
    pub gateway_executable: String,
    /// Arguments to pass to the gateway executable (default `--serve`).
    #[serde(default = "default_gateway_args")]
    pub gateway_args: String,
    /// Environment variables to set when spawning the gateway (e.g., RIDE_INIT).
    #[serde(default)]
    pub gateway_env: Vec<String>,
    /// Version string shown in the status bar.
    #[serde(default = "default_apl_version")]
    pub apl_version: String,
    /// Connect to the gateway automatically at startup.
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    /// Maximum result-pane lines kept in memory.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    4502
}
fn default_gateway_executable() -> String {
    "../rust-apl/target/debug/apl".to_string()
}
fn default_gateway_args() -> String {
    "--serve".to_string()
}
fn default_apl_version() -> String {
    "GNU APL 2.0 (Rust)".to_string()
}
fn default_true() -> bool {
    true
}
fn default_max_results() -> usize {
    500
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            gateway_host: default_host(),
            gateway_port: default_port(),
            gateway_executable: default_gateway_executable(),
            gateway_args: default_gateway_args(),
            gateway_env: Vec::new(),
            apl_version: default_apl_version(),
            auto_connect: default_true(),
            max_results: default_max_results(),
        }
    }
}

impl EditorConfig {
    /// Socket address `host:port` for the gateway.
    pub fn gateway_addr(&self) -> String {
        format!("{}:{}", self.gateway_host, self.gateway_port)
    }

    /// Load from `path`. A missing file yields defaults (not an error);
    /// a present-but-unparsable file is an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))
    }

    /// Default config file location: `editor.toml` in the working directory.
    pub fn default_path() -> PathBuf {
        PathBuf::from("editor.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_ride_convention() {
        let c = EditorConfig::default();
        assert_eq!(c.gateway_port, 4502);
        assert_eq!(c.gateway_addr(), "127.0.0.1:4502");
        assert!(!c.apl_version.is_empty());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let c = EditorConfig::load(Path::new("/nonexistent/editor.toml")).unwrap();
        assert_eq!(c.gateway_port, 4502);
    }

    #[test]
    fn partial_file_fills_rest_with_defaults() {
        let dir = std::env::temp_dir();
        let p = dir.join("apl_editor_test_partial.toml");
        std::fs::write(&p, "gateway_port = 4503\n").unwrap();
        let c = EditorConfig::load(&p).unwrap();
        assert_eq!(c.gateway_port, 4503);
        assert_eq!(c.gateway_host, "127.0.0.1");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn bad_toml_is_an_error() {
        let dir = std::env::temp_dir();
        let p = dir.join("apl_editor_test_bad.toml");
        std::fs::write(&p, "gateway_port = [\n").unwrap();
        assert!(EditorConfig::load(&p).is_err());
        std::fs::remove_file(&p).ok();
    }
}
