//! `/settings` form mode for `PUT /api/config` (W2-07): merge
//! `application/x-www-form-urlencoded` dotted-path fields onto the raw file
//! tree.
//!
//! The settings page submits fields named after config paths
//! (`search.deadline_ms`, `engines.<id>.egress.proxy`). The merge is
//! whitelisted — only the paths the form renders are writable — and each
//! value is coerced to its schema type before `Config::from_raw` re-runs the
//! full validation (`${...}` interpolation, `deny_unknown_fields`, engine
//! pinning). Fields the form does not submit keep their file-layer value, so
//! templates and secrets the page never displays survive a save untouched.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;

#[cfg(feature = "ui")]
use cauce_core::config::system_env;
use cauce_core::config::{Config, ConfigError, EngineEntry};

fn invalid(path: &str, msg: impl Into<String>) -> ConfigError {
    ConfigError::InvalidValue {
        path: path.to_string(),
        msg: msg.into(),
    }
}

/// Merge the submitted form `pairs` over the file layer of `current` and
/// return the candidate raw tree (validation happens in the caller via
/// [`Config::from_raw`], identical to the TOML body path).
///
/// The base is `raw_tree()` — the file verbatim, templates unresolved —
/// never the resolved/display tree, so `CAUCE_*` overrides and
/// `CAUCE_ENGINES` pinning cannot be baked into `config.toml` by a save.
/// A `Config` without a file layer (`Config::default()` in tests) falls
/// back to the redacted display tree; the `<redacted>` placeholders it can
/// contain are restored by the caller's `restore_redacted` pass like any
/// `GET -> edit -> PUT` roundtrip.
pub fn merge_form_config(
    current: &Config,
    pairs: &[(String, String)],
) -> Result<toml::Value, ConfigError> {
    let mut tree = current
        .raw_tree()
        .cloned()
        .or_else(|| current.display_tree().ok())
        .unwrap_or_else(|| toml::Value::Table(toml::Table::new()));

    // Last value wins on a repeated name: every enabled checkbox pairs a
    // hidden `false` with the submitted `true` when ticked.
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in pairs {
        fields.insert(name.clone(), value.clone());
    }

    for (name, value) in &fields {
        apply_field(&mut tree, current, name, value)?;
    }
    Ok(tree)
}

/// Whitelisted scalar paths the settings form renders, with coercion.
fn apply_field(
    tree: &mut toml::Value,
    current: &Config,
    name: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match name {
        "search.deadline_ms"
        | "search.ttl_s"
        | "admission.max_wait_ms"
        | "admission.max_concurrent_per_engine"
        | "logs.retention_days" => {
            let n = value
                .trim()
                .parse::<i64>()
                .map_err(|_| invalid(name, "expected a non-negative integer"))?;
            if n < 0 {
                return Err(invalid(name, "expected a non-negative integer"));
            }
            set_path(tree, name, toml::Value::Integer(n))
        }
        "ai.base_url" | "ai.api_key" | "ai.model" => {
            set_path(tree, name, toml::Value::String(value.to_string()))
        }
        "ai.enabled" => set_path(tree, name, toml::Value::Boolean(parse_bool(name, value)?)),
        _ if name.starts_with("engines.") => apply_engine_field(tree, current, name, value),
        _ => Err(invalid(name, "unknown settings field")),
    }
}

/// `engines.<id>.enabled` / `.tier` / `.egress.proxy` (the `.egress.proxy`
/// suffix is matched first so an id cannot shadow it).
fn apply_engine_field(
    tree: &mut toml::Value,
    current: &Config,
    name: &str,
    value: &str,
) -> Result<(), ConfigError> {
    let rest = &name["engines.".len()..];
    let (id, field) = if let Some(id) = rest.strip_suffix(".egress.proxy") {
        (id, "egress.proxy")
    } else if let Some((id, field)) = rest.rsplit_once('.') {
        (id, field)
    } else {
        return Err(invalid(name, "expected engines.<id>.<field>"));
    };
    let resolved = current
        .engine(id)
        .ok_or_else(|| invalid(name, format!("unknown engine {id:?}")))?;

    match field {
        "enabled" => {
            let enabled = parse_bool(name, value)?;
            // A file-less entry whose submitted flag already matches the
            // resolved value needs no `[[engines]]` stanza — keeps the
            // built-ins out of `config.toml` until they diverge.
            if engine_entry(tree, id).is_none() && enabled == resolved.enabled {
                return Ok(());
            }
            engine_entry_mut(tree, resolved)?
                .insert("enabled".to_string(), toml::Value::Boolean(enabled));
            Ok(())
        }
        "tier" => {
            if value.trim().is_empty() {
                // "default" clears the override.
                if let Some(entry) = engine_entry(tree, id) {
                    entry.remove("tier");
                }
                return Ok(());
            }
            let tier = value
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|t| (1..=3).contains(t))
                .ok_or_else(|| invalid(name, "expected 1, 2 or 3"))?;
            // Same skip as `enabled`: a file-less engine whose submitted
            // value already matches the resolved one needs no stanza.
            if engine_entry(tree, id).is_none() && resolved.tier.is_some_and(|t| t.as_u8() == tier)
            {
                return Ok(());
            }
            engine_entry_mut(tree, resolved)?
                .insert("tier".to_string(), toml::Value::Integer(i64::from(tier)));
            Ok(())
        }
        "egress.proxy" => {
            if value.trim().is_empty() {
                if let Some(entry) = engine_entry(tree, id)
                    && let Some(egress) = entry.get_mut("egress").and_then(|v| v.as_table_mut())
                {
                    egress.remove("proxy");
                    if egress.is_empty() {
                        entry.remove("egress");
                    }
                }
                return Ok(());
            }
            let proxy = value.trim();
            if engine_entry(tree, id).is_none()
                && resolved.egress.as_ref().and_then(|e| e.proxy.as_deref()) == Some(proxy)
            {
                return Ok(());
            }
            let entry = engine_entry_mut(tree, resolved)?;
            let egress = entry
                .entry("egress".to_string())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if !egress.is_table() {
                *egress = toml::Value::Table(toml::Table::new());
            }
            egress.as_table_mut().expect("just ensured table").insert(
                "proxy".to_string(),
                toml::Value::String(value.trim().to_string()),
            );
            Ok(())
        }
        _ => Err(invalid(name, "expected enabled, tier or egress.proxy")),
    }
}

fn parse_bool(name: &str, value: &str) -> Result<bool, ConfigError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(invalid(name, "expected true or false")),
    }
}

/// The `[[engines]]` entry for `id` in the raw tree, if the file defines one.
fn engine_entry<'a>(tree: &'a mut toml::Value, id: &str) -> Option<&'a mut toml::Table> {
    tree.get_mut("engines")?
        .as_array_mut()?
        .iter_mut()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(id))?
        .as_table_mut()
}

/// [`engine_entry`] that creates the stanza when missing: the new entry is
/// seeded from the resolved [`EngineEntry`] so required fields (`kind`, an
/// exec `command`, ...) are always present and valid — minus `enabled` (a
/// `CAUCE_ENGINES` pin must never be baked into the file by a tier/proxy
/// edit) and `env` (process secrets never serialise).
fn engine_entry_mut<'a>(
    tree: &'a mut toml::Value,
    resolved: &EngineEntry,
) -> Result<&'a mut toml::Table, ConfigError> {
    if engine_entry(tree, resolved.id.as_str()).is_none() {
        let mut entry = toml::from_str::<toml::Value>(
            &toml::to_string_pretty(resolved).map_err(ConfigError::Encode)?,
        )
        .map_err(ConfigError::Invalid)?;
        if let Some(table) = entry.as_table_mut() {
            table.remove("enabled");
            table.remove("env");
        }
        let engines = tree
            .as_table_mut()
            .ok_or_else(|| invalid("engines", "config root is not a table"))?
            .entry("engines".to_string())
            .or_insert_with(|| toml::Value::Array(Vec::new()));
        if !engines.is_array() {
            *engines = toml::Value::Array(Vec::new());
        }
        engines
            .as_array_mut()
            .expect("just ensured array")
            .push(entry);
    }
    Ok(engine_entry(tree, resolved.id.as_str()).expect("entry just ensured"))
}

/// Per-field validation for the HTMX submit: every `(field, message)` pair
/// `apply_field` rejects, so the page can place one error line under each
/// offending input. Also surfaces schema errors `Config::from_raw` finds
/// after a clean field merge (e.g. a cross-field rule).
#[cfg(feature = "ui")]
pub fn field_errors(current: &Config, pairs: &[(String, String)]) -> Vec<(String, String)> {
    let mut tree = current
        .raw_tree()
        .cloned()
        .or_else(|| current.display_tree().ok())
        .unwrap_or_else(|| toml::Value::Table(toml::Table::new()));

    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in pairs {
        fields.insert(name.clone(), value.clone());
    }

    let mut errors = Vec::new();
    for (name, value) in &fields {
        if let Err(e) = apply_field(&mut tree, current, name, value) {
            errors.push((name.clone(), e.to_string()));
        }
    }
    if errors.is_empty()
        && let Err(e) = Config::from_raw(&tree, &system_env())
    {
        let path = match &e {
            ConfigError::InvalidValue { path, .. } => path.clone(),
            ConfigError::InvalidEngine { id, .. } => format!("engines.{id}"),
            _ => String::new(),
        };
        errors.push((path, e.to_string()));
    }
    errors
}

/// Write `value` at the dotted `path`, creating intermediate tables.
fn set_path(tree: &mut toml::Value, path: &str, value: toml::Value) -> Result<(), ConfigError> {
    let mut cur = tree;
    let mut segments = path.split('.').peekable();
    while let Some(seg) = segments.next() {
        if segments.peek().is_none() {
            let table = cur
                .as_table_mut()
                .ok_or_else(|| invalid(path, "config root is not a table"))?;
            table.insert(seg.to_string(), value);
            return Ok(());
        }
        cur = cur
            .as_table_mut()
            .ok_or_else(|| invalid(path, "config root is not a table"))?
            .entry(seg.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cauce_core::config::EnvMap;

    fn env() -> EnvMap {
        EnvMap::new()
    }

    fn cfg(raw: &str) -> Config {
        let tree = toml::from_str(raw).unwrap();
        Config::from_raw(&tree, &env()).unwrap()
    }

    fn pairs(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn scalar_fields_merge_onto_raw_tree() {
        let current = cfg("[search]\nttl_s = 60\n[ai]\napi_key = \"${env:CAUCE_TEST_KEY:-k}\"\n");
        let tree = merge_form_config(&current, &pairs(&[("search.deadline_ms", "1234")])).unwrap();
        assert_eq!(tree["search"]["deadline_ms"].as_integer(), Some(1234));
        // Untouched fields keep their file-layer values.
        assert_eq!(tree["search"]["ttl_s"].as_integer(), Some(60));
        assert_eq!(
            tree["ai"]["api_key"].as_str(),
            Some("${env:CAUCE_TEST_KEY:-k}")
        );
    }

    #[test]
    fn last_value_wins_for_checkbox_pairs() {
        let current = cfg("");
        let tree = merge_form_config(
            &current,
            &pairs(&[
                ("engines.replay.enabled", "false"),
                ("engines.replay.enabled", "true"),
            ]),
        )
        .unwrap();
        let replay = tree["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"].as_str() == Some("replay"))
            .unwrap();
        assert_eq!(replay["enabled"].as_bool(), Some(true));
        assert_eq!(replay["kind"].as_str(), Some("replay"));
    }

    #[test]
    fn builtin_engine_at_resolved_value_writes_no_entry() {
        let current = cfg("");
        // replay's built-in enabled flag is false; submitting false adds nothing.
        let tree =
            merge_form_config(&current, &pairs(&[("engines.replay.enabled", "false")])).unwrap();
        assert!(tree.get("engines").is_none());
        // ddgs' built-in flag is true; disabling it creates a full entry.
        let tree =
            merge_form_config(&current, &pairs(&[("engines.ddgs.enabled", "false")])).unwrap();
        let ddgs = tree["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"].as_str() == Some("ddgs"))
            .unwrap();
        assert_eq!(ddgs["enabled"].as_bool(), Some(false));
        assert_eq!(ddgs["kind"].as_str(), Some("exec"));
        assert_eq!(ddgs["command"].as_str(), Some("python3"));
    }

    #[test]
    fn tier_and_proxy_round_trip() {
        let current = cfg(
            "[[engines]]\nid = \"replay\"\nkind = \"replay\"\nenabled = true\ntier = 3\n[engines.egress]\nproxy = \"http://p:1\"\n",
        );
        let tree = merge_form_config(
            &current,
            &pairs(&[
                ("engines.replay.tier", ""),
                ("engines.replay.egress.proxy", ""),
            ]),
        )
        .unwrap();
        let replay = &tree["engines"][0];
        assert!(replay.get("tier").is_none());
        assert!(replay.get("egress").is_none());

        let tree = merge_form_config(
            &current,
            &pairs(&[
                ("engines.replay.tier", "1"),
                ("engines.replay.egress.proxy", "socks5://p:2"),
            ]),
        )
        .unwrap();
        let replay = &tree["engines"][0];
        assert_eq!(replay["tier"].as_integer(), Some(1));
        assert_eq!(replay["egress"]["proxy"].as_str(), Some("socks5://p:2"));
    }

    /// A `CAUCE_ENGINES=replay` pin resolves `enabled = true` on the builtin;
    /// a tier edit that creates the stanza must not bake that pin (or any
    /// resolved `env`) into the file.
    #[test]
    fn seeded_stanza_omits_pinned_enabled_and_env() {
        let mut resolved = Config::from_raw(&toml::from_str("").unwrap(), &env())
            .unwrap()
            .engines
            .iter()
            .find(|e| e.id.as_str() == "replay")
            .unwrap()
            .clone();
        resolved.enabled = true; // as CAUCE_ENGINES resolves it
        resolved.env.insert("SECRET".to_string(), "x".to_string());

        let mut tree = toml::Value::Table(toml::Table::new());
        let entry = engine_entry_mut(&mut tree, &resolved).unwrap();
        entry.insert("tier".to_string(), toml::Value::Integer(2));

        let out = toml::to_string_pretty(&tree).unwrap();
        assert!(!out.contains("enabled"), "pin must not persist: {out}");
        assert!(!out.contains("env"), "env must not persist: {out}");
        assert!(out.contains("tier = 2"), "{out}");
    }

    #[test]
    fn unknown_fields_and_bad_values_are_rejected() {
        let current = cfg("");
        for bad in [
            ("server.port", "1"),
            ("search.deadline_ms", "soon"),
            ("engines.nope.enabled", "true"),
            ("engines.replay.tier", "9"),
            ("ai.enabled", "yes"),
        ] {
            assert!(
                merge_form_config(&current, &pairs(&[bad])).is_err(),
                "{bad:?}"
            );
        }
    }
}
