use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::VeyraError;

/// Resolved XDG Base Directory paths for a single Veyra application instance.
///
/// Follows the freedesktop.org XDG Base Directory Specification:
/// `XDG_CONFIG_HOME` (default `~/.config`), `XDG_CACHE_HOME` (default `~/.cache`),
/// `XDG_STATE_HOME` (default `~/.local/state`) and `XDG_DATA_HOME` (default `~/.local/share`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirs {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl XdgDirs {
    /// Resolves XDG directories for `app_name`, reading overrides from the environment.
    pub fn resolve(app_name: &str) -> Result<Self, VeyraError> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(VeyraError::MissingHomeDirectory)?;

        Ok(Self {
            config_dir: resolve_dir(env::var("XDG_CONFIG_HOME").ok(), &home, ".config", app_name),
            cache_dir: resolve_dir(env::var("XDG_CACHE_HOME").ok(), &home, ".cache", app_name),
            state_dir: resolve_dir(
                env::var("XDG_STATE_HOME").ok(),
                &home,
                ".local/state",
                app_name,
            ),
            data_dir: resolve_dir(
                env::var("XDG_DATA_HOME").ok(),
                &home,
                ".local/share",
                app_name,
            ),
        })
    }

    /// Creates all four directories on disk if they do not already exist.
    pub fn ensure_created(&self) -> Result<(), VeyraError> {
        for dir in [
            &self.config_dir,
            &self.cache_dir,
            &self.state_dir,
            &self.data_dir,
        ] {
            fs::create_dir_all(dir).map_err(|source| VeyraError::DirectoryCreation {
                path: dir.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// Path to the structured log file under the state directory.
    pub fn log_file(&self) -> PathBuf {
        self.state_dir.join("veyra.log")
    }

    /// Directory Faz 53's crash reports are written to and read back from.
    pub fn crashes_dir(&self) -> PathBuf {
        self.state_dir.join("crashes")
    }
}

/// Resolves a single XDG directory: `$ENV_VALUE/app_name` if the env var is set and
/// non-empty, otherwise `home/fallback_rel/app_name`. Pure function, kept separate
/// from live `std::env` access so it can be unit tested deterministically.
fn resolve_dir(
    env_value: Option<String>,
    home: &Path,
    fallback_rel: &str,
    app_name: &str,
) -> PathBuf {
    let base = match env_value {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => home.join(fallback_rel),
    };
    base.join(app_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_env_override_when_set() {
        let home = Path::new("/home/testuser");
        let resolved = resolve_dir(Some("/custom/config".to_string()), home, ".config", "veyra");
        assert_eq!(resolved, PathBuf::from("/custom/config/veyra"));
    }

    #[test]
    fn falls_back_to_home_relative_path_when_unset() {
        let home = Path::new("/home/testuser");
        let resolved = resolve_dir(None, home, ".config", "veyra");
        assert_eq!(resolved, PathBuf::from("/home/testuser/.config/veyra"));
    }

    #[test]
    fn falls_back_when_env_value_is_empty_string() {
        let home = Path::new("/home/testuser");
        let resolved = resolve_dir(Some(String::new()), home, ".cache", "veyra");
        assert_eq!(resolved, PathBuf::from("/home/testuser/.cache/veyra"));
    }

    #[test]
    fn log_file_lives_under_state_dir() {
        let dirs = XdgDirs {
            config_dir: PathBuf::from("/x/config"),
            cache_dir: PathBuf::from("/x/cache"),
            state_dir: PathBuf::from("/x/state"),
            data_dir: PathBuf::from("/x/data"),
        };
        assert_eq!(dirs.log_file(), PathBuf::from("/x/state/veyra.log"));
    }

    #[test]
    fn crashes_dir_lives_under_state_dir() {
        let dirs = XdgDirs {
            config_dir: PathBuf::from("/x/config"),
            cache_dir: PathBuf::from("/x/cache"),
            state_dir: PathBuf::from("/x/state"),
            data_dir: PathBuf::from("/x/data"),
        };
        assert_eq!(dirs.crashes_dir(), PathBuf::from("/x/state/crashes"));
    }
}
