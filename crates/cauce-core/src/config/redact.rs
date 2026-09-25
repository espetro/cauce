//! Secret-leaf redaction for the display tree. Values produced by `${...}`
//! interpolation templates print as their raw template text (the template
//! itself is not secret); every other secret-bearing leaf — the fixed
//! [`SECRET_PATHS`] plus every `engines.<i>.env.*` value — renders as
//! `<redacted>`. [`redacted_leaves`] finds the `<redacted>` placeholders a
//! `PUT /api/config` body sends back so `Config::restore_redacted` can map
//! each to its current secret.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;

use super::tree::tree_mut_at;

/// What a secret leaf renders as in the display tree when it was not
/// produced by an interpolation template (templates print as their raw
/// `${...}` text instead).
pub(super) const REDACTED: &str = "<redacted>";

/// Dotted paths whose values are secrets by position, so the display tree
/// redacts them no matter the value's origin (`${env:...}` template, `CAUCE_*`
/// override or a file literal). Engine `env` maps are covered separately:
/// every `engines.<i>.env.*` value is secret-bearing.
pub(super) const SECRET_PATHS: &[&[&str]] = &[&["ai", "api_key"]];

/// Replace the leaf at `path` with `<redacted>` unless `templates` covers
/// that path (a `${...}` template already displays as its raw text, which
/// reveals the indirection but never the secret) or the leaf is an empty
/// string (no secret to hide; showing `<redacted>` would falsely imply one
/// is set).
fn redact_leaf(tree: &mut toml::Value, path: &[String], templates: &BTreeMap<Vec<String>, String>) {
    if templates.contains_key(path) {
        return;
    }
    let Some(slot) = tree_mut_at(tree, path) else {
        return;
    };
    if let toml::Value::String(s) = &*slot
        && !s.is_empty()
    {
        *slot = toml::Value::String(REDACTED.to_string());
    }
}

/// Redact every secret-bearing leaf the template overlay did not already
/// cover: the fixed `SECRET_PATHS` plus every `engines.<i>.env.*` value
/// (child-process env vars are where engine credentials live). This is the
/// guard for secrets that entered the resolved config as literals — an
/// `CAUCE_*` override such as `CAUCE_AI_API_KEY` or a plain string in the file.
pub(super) fn redact_secret_paths(
    tree: &mut toml::Value,
    templates: &BTreeMap<Vec<String>, String>,
) {
    for path in SECRET_PATHS {
        let owned: Vec<String> = path.iter().map(|s| (*s).to_string()).collect();
        redact_leaf(tree, &owned, templates);
    }
    // `engines` is a `&mut` borrow of `tree`, so the env leaves are
    // redacted in place rather than via `redact_leaf`/`tree_mut_at`.
    let Some(toml::Value::Array(engines)) = tree.get_mut("engines") else {
        return;
    };
    for (i, entry) in engines.iter_mut().enumerate() {
        let Some(toml::Value::Table(env)) = entry.get_mut("env") else {
            continue;
        };
        for (key, value) in env.iter_mut() {
            let path = vec![
                "engines".to_string(),
                i.to_string(),
                "env".to_string(),
                key.clone(),
            ];
            if templates.contains_key(&path) {
                continue;
            }
            if let toml::Value::String(s) = value
                && !s.is_empty()
            {
                *value = toml::Value::String(REDACTED.to_string());
            }
        }
    }
}

/// Recursively collect the paths of every `<redacted>` string leaf.
pub(super) fn redacted_leaves(
    tree: &toml::Value,
    at: &mut Vec<String>,
    out: &mut Vec<Vec<String>>,
) {
    match tree {
        toml::Value::String(s) if s == REDACTED => out.push(at.clone()),
        toml::Value::Table(t) => {
            for (k, v) in t {
                at.push(k.clone());
                redacted_leaves(v, at, out);
                at.pop();
            }
        }
        toml::Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                at.push(i.to_string());
                redacted_leaves(v, at, out);
                at.pop();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::Config;
    use super::super::tests_support::{sandbox, write_config};
    use super::super::tree::tree_at;
    use super::*;

    /// A resolved-value probe proving the secret really loaded (a redact
    /// case could otherwise pass vacuously on a load that dropped it).
    enum Probe {
        ApiKey(&'static str),
        EngineEnv(&'static str, &'static str, &'static str),
    }

    /// env + TOML file -> redacted display output. `present` markers are
    /// checked on every display surface (display TOML, `Debug`, JSON and
    /// pretty-TOML serialization), `present_toml` only on `display_toml`
    /// (TOML-specific syntax) and `absent` nowhere at all.
    struct RedactCase {
        env: &'static [(&'static str, &'static str)],
        /// TOML file body; empty means no file.
        file: &'static str,
        resolved: &'static [Probe],
        present: &'static [&'static str],
        present_toml: &'static [&'static str],
        absent: &'static [&'static str],
    }

    /// Whatever the secret's provenance — `${...}` template, `CAUCE_*`
    /// override or file literal (#82) — it must never render on any
    /// display surface; templates show their raw `${...}` text, the rest
    /// `<redacted>`, and an unset secret path shows `""` rather than
    /// fabricating a redacted one.
    #[test]
    fn display_redact_cases() {
        let cases = &[
            RedactCase {
                env: &[("CAUCE_AI_API_KEY", "sk-live-secret")],
                file: "[ai]\napi_key = \"${env:CAUCE_AI_API_KEY}\"\n[server]\nport = 4480\n",
                resolved: &[],
                present: &["${env:CAUCE_AI_API_KEY}"],
                present_toml: &["4480"],
                absent: &["sk-live-secret"],
            },
            RedactCase {
                env: &[("CAUCE_AI_API_KEY", "sk-live-secret")],
                file: "[ai]\napi_key = \"${env:CAUCE_AI_API_KEY}\"\n",
                resolved: &[Probe::ApiKey("sk-live-secret")],
                present: &["${env:CAUCE_AI_API_KEY}"],
                present_toml: &[],
                absent: &["sk-live-secret"],
            },
            RedactCase {
                env: &[("CAUCE_AI_API_KEY", "s3cret-from-env")],
                file: "",
                resolved: &[Probe::ApiKey("s3cret-from-env")],
                present: &[REDACTED],
                present_toml: &[],
                absent: &["s3cret-from-env"],
            },
            // File literals at secret paths are redacted too, and every
            // `engines.*.env.*` value is secret-bearing — unless it came
            // from a template, which keeps its raw `${...}` text.
            RedactCase {
                env: &[("ENGINE_TMPL", "tmpl-secret")],
                file: "[ai]\napi_key = \"literal-secret\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"engine-secret\"\nOTHER = \"${env:ENGINE_TMPL}\"\n",
                resolved: &[
                    Probe::ApiKey("literal-secret"),
                    Probe::EngineEnv("x", "MY_KEY", "engine-secret"),
                ],
                present: &[],
                present_toml: &["${env:ENGINE_TMPL}"],
                absent: &["literal-secret", "engine-secret", "tmpl-secret"],
            },
            // An unset secret path displays as `""`, not `<redacted>`.
            RedactCase {
                env: &[],
                file: "",
                resolved: &[],
                present: &[],
                present_toml: &["api_key = \"\""],
                absent: &[REDACTED],
            },
        ];
        for case in cases {
            let (tmp, env) = sandbox(case.env);
            if !case.file.is_empty() {
                write_config(&tmp.path().join("cfg"), case.file);
            }
            let cfg = Config::load_with(&env).unwrap();
            for probe in case.resolved {
                match probe {
                    Probe::ApiKey(want) => assert_eq!(&cfg.ai.api_key, want),
                    Probe::EngineEnv(id, key, want) => assert_eq!(
                        cfg.engine(id).unwrap().env.get(*key).map(String::as_str),
                        Some(*want)
                    ),
                }
            }
            let shown = cfg.display_toml().unwrap();
            for marker in case.present_toml {
                assert!(shown.contains(marker), "{marker:?} missing: {shown}");
            }
            for rendered in [
                shown,
                format!("{cfg:?}"),
                serde_json::to_string(&cfg).unwrap(),
                toml::to_string_pretty(&cfg).unwrap(),
            ] {
                for marker in case.present {
                    assert!(rendered.contains(marker), "{marker:?} missing: {rendered}");
                }
                for marker in case.absent {
                    assert!(!rendered.contains(marker), "{marker:?} leaked: {rendered}");
                }
            }
        }
    }

    /// A display -> PUT roundtrip restores `<redacted>` leaves to their
    /// real values (template text or resolved literal) instead of
    /// persisting the placeholder.
    #[test]
    fn restore_redacted_roundtrips_secret_leaves() {
        let (_tmp, env) = sandbox(&[("CAUCE_AI_API_KEY", "env-secret"), ("TMPL", "t-secret")]);
        let mut submitted: toml::Value = toml::from_str(
            "[ai]\napi_key = \"<redacted>\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"<redacted>\"\nOTHER = \"${env:TMPL}\"\n",
        )
        .unwrap();

        // No current secret behind the placeholders yet.
        let (_tmp2, env2) = sandbox(&[("TMPL", "t-secret")]);
        let empty = Config::load_with(&env2).unwrap();
        assert!(empty.restore_redacted(&mut submitted.clone()).is_err());

        // With a current config that has the secrets, both restore.
        write_config(
            &_tmp.path().join("cfg"),
            "[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nMY_KEY = \"file-secret\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();
        let restored = cfg.restore_redacted(&mut submitted).unwrap();
        assert_eq!(restored, ["ai.api_key", "engines.0.env.MY_KEY"]);
        assert_eq!(
            tree_at(&submitted, &["ai".into(), "api_key".into()]).and_then(|v| v.as_str()),
            Some("env-secret")
        );
        assert_eq!(
            tree_at(
                &submitted,
                &["engines".into(), "0".into(), "env".into(), "MY_KEY".into()]
            )
            .and_then(|v| v.as_str()),
            Some("file-secret")
        );
        // The restored tree validates and keeps the template untouched.
        let cfg2 = Config::from_raw(&submitted, &env).unwrap();
        assert_eq!(cfg2.ai.api_key, "env-secret");
        assert_eq!(
            cfg2.engine("x")
                .unwrap()
                .env
                .get("MY_KEY")
                .map(String::as_str),
            Some("file-secret")
        );
    }

    /// A `<redacted>` at a non-secret path (or any spot with no current
    /// secret) is rejected rather than persisted as a literal.
    #[test]
    fn restore_redacted_rejects_orphaned_placeholder() {
        let (_tmp, env) = sandbox(&[]);
        let cfg = Config::load_with(&env).unwrap();
        let mut submitted: toml::Value =
            toml::from_str("[search]\ndeadline_ms = \"<redacted>\"\n").unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());

        // String-typed non-secret paths are rejected too, and a fabricated
        // `<redacted>` on an engine field cannot leak a *different* engine's
        // value across a reordered array.
        let mut submitted: toml::Value = toml::from_str(
            "[server]\nhost = \"<redacted>\"\n\n[[engines]]\nid = \"x\"\nkind = \"exec\"\ncommand = \"<redacted>\"\n",
        )
        .unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());

        // Unknown engine id under env is rejected.
        let mut submitted: toml::Value = toml::from_str(
            "[[engines]]\nid = \"ghost\"\nkind = \"exec\"\ncommand = \"/bin/x\"\n\n[engines.env]\nK = \"<redacted>\"\n",
        )
        .unwrap();
        assert!(cfg.restore_redacted(&mut submitted).is_err());
    }

    /// Engine env placeholders bind by engine id, not array index: a
    /// submitted `[[engines]]` order different from the current config still
    /// restores each secret onto the right engine.
    #[test]
    fn restore_redacted_matches_engine_env_by_id() {
        let (tmp, env) = sandbox(&[]);
        write_config(
            &tmp.path().join("cfg"),
            "[[engines]]\nid = \"a\"\nkind = \"exec\"\ncommand = \"/bin/a\"\n\n[engines.env]\nK = \"secret-a\"\n\n[[engines]]\nid = \"b\"\nkind = \"exec\"\ncommand = \"/bin/b\"\n\n[engines.env]\nK = \"secret-b\"\n",
        );
        let cfg = Config::load_with(&env).unwrap();

        // Submitted order is b, a — the reverse of the file.
        let mut submitted: toml::Value = toml::from_str(
            "[[engines]]\nid = \"b\"\nkind = \"exec\"\ncommand = \"/bin/b\"\n\n[engines.env]\nK = \"<redacted>\"\n\n[[engines]]\nid = \"a\"\nkind = \"exec\"\ncommand = \"/bin/a\"\n\n[engines.env]\nK = \"<redacted>\"\n",
        )
        .unwrap();
        let mut restored = cfg.restore_redacted(&mut submitted).unwrap();
        restored.sort();
        assert_eq!(restored, ["engines.0.env.K", "engines.1.env.K"]);
        let k = |i: &str| {
            tree_at(
                &submitted,
                &["engines".into(), i.into(), "env".into(), "K".into()],
            )
            .and_then(|v| v.as_str().map(String::from))
        };
        assert_eq!(k("0").as_deref(), Some("secret-b"));
        assert_eq!(k("1").as_deref(), Some("secret-a"));
    }
}
