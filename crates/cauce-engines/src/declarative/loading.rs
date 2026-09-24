//! Spec source loading (W1-02): `engines/*.yaml` files are embedded into
//! the binary at build time via `rust-embed` (the `include` glob keeps
//! `engines/fixtures/` bodies out of the binary — fixtures are read from
//! the filesystem only); at runtime, `$CAUCE_CONFIG_DIR/engines/*.yaml`
//! files override them — a config-dir file whose spec `id` matches an
//! embedded spec replaces it, any other `id` adds a new spec.
//!
//! Lookup order for one name (a `[[engines]]` `spec` value or an entry
//! `id`):
//!
//! 1. literal file path (`spec = "/abs/or/rel.yaml"`)
//! 2. `$config_dir/engines/<name>` / `<name>.yaml` / `<name>.yml`
//! 3. embedded `<name>` / `<name>.yaml` / `<name>.yml`
//! 4. embedded spec whose `id` equals `name`
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rust_embed::RustEmbed;
use tracing::warn;

use cauce_core::config::{EngineEntry, EnvMap};

use super::spec::{CompiledSpec, EngineSpec, SpecError};

/// `engines/*.yaml` embedded at build: specs ship inside the binary while
/// the `include` globs keep `LICENSE` and every `fixtures/` body out of it
/// (`fixture_pairs` only ever reads the filesystem).
#[derive(RustEmbed)]
#[folder = "../../engines/"]
#[include = "*.yaml"]
#[include = "*.yml"]
struct EmbeddedEngines;

/// `(file_name, yaml_text)` for every embedded top-level `*.yaml`/`*.yml`
/// spec.
fn embedded_specs() -> Vec<(String, String)> {
    EmbeddedEngines::iter()
        .filter_map(|name| {
            let file = EmbeddedEngines::get(&name)?;
            let text = std::str::from_utf8(&file.data).ok()?;
            Some((name.into_owned(), text.to_string()))
        })
        .collect()
}

/// `*.yaml`/`*.yml` files under `$config_dir/engines/`, sorted by name.
fn override_spec_files(config_dir: &Path) -> Vec<PathBuf> {
    let dir = config_dir.join("engines");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "yaml" || e == "yml")
        })
        .collect();
    files.sort_unstable();
    files
}

/// One spec source's load outcome: [`load_specs_report`]'s element.
pub struct SpecSource {
    /// Spec `id` when the YAML parsed far enough to name one; the file
    /// name/path otherwise.
    pub name: String,
    /// The compiled spec, or why it could not load.
    pub spec: Result<CompiledSpec, String>,
}

/// `load_specs`' merge order (embedded `engines/*.yaml` then
/// `$config_dir/engines/*.yaml` overrides by spec `id`) with failures
/// returned instead of skipped: for the `--live` drift canary a shipped
/// spec that cannot even load *is* the drift being hunted — it must
/// surface as a failure, not quietly drop out of the worklist.
pub fn load_specs_report(config_dir: &Path, env: &EnvMap) -> Vec<SpecSource> {
    let mut errors = Vec::new();
    let mut by_id: BTreeMap<String, String> = BTreeMap::new();
    for (name, text) in embedded_specs() {
        match EngineSpec::from_yaml(&text) {
            Ok(spec) => {
                by_id.insert(spec.id.to_string(), text);
            }
            Err(e) => errors.push(SpecSource {
                name,
                spec: Err(format!("invalid spec yaml: {e}")),
            }),
        }
    }
    for path in override_spec_files(config_dir) {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                errors.push(SpecSource {
                    name: path.display().to_string(),
                    spec: Err(format!("unreadable spec: {e}")),
                });
                continue;
            }
        };
        match EngineSpec::from_yaml(&text) {
            Ok(spec) => {
                by_id.insert(spec.id.to_string(), text);
            }
            Err(e) => errors.push(SpecSource {
                name: path.display().to_string(),
                spec: Err(format!("invalid spec yaml: {e}")),
            }),
        }
    }
    let mut out: Vec<SpecSource> = by_id
        .into_iter()
        .map(|(id, text)| SpecSource {
            name: id,
            spec: CompiledSpec::from_yaml(&text, env).map_err(|e| e.to_string()),
        })
        .collect();
    out.extend(errors);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every loadable spec: [`load_specs_report`] filtered to successes.
/// Specs that fail to parse or compile are skipped with a `warn` — one
/// bad user file must not take down the rest of the engine set.
pub fn load_specs(config_dir: &Path, env: &EnvMap) -> Vec<CompiledSpec> {
    load_specs_report(config_dir, env)
        .into_iter()
        .filter_map(|source| match source.spec {
            Ok(spec) => Some(spec),
            Err(e) => {
                warn!(file = %source.name, error = %e, "engine spec skipped");
                None
            }
        })
        .collect()
}

/// Resolve the YAML text for a `[[engines]]` entry, following the lookup
/// order in the module docs.
pub fn resolve_spec_source(entry: &EngineEntry, config_dir: &Path) -> Result<String, SpecError> {
    let name = entry.spec.clone().unwrap_or_else(|| entry.id.to_string());
    resolve_named(&name, config_dir).ok_or(SpecError::NotFound(name))
}

/// The lookup order shared by `resolve_spec_source` and `engine test`.
pub(crate) fn resolve_named(name: &str, config_dir: &Path) -> Option<String> {
    // 1. literal path
    let literal = Path::new(name);
    if literal.is_file() {
        return std::fs::read_to_string(literal).ok();
    }
    // 2. config-dir override
    let dir = config_dir.join("engines");
    for candidate in [
        dir.join(name),
        dir.join(format!("{name}.yaml")),
        dir.join(format!("{name}.yml")),
    ] {
        if candidate.is_file() {
            return std::fs::read_to_string(candidate).ok();
        }
    }
    // 3. embedded by file name
    for candidate in [
        name.to_string(),
        format!("{name}.yaml"),
        format!("{name}.yml"),
    ] {
        if let Some(f) = EmbeddedEngines::get(&candidate)
            && let Ok(text) = std::str::from_utf8(&f.data)
        {
            return Some(text.to_string());
        }
    }
    // 4. embedded by spec id
    for (_file, text) in embedded_specs() {
        if let Ok(spec) = EngineSpec::from_yaml(&text)
            && spec.id.as_str() == name
        {
            return Some(text);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canary's strict worklist keeps unloadable specs visible as
    /// `Err` entries; `load_specs` still filters them for app startup.
    #[test]
    fn load_specs_report_surfaces_specs_that_cannot_load() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("engines");
        std::fs::create_dir_all(&dir).unwrap();
        // Not even YAML-parseable and valid-YAML-but-invalid-spec.
        std::fs::write(dir.join("broken.yaml"), "id: [not yaml").unwrap();
        std::fs::write(dir.join("badkind.yaml"), "id: bad\nparse: { kind: nope }\n").unwrap();
        let report = load_specs_report(tmp.path(), &EnvMap::new());
        assert!(
            report
                .iter()
                .any(|s| s.name.contains("broken") && s.spec.is_err())
        );
        assert!(
            report
                .iter()
                .any(|s| s.name.contains("badkind") && s.spec.is_err())
        );
        // Embedded shipped specs still load.
        assert!(report.iter().filter(|s| s.spec.is_ok()).count() >= 3);
        // The tolerant loader drops the broken files entirely.
        assert!(
            load_specs(tmp.path(), &EnvMap::new())
                .iter()
                .all(|s| s.id().as_str() != "bad")
        );
    }

    #[test]
    fn embed_includes_only_top_level_yaml() {
        // The `include` glob must keep `engines/fixtures/**` bodies (and
        // `LICENSE`) out of the binary: every embedded path is a bare
        // top-level `*.yaml`/`*.yml` file name.
        for path in EmbeddedEngines::iter() {
            assert!(
                !path.contains('/') && (path.ends_with(".yaml") || path.ends_with(".yml")),
                "non-spec path embedded: {path}"
            );
        }
    }
}
