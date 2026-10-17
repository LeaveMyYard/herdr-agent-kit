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
