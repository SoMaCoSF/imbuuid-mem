// file_id: SOM-DOC-10434 (part)
// name: src/config.rs
// description: ~/.imbuuid/tui-config.json — persists UI density (F6) across runs.
// project_id: IMBUUID-MEM
// category: tui
// created: 2026-10-17
// version: 1.0.0

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    Comfortable,
}

impl Density {
    pub fn toggle(&self) -> Density {
        match self {
            Density::Compact => Density::Comfortable,
            Density::Comfortable => Density::Compact,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Comfortable => "comfortable",
        }
    }

    /// Table row height: compact = 1 line, comfortable = 2 lines (namespace subline).
    pub fn row_height(&self) -> u16 {
        match self {
            Density::Compact => 1,
            Density::Comfortable => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_density")]
    pub density: Density,
}

fn default_density() -> Density {
    Density::Compact
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            density: Density::Compact,
        }
    }
}

pub fn config_path(base: &Path) -> std::path::PathBuf {
    base.join("tui-config.json")
}

pub fn load(base: &Path) -> TuiConfig {
    let p = config_path(base);
    if !p.exists() {
        return TuiConfig::default();
    }
    match std::fs::read_to_string(&p) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => TuiConfig::default(),
    }
}

/// Save is best-effort: a config write failure must never crash the TUI.
pub fn save(base: &Path, cfg: &TuiConfig) -> Result<(), String> {
    let p = config_path(base);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_base(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("imbuuid-tui-cfgtest-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn density_toggle_cycles() {
        assert_eq!(Density::Compact.toggle(), Density::Comfortable);
        assert_eq!(Density::Comfortable.toggle(), Density::Compact);
        assert_eq!(Density::Compact.row_height(), 1);
        assert_eq!(Density::Comfortable.row_height(), 2);
    }

    #[test]
    fn config_roundtrip_through_temp_dir() {
        let base = tmp_base("roundtrip");
        let cfg = TuiConfig { density: Density::Comfortable };
        save(&base, &cfg).unwrap();
        let back = load(&base);
        assert_eq!(back.density, Density::Comfortable);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_config_returns_default() {
        let base = tmp_base("missing");
        let cfg = load(&base);
        assert_eq!(cfg.density, Density::Compact);
    }
}
