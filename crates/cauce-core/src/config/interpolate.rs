//! `${...}` interpolation on string values at load: `${env:NAME}` (missing
//! is an error), `${env:NAME:-default}`, `${env:NAME:?msg}` (missing is an
//! error carrying `msg`), `${file:PATH}`, and `$$` as a literal `$`. The
//! `:` forms follow POSIX: unset-or-empty counts as missing. Paths whose
//! value changed are recorded in `templates` (mapped to their raw text) so
//! the display tree can show the template instead of the resolved secret.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{ConfigError, EnvMap, home_dir};

/// Recursively interpolate every string in the merged tree. `at` tracks the
/// dotted path for error messages; paths whose value changed are recorded
/// in `templates` (mapped to their raw text) for redacted display.
pub(super) fn interpolate_tree(
    value: &mut toml::Value,
    env: &EnvMap,
    at: &mut Vec<String>,
    templates: &mut BTreeMap<Vec<String>, String>,
) -> Result<(), ConfigError> {
    match value {
        toml::Value::String(s) => {
            if s.contains('$') {
                let raw = std::mem::take(s);
                *s = interpolate_str(&raw, env, &at.join("."))?;
                if *s != raw {
                    templates.insert(at.clone(), raw);
                }
            }
        }
        toml::Value::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                at.push(i.to_string());
                interpolate_tree(item, env, at, templates)?;
                at.pop();
            }
        }
        toml::Value::Table(table) => {
            for (key, item) in table.iter_mut() {
                at.push(key.clone());
                interpolate_tree(item, env, at, templates)?;
                at.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

/// Interpolate one string value: `${env:...}`, `${file:...}`, `$$` escape.
/// A bare `$` not followed by `$` or `{` is literal.
///
/// Public so engine spec loaders (cauce-engines `declarative`) can run the
/// same `${env:NAME}`/`${file:PATH}` contract on `request.headers` values.
/// `path` is the dotted location used in error messages.
pub fn interpolate_str(raw: &str, env: &EnvMap, path: &str) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(tail) = after.strip_prefix('$') {
            out.push('$');
            rest = tail;
        } else if let Some(tail) = after.strip_prefix('{') {
            let end = tail
                .find('}')
                .ok_or_else(|| ConfigError::BadInterpolation {
                    path: path.to_string(),
                    expr: format!("${{{tail}"),
                })?;
            out.push_str(&resolve_expr(&tail[..end], env, path)?);
            rest = &tail[end + 1..];
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// Resolve the inside of a `${...}` expression.
fn resolve_expr(inner: &str, env: &EnvMap, path: &str) -> Result<String, ConfigError> {
    if let Some(spec) = inner.strip_prefix("env:") {
        // First `:-` or `:?` wins; the rest of the string is the payload.
        let split = [(":-", '-'), (":?", '?')]
            .into_iter()
            .filter_map(|(sep, kind)| spec.find(sep).map(|i| (i, kind)))
            .min_by_key(|(i, _)| *i);
        let (name, suffix) = match split {
            Some((i, kind)) => (&spec[..i], Some((kind, &spec[i + 2..]))),
            None => (spec, None),
        };
        // POSIX semantics: the `:`-forms (`:-`, `:?`) treat unset OR empty
        // as missing; the plain form treats empty as a real value.
        let value = env.get(name).filter(|v| !v.is_empty());
        return match suffix {
            Some(('-', default)) => Ok(value.cloned().unwrap_or_else(|| default.to_string())),
            Some(('?', msg)) => value.cloned().ok_or_else(|| ConfigError::MissingEnvMsg {
                path: path.to_string(),
                var: name.to_string(),
                msg: msg.to_string(),
            }),
            // Plain `${env:NAME}`: unset errors, empty stays empty.
            _ => env
                .get(name)
                .cloned()
                .ok_or_else(|| ConfigError::MissingEnv {
                    path: path.to_string(),
                    var: name.to_string(),
                }),
        };
    }
    if let Some(file) = inner.strip_prefix("file:") {
        let file = expand_home(file, env);
        return std::fs::read_to_string(&file)
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|source| ConfigError::MissingFile {
                path: path.to_string(),
                file,
                source,
            });
    }
    Err(ConfigError::BadInterpolation {
        path: path.to_string(),
        expr: format!("${{{inner}}}"),
    })
}

/// `~/...` inside `${file:...}` resolves against the user's home directory.
fn expand_home(path: &str, env: &EnvMap) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home_dir(env).join(rest),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::{sandbox, write_config};
    use super::super::{Config, ConfigError};

    /// `${env:...}`/`${file:...}`/escape/disable cases share one
    /// sandbox-and-load shape; a table keeps each contract visible
    /// without repeating the harness. `FILE` inside `value` is
    /// substituted with a per-case `secret.txt` path (POSIX `:-`/`:?`
    /// treat unset-or-empty as missing).
    #[derive(Debug)]
    enum InterpWant {
        Ok(&'static str),
        MissingEnv(&'static str),
        MissingEnvMsg(&'static str, &'static str),
        MissingFile,
    }

    struct InterpCase {
        env: &'static [(&'static str, &'static str)],
        /// TOML written before `[ai]` (e.g. a `[config]` override).
        prefix: &'static str,
        /// `secret.txt` contents written beside the config.
        file: Option<&'static str>,
        /// Raw TOML value of `ai.api_key`.
        value: &'static str,
        want: InterpWant,
    }

    #[test]
    fn interpolation_cases() {
        let cases = &[
            InterpCase {
                env: &[("MY_SECRET", "s3cret")],
                prefix: "",
                file: None,
                value: "${env:MY_SECRET}",
                want: InterpWant::Ok("s3cret"),
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: None,
                value: "${env:UNSET_VAR}",
                want: InterpWant::MissingEnv("UNSET_VAR"),
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: None,
                value: "${env:UNSET_VAR:-fallback}",
                want: InterpWant::Ok("fallback"),
            },
            InterpCase {
                env: &[("SET_VAR", "real")],
                prefix: "",
                file: None,
                value: "${env:SET_VAR:-fallback}",
                want: InterpWant::Ok("real"),
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: None,
                value: "${env:UNSET_VAR:?get a provider key first}",
                want: InterpWant::MissingEnvMsg("UNSET_VAR", "get a provider key first"),
            },
            InterpCase {
                env: &[("SET_VAR", "real-value")],
                prefix: "",
                file: None,
                value: "${env:SET_VAR:?unreachable}",
                want: InterpWant::Ok("real-value"),
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: Some("file-secret\n"),
                value: "${file:FILE}",
                want: InterpWant::Ok("file-secret"),
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: None,
                value: "${file:/nonexistent/secret}",
                want: InterpWant::MissingFile,
            },
            InterpCase {
                env: &[],
                prefix: "",
                file: None,
                value: "literal $$HOME and $$",
                want: InterpWant::Ok("literal $HOME and $"),
            },
            InterpCase {
                env: &[("EMPTY_VAR", "")],
                prefix: "",
                file: None,
                value: "${env:EMPTY_VAR:-fallback}",
                want: InterpWant::Ok("fallback"),
            },
            InterpCase {
                env: &[("EMPTY_VAR", "")],
                prefix: "",
                file: None,
                value: "${env:EMPTY_VAR}",
                want: InterpWant::Ok(""),
            },
            InterpCase {
                env: &[("EMPTY_VAR", "")],
                prefix: "",
                file: None,
                value: "${env:EMPTY_VAR:?need a key}",
                want: InterpWant::MissingEnvMsg("EMPTY_VAR", "need a key"),
            },
            // `$${env:X}` produces the literal text; no expansion.
            InterpCase {
                env: &[("MY_SECRET", "s3cret")],
                prefix: "",
                file: None,
                value: "$${env:MY_SECRET}",
                want: InterpWant::Ok("${env:MY_SECRET}"),
            },
            InterpCase {
                env: &[("PART_A", "sk-"), ("PART_B", "bf-123")],
                prefix: "",
                file: None,
                value: "${env:PART_A}${env:PART_B}",
                want: InterpWant::Ok("sk-bf-123"),
            },
            InterpCase {
                env: &[("MY_SECRET", "s3cret")],
                prefix: "[config]\ninterpolation = false\n",
                file: None,
                value: "${env:MY_SECRET}",
                want: InterpWant::Ok("${env:MY_SECRET}"),
            },
        ];
        for case in cases {
            let (tmp, env) = sandbox(case.env);
            let mut value = case.value.to_string();
            if let Some(content) = case.file {
                let secret = tmp.path().join("secret.txt");
                std::fs::write(&secret, content).unwrap();
                value = value.replace("FILE", &secret.display().to_string());
            }
            write_config(
                &tmp.path().join("cfg"),
                &format!("{}[ai]\napi_key = \"{value}\"\n", case.prefix),
            );
            match (&case.want, Config::load_with(&env)) {
                (InterpWant::Ok(want), Ok(cfg)) => {
                    assert_eq!(&cfg.ai.api_key, want, "{value:?} must resolve")
                }
                (InterpWant::MissingEnv(want), Err(ConfigError::MissingEnv { var, .. })) => {
                    assert_eq!(&var, want, "{value:?} must name the missing var")
                }
                (
                    InterpWant::MissingEnvMsg(wv, wm),
                    Err(ConfigError::MissingEnvMsg { var, msg, .. }),
                ) => {
                    assert_eq!(&var, wv, "{value:?} must name the missing var");
                    assert_eq!(&msg, wm, "{value:?} must carry the message");
                }
                (InterpWant::MissingFile, Err(ConfigError::MissingFile { .. })) => {}
                (want, got) => panic!("{value:?}: expected {want:?}, got {got:?}"),
            }
        }
    }
}
