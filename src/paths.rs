//! Self-resolution of Herdr plugin directories (spec section 13).

use std::env;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const PLUGIN_ID: &str = "caioniehues.agent-team";

#[derive(Debug, Error)]
pub enum PathError {
    #[error("cannot resolve {name}: set {name} or HOME (XDG overrides are supported)")]
    Unresolved { name: &'static str },
}

pub fn state_dir() -> Result<PathBuf, PathError> {
    resolve_dir(
        "HERDR_PLUGIN_STATE_DIR",
        "XDG_STATE_HOME",
        ".local/state",
        Path::new("herdr/plugins").join(PLUGIN_ID),
    )
}

pub fn config_dir() -> Result<PathBuf, PathError> {
    resolve_dir(
        "HERDR_PLUGIN_CONFIG_DIR",
        "XDG_CONFIG_HOME",
        ".config",
        Path::new("herdr/plugins/config").join(PLUGIN_ID),
    )
}

/// Populate the same variables Herdr injects for direct PATH invocation.
pub fn hydrate_environment() {
    if env::var_os("HERDR_PLUGIN_STATE_DIR").is_none() {
        if let Ok(path) = state_dir() {
            env::set_var("HERDR_PLUGIN_STATE_DIR", path);
        }
    }
    if env::var_os("HERDR_PLUGIN_CONFIG_DIR").is_none() {
        if let Ok(path) = config_dir() {
            env::set_var("HERDR_PLUGIN_CONFIG_DIR", path);
        }
    }
}

fn resolve_dir(
    explicit: &'static str,
    xdg: &str,
    home_suffix: &str,
    herdr_suffix: PathBuf,
) -> Result<PathBuf, PathError> {
    if let Some(path) = env::var_os(explicit) {
        return Ok(PathBuf::from(path));
    }
    let base = env::var_os(xdg)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(home_suffix)))
        .ok_or(PathError::Unresolved { name: explicit })?;
    Ok(base.join(herdr_suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn explicit_environment_wins_and_well_known_layout_is_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        env::set_var("HERDR_PLUGIN_STATE_DIR", "/explicit/state");
        env::set_var("XDG_STATE_HOME", "/xdg/state");
        assert_eq!(state_dir().unwrap(), PathBuf::from("/explicit/state"));
        env::remove_var("HERDR_PLUGIN_STATE_DIR");
        assert_eq!(
            state_dir().unwrap(),
            PathBuf::from("/xdg/state/herdr/plugins").join(PLUGIN_ID)
        );
        env::remove_var("XDG_STATE_HOME");
    }

    #[test]
    fn missing_environment_has_a_clear_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved = ["HERDR_PLUGIN_CONFIG_DIR", "XDG_CONFIG_HOME", "HOME"]
            .map(|key| (key, env::var_os(key)));
        for (key, _) in &saved {
            env::remove_var(key);
        }
        assert!(config_dir()
            .unwrap_err()
            .to_string()
            .contains("HERDR_PLUGIN_CONFIG_DIR"));
        for (key, value) in saved {
            if let Some(value) = value {
                env::set_var(key, value);
            }
        }
    }
}
