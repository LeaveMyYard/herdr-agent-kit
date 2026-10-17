//! `config.toml` in the plugin's config directory, re-read when it changes.
//!
//! The directory is `$HERDR_PLUGIN_CONFIG_DIR` (herdr sets it for plugin
//! commands), else what `herdr plugin config-dir <id>` prints. The second needs
//! herdr, so the caller passes it in as a fallback (from
//! [`crate::herdr_api`]); it is asked at most once per [`ConfigFile`], as the
//! Python plugins cache it.
//!
//! [`ConfigFile::load_as`] deserializes the settings into any serde type: a
//! plugin's own settings struct, or `serde_json::Map<String, serde_json::Value>`
//! for a loose view. No `toml` type is part of the API.
//!
//! Loading re-parses only when the file's modification time, size
//! or inode changed, so calling it on every use is cheap and an edit applies
//! without a restart (panorama `config.load`; the inbox re-reads every call,
//! which this matches). A missing file is an empty table. A file that does not
//! parse is an empty table too, with the error kept in
//! [`ConfigFile::error`] (until the file changes again) for the caller to log.
//!
//! The file is parsed as TOML 1.0, as Python's `tomllib`
//! does: a file that uses TOML 1.1 (a trailing comma in an inline table, say)
//! is ignored whole, here as in the Python plugins, with the same
//! `ignoring <path>: <reason>` diagnostic.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::de::DeserializeOwned;
use toml::Table;

use super::PluginEnv;

/// The file name in the config directory.
pub const FILE_NAME: &str = "config.toml";

/// The plugin config directory: `$HERDR_PLUGIN_CONFIG_DIR`, else
/// `fallback()` (herdr's `plugin config-dir <id>`; `None` or an empty path
/// when herdr cannot say).
pub fn config_dir(env: &PluginEnv, fallback: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    match env.plugin_config_dir() {
        Some(dir) => Some(dir.to_path_buf()),
        None => fallback().filter(|d| !d.as_os_str().is_empty()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    mtime: Option<SystemTime>,
    len: u64,
    ino: u64,
    dev: u64,
}

/// One plugin's `config.toml`, cached until it changes.
#[derive(Clone, Debug, Default)]
pub struct ConfigFile {
    path: Option<PathBuf>,
    stamp: Option<Stamp>,
    table: Table,
    error: Option<String>,
}

impl ConfigFile {
    /// `config.toml` in `dir`.
    pub fn in_dir(dir: impl AsRef<Path>) -> Self {
        ConfigFile {
            path: Some(dir.as_ref().join(FILE_NAME)),
            ..ConfigFile::default()
        }
    }

    /// `config.toml` in [`config_dir`], or a config that is always empty when
    /// there is no config directory.
    pub fn resolve(env: &PluginEnv, fallback: impl FnOnce() -> Option<PathBuf>) -> Self {
        match config_dir(env, fallback) {
            Some(dir) => ConfigFile::in_dir(dir),
            None => ConfigFile::default(),
        }
    }

    /// The file's path (`None`: no config directory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The settings as a `T`: the file's top-level table, re-read if the file
    /// changed since the last call, deserialized with serde. The table is
    /// empty when the file is missing or broken ([`ConfigFile::error`] says
    /// why), so a `T` whose fields all have defaults always loads. `Err` is
    /// the deserialization error (a setting of the wrong type, say).
    pub fn load_as<T: DeserializeOwned>(&mut self) -> Result<T, String> {
        toml::Value::Table(self.load().clone())
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())
    }

    /// The file's top-level table, re-read if the file changed since the last
    /// call. Empty when the file is missing or broken.
    pub(crate) fn load(&mut self) -> &Table {
        let Some(path) = &self.path else {
            return &self.table;
        };
        let stamp = match fs::metadata(path) {
            Ok(meta) => Stamp {
                mtime: meta.modified().ok(),
                len: meta.len(),
                ino: meta.ino(),
                dev: meta.dev(),
            },
            Err(_) => {
                self.stamp = None;
                self.table.clear();
                self.error = None;
                return &self.table;
            }
        };
        if self.stamp.as_ref() != Some(&stamp) {
            let parsed = fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|text| super::toml10::parse(&text));
            match parsed {
                Ok(table) => {
                    self.table = table;
                    self.error = None;
                }
                Err(e) => {
                    self.table.clear();
                    self.error = Some(format!("ignoring {}: {e}", path.display()));
                }
            }
            self.stamp = Some(stamp);
        }
        &self.table
    }

    /// Why the last load ignored the file, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_env::vars::HERDR_PLUGIN_CONFIG_DIR;
    use std::cell::Cell;

    fn env(vars: &[(&str, &str)]) -> PluginEnv {
        PluginEnv::from_vars(vars.iter().map(|(k, v)| (*k, *v)), None, 1000)
    }

    #[test]
    fn the_env_wins_and_the_fallback_is_not_asked() {
        let asked = Cell::new(false);
        let dir = config_dir(&env(&[(HERDR_PLUGIN_CONFIG_DIR, "/c")]), || {
            asked.set(true);
            None
        });
        assert_eq!(dir, Some(PathBuf::from("/c")));
        assert!(!asked.get());
        let dir = config_dir(&env(&[(HERDR_PLUGIN_CONFIG_DIR, "")]), || {
            Some(PathBuf::from("/h"))
        });
        assert_eq!(dir, Some(PathBuf::from("/h")));
        assert_eq!(config_dir(&env(&[]), || Some(PathBuf::new())), None);
        assert_eq!(config_dir(&env(&[]), || None), None);
    }

    #[test]
    fn no_dir_is_always_empty() {
        let mut cfg = ConfigFile::resolve(&env(&[]), || None);
        assert_eq!(cfg.path(), None);
        assert!(cfg.load().is_empty());
    }

    #[test]
    fn loads_reloads_and_survives_missing_and_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut cfg = ConfigFile::in_dir(dir.path());
        assert!(cfg.load().is_empty());
        assert_eq!(cfg.error(), None);

        fs::write(&path, "tab_prefix = \"❓\"\nhome_tab = false\n").unwrap();
        assert_eq!(cfg.load()["tab_prefix"].as_str(), Some("❓"));
        assert_eq!(cfg.load()["home_tab"].as_bool(), Some(false));

        // An edit of a different size is seen even within one mtime tick.
        fs::write(&path, "tab_prefix = \"\"\n").unwrap();
        assert_eq!(cfg.load()["tab_prefix"].as_str(), Some(""));
        assert!(cfg.load().get("home_tab").is_none());

        fs::write(&path, "tab_prefix = [unclosed\n").unwrap();
        assert!(cfg.load().is_empty());
        let err = cfg.error().unwrap().to_owned();
        assert!(err.starts_with("ignoring "), "{err}");

        // TOML 1.1 is ignored whole, as tomllib (TOML 1.0) does.
        fs::write(&path, "tab_prefix = \"Z\"\nhome = { tab = false, }\n").unwrap();
        assert!(cfg.load().is_empty());
        let err = cfg.error().unwrap().to_owned();
        assert!(
            err.contains("a TOML 1.1 trailing comma in an inline table"),
            "{err}"
        );

        // A replacement (new inode) of the same size is seen too.
        let tmp = dir.path().join("tmp.toml");
        fs::write(&tmp, "tab_prefix = \"X\"\n").unwrap();
        fs::write(&path, "tab_prefix = \"Y\"\n").unwrap();
        assert_eq!(cfg.load()["tab_prefix"].as_str(), Some("Y"));
        fs::rename(&tmp, &path).unwrap();
        assert_eq!(cfg.load()["tab_prefix"].as_str(), Some("X"));
        assert_eq!(cfg.error(), None);

        fs::remove_file(&path).unwrap();
        assert!(cfg.load().is_empty());
    }

    #[test]
    fn load_as_deserializes_the_table() {
        #[derive(serde::Deserialize, Debug, PartialEq, Default)]
        #[serde(default)]
        struct Settings {
            tab_prefix: Option<String>,
            home_tab: bool,
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut cfg = ConfigFile::in_dir(dir.path());
        assert_eq!(cfg.load_as::<Settings>(), Ok(Settings::default()));
        fs::write(&path, "tab_prefix = \"P\"\nhome_tab = true\nother = 1\n").unwrap();
        let s: Settings = cfg.load_as().unwrap();
        assert_eq!(s.tab_prefix.as_deref(), Some("P"));
        assert!(s.home_tab);
        let loose: serde_json::Map<String, serde_json::Value> = cfg.load_as().unwrap();
        assert_eq!(loose["other"], 1);
        fs::write(&path, "home_tab = \"yes\"\n").unwrap();
        assert!(cfg.load_as::<Settings>().is_err());
    }
}
