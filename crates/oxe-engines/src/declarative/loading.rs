//! Spec source loading (W1-02): `engines/*.yaml` files are embedded into
//! the binary at build time via `include_dir`; at runtime,
//! `$OXE_CONFIG_DIR/engines/*.yaml` files override them — a config-dir
//! file whose spec `id` matches an embedded spec replaces it, any other
//! `id` adds a new spec.
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

use include_dir::{Dir, include_dir};
use tracing::warn;

use oxe_core::config::{EngineEntry, EnvMap};

use super::spec::{CompiledSpec, EngineSpec, SpecError};

/// `engines/` embedded at build (specs ship inside the binary; the
/// directory also carries `LICENSE` and `fixtures/`, which are embedded
/// too but never consulted at runtime).
static EMBEDDED_ENGINES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../engines");

/// `(file_name, yaml_text)` for every embedded top-level `*.yaml`/`*.yml`
/// spec (fixtures subdirectories are not walked).
fn embedded_specs() -> Vec<(&'static str, &'static str)> {
    EMBEDDED_ENGINES
        .files()
        .filter_map(|f| {
            let name = f.path().file_name()?.to_str()?;
            let is_yaml = name.ends_with(".yaml") || name.ends_with(".yml");
            is_yaml.then(|| {
                f.contents_utf8()
                    .map(|text| (f.path().to_str().unwrap_or(name), text))
            })?
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

/// Every loadable spec: embedded first, then `$config_dir/engines/*.yaml`
/// overrides merged by spec `id` (a same-`id` file replaces the embedded
/// spec). Specs that fail to parse or compile are skipped with a `warn` —
/// one bad user file must not take down the rest of the engine set.
pub fn load_specs(config_dir: &Path, env: &EnvMap) -> Vec<CompiledSpec> {
    let mut by_id: BTreeMap<String, String> = BTreeMap::new();
    for (name, text) in embedded_specs() {
        match EngineSpec::from_yaml(text) {
            Ok(spec) => {
                by_id.insert(spec.id.to_string(), text.to_string());
            }
            Err(e) => warn!(file = name, error = %e, "embedded engine spec skipped"),
        }
    }
    for path in override_spec_files(config_dir) {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!(file = %path.display(), error = %e, "engine spec override unreadable");
                continue;
            }
        };
        match EngineSpec::from_yaml(&text) {
            Ok(spec) => {
                by_id.insert(spec.id.to_string(), text);
            }
            Err(e) => {
                warn!(file = %path.display(), error = %e, "engine spec override skipped")
            }
        }
    }
    by_id
        .into_values()
        .filter_map(|text| match CompiledSpec::from_yaml(&text, env) {
            Ok(spec) => Some(spec),
            Err(e) => {
                warn!(error = %e, "engine spec failed to compile");
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
        if let Some(f) = EMBEDDED_ENGINES.get_file(&candidate)
            && let Some(text) = f.contents_utf8()
        {
            return Some(text.to_string());
        }
    }
    // 4. embedded by spec id
    for (_file, text) in embedded_specs() {
        if let Ok(spec) = EngineSpec::from_yaml(text)
            && spec.id.as_str() == name
        {
            return Some(text.to_string());
        }
    }
    None
}
