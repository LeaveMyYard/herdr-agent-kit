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

use std::fmt;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::de::DeserializeOwned;
use toml::Table;

use super::PluginEnv;

/// [`ConfigFile::load_as`]'s error: the settings do not deserialize into the
/// requested type (a setting of the wrong type, say). Opaque, so it can carry
/// more detail later without a break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

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
    pub fn load_as<T: DeserializeOwned>(&mut self) -> Result<T, ConfigError> {
        toml::Value::Table(self.load().clone())
            .try_into()
            .map_err(|e: toml::de::Error| ConfigError {
                message: e.message().to_owned(),
            })
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
mod tests;
