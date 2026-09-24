//! TOML tree plumbing for [`crate::config`]: serialize typed sections back
//! into `toml::Value`, navigate or create leaves along dotted paths, and
//! parse `CAUCE_*` override strings into TOML scalars.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde::Serialize;

use super::{Config, ConfigError};

/// Serialise then reparse to get a `toml::Value` view of `v` (there is no
/// direct `Serialize -> Value` path in the `toml` crate).
pub(super) fn to_value<T: Serialize>(v: &T) -> Result<toml::Value, ConfigError> {
    let text = toml::to_string_pretty(v).map_err(ConfigError::Encode)?;
    toml::from_str(&text).map_err(ConfigError::Invalid)
}

/// The built-in defaults as a TOML tree; used as the raw layer when no
/// config file exists so a first `save` writes a complete template file.
pub(super) fn default_tree() -> Result<toml::Value, ConfigError> {
    to_value(&Config::default().sections())
}

/// Navigate `tree` along `path` (numeric segments index into arrays) and
/// return the leaf slot, or `None` when the path does not resolve.
pub(super) fn tree_mut_at<'a>(
    tree: &'a mut toml::Value,
    path: &[String],
) -> Option<&'a mut toml::Value> {
    let mut cur = tree;
    for seg in path {
        cur = match cur {
            toml::Value::Table(t) => t.get_mut(seg)?,
            toml::Value::Array(a) => a.get_mut(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

pub(super) fn set_display(slot: Option<&mut toml::Value>, value: toml::Value) {
    if let Some(slot) = slot {
        *slot = value;
    }
}

/// Navigate `tree` along `path` read-only; sibling of [`tree_mut_at`].
pub(super) fn tree_at<'a>(tree: &'a toml::Value, path: &[String]) -> Option<&'a toml::Value> {
    let mut cur = tree;
    for seg in path {
        cur = match cur {
            toml::Value::Table(t) => t.get(seg)?,
            toml::Value::Array(a) => a.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Parse an `CAUCE_*` override string into a TOML scalar: booleans, integers
/// and floats get their native type; everything else stays a string.
pub(super) fn env_scalar(s: &str) -> toml::Value {
    match s {
        "true" => return toml::Value::Boolean(true),
        "false" => return toml::Value::Boolean(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>()
        && f.is_finite()
    {
        return toml::Value::Float(f);
    }
    toml::Value::String(s.to_string())
}

/// Set `path` (e.g. `["server", "port"]`) inside a TOML tree, creating or
/// overwriting intermediate tables as needed.
pub(super) fn set_path(root: &mut toml::Value, path: &[&str], value: toml::Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut cur = root;
    for seg in parents {
        if !cur.is_table() {
            *cur = toml::Value::Table(toml::Table::new());
        }
        let table = match cur.as_table_mut() {
            Some(t) => t,
            None => unreachable!("just converted to table"),
        };
        cur = table
            .entry((*seg).to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    }
    if !cur.is_table() {
        *cur = toml::Value::Table(toml::Table::new());
    }
    if let Some(table) = cur.as_table_mut() {
        table.insert((*last).to_string(), value);
    }
}
