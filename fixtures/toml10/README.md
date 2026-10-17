# TOML 1.0 goldens (Python `tomllib`)

What Python's `tomllib` (TOML 1.0) makes of each input, for `plugin_env::toml10` (the
Rust plugins read `config.toml` as the Python plugins do while both run, plan §5.4).
`tests/toml10_goldens.rs` checks every case. Captured 2026-10-17 with CPython 3.13.0;
nothing in `cases.json` was edited by hand.

## How they were captured

```
git clone https://github.com/toml-lang/toml-test <checkout>   # commit ff49d10
python3 -I capture/capture_tomllib.py <checkout> > cases.json
```

The inputs are the hand-written cases in `capture/capture_tomllib.py` (the TOML 1.1
additions, the same shapes in TOML 1.0, implementation limits, and config files like the
plugins' own), then every `tests/valid` and `tests/invalid` file of
[toml-test](https://github.com/toml-lang/toml-test) (MIT, © TOML authors), named
`toml-test/<path>`. The script's docstring describes the format and the value tags.

`tomllib` is TOML 1.0 in every Python the plugins support (`requires-python >= 3.11`, up
to 3.13 here); re-capture if a later Python's `tomllib` reads TOML 1.1.
