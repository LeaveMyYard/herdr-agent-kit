//! The lenient field readers every type here uses: a value that does not
//! fit reads as its default, a list item that does not fit is skipped.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Any value that does not fit `T` becomes `T::default()`.
pub(super) fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

/// A list whose items that do not fit `T` are skipped; anything but a list
/// is empty.
pub(super) fn lenient_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| T::deserialize(v).ok())
            .collect(),
        _ => Vec::new(),
    })
}

/// An `Option<Vec<T>>` that keeps "absent" (`None`) apart from "empty".
pub(super) fn lenient_opt_vec<'de, D, T>(d: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => Some(
            items
                .into_iter()
                .filter_map(|v| T::deserialize(v).ok())
                .collect(),
        ),
        _ => None,
    })
}

/// argv as panorama reads it: strings and integers, as strings; anything
/// else dropped (`[str(a) for a in argv if isinstance(a, (str, int))]`).
pub(super) fn argv<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s),
                Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}
