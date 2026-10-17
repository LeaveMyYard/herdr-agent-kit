//! Repairing a line Python's `json` reads and serde_json does not: lone UTF-16
//! surrogate escapes.

/// The code unit of a `\uXXXX` escape starting at `raw[i]` (the backslash).
fn escape_unit(raw: &[u8], i: usize) -> Option<u16> {
    let hex = raw.get(i + 2..i + 6)?;
    if raw.get(i + 1) != Some(&b'u') || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()
}

/// `raw` with every unpaired UTF-16 surrogate escape (`\ud800`..`\udfff`
/// not part of a high-low pair) replaced by `\ufffd`. Escaped backslashes are
/// skipped, so `\\ud83d` (a backslash, then text) is left alone.
pub(super) fn replace_lone_surrogates(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        match escape_unit(raw, i) {
            Some(0xD800..=0xDBFF) if matches!(escape_unit(raw, i + 6), Some(0xDC00..=0xDFFF)) => {
                out.extend_from_slice(&raw[i..i + 12]);
                i += 12;
            }
            Some(0xD800..=0xDFFF) => {
                out.extend_from_slice(b"\\ufffd");
                i += 6;
            }
            _ => {
                // any other escape (or a stray backslash): copy it and the byte it escapes
                out.extend_from_slice(&raw[i..(i + 2).min(raw.len())]);
                i += 2;
            }
        }
    }
    out
}
