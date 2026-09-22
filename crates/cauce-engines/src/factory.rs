//! Engine construction from `[[engines]]` config entries plus spec
//! auto-registration — the single factory shared by `cauce serve` (all
//! enabled engines) and `cauce record` (one engine by id), so both resolve
//! `command`/`args`/`env`/`cwd`/`tier`/`page_size` the same way.
//!
//! Declarative engines (W1-02) resolve their YAML spec through
//! [`declarative::resolve_spec_source`] (entry `spec` path/name, else the
//! entry `id`, looking in `$CAUCE_CONFIG_DIR/engines/` before the embedded
//! `engines/*.yaml`) and get their `HttpClient` from the entry's
//! `[engines.<id>.egress]` table.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cauce_core::config::{Config, EngineEntry, EngineKind, system_env};
use cauce_core::http::HttpClient;
use cauce_core::{Engine, Tier};
use tracing::warn;

use crate::declarative::{CompiledSpec, DeclarativeEngine, resolve_spec_source};
use crate::exec::{ExecEngine, ExecSpec};
use crate::replay::Replay;

/// Construct every `enabled` engine in `cfg` (`Config::load` already
/// applied `CAUCE_ENGINES` pinning), then auto-register every *enabled*
/// declarative spec that has no `[[engines]]` entry — dropping
/// `engines/bing.yaml` into the repo (or the config dir) is all it takes
/// to join the default fan-out.
///
/// Auto-registration is suppressed while `CAUCE_ENGINES` pins the set: the
/// pin is exhaustive, so an unconfigured spec must not appear just
/// because its file exists (the golden path pins `CAUCE_ENGINES=replay`).
pub fn build_engines(cfg: &Config) -> Vec<Arc<dyn Engine>> {
    let mut engines: Vec<Arc<dyn Engine>> = cfg
        .enabled_engines()
        .filter_map(|e| build_engine(e, cfg.config_dir()))
        .collect();

    if !cauce_engines_pinned() {
        for spec in crate::declarative::load_specs(cfg.config_dir(), &system_env()) {
            let id = spec.id().clone();
            if !spec.spec().enabled || cfg.engine(id.as_str()).is_some() {
                continue;
            }
            match HttpClient::from_egress_config(id.clone(), None) {
                Ok(http) => engines.push(Arc::new(DeclarativeEngine::new(spec, http))),
                Err(e) => warn!(id = %id, error = %e, "declarative engine skipped"),
            }
        }
    }
    engines
}

/// `CAUCE_ENGINES` set and non-empty (config treats empty as unset).
fn cauce_engines_pinned() -> bool {
    std::env::var("CAUCE_ENGINES").is_ok_and(|v| !v.trim().is_empty())
}

/// Construct one engine from a config entry, regardless of its `enabled`
/// flag (`cauce record --engine <id>` reaches disabled entries too).
/// `config_dir` is `cfg.config_dir()` — spec overrides live under its
/// `engines/` subdirectory. Returns `None` — with a warning — for entries
/// this wave cannot run.
pub fn build_engine(entry: &EngineEntry, config_dir: &Path) -> Option<Arc<dyn Engine>> {
    match entry.kind {
        EngineKind::Replay => {
            if entry.id.as_str() != "replay" {
                warn!(
                    id = %entry.id,
                    "replay engines always run as id \"replay\"; a pin on this id will miss"
                );
            }
            Some(Arc::new(Replay::from_env()))
        }
        EngineKind::Exec => {
            let Some(command) = &entry.command else {
                warn!(id = %entry.id, "exec engine without command; skipped");
                return None;
            };
            Some(Arc::new(ExecEngine::new(ExecSpec {
                id: entry.id.clone(),
                command: command.clone(),
                args: entry.args.clone(),
                env: entry
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                cwd: entry
                    .cwd
                    .as_ref()
                    .map(PathBuf::from)
                    .or_else(|| resolve_exec_cwd(entry)),
                page_size: entry.page_size.unwrap_or(10),
                tier: entry.tier.unwrap_or(Tier::T2),
            })))
        }
        EngineKind::Declarative => match build_declarative(entry, config_dir) {
            Ok(engine) => Some(Arc::new(engine)),
            Err(e) => {
                warn!(id = %entry.id, error = %e, "declarative engine skipped");
                None
            }
        },
    }
}

/// `kind = "declarative"`: resolve the spec source, compile it (with the
/// process env for `${...}` header interpolation), build the `HttpClient`
/// from `[engines.<id>.egress]`, apply `tier`/`page_size` overrides.
fn build_declarative(
    entry: &EngineEntry,
    config_dir: &Path,
) -> Result<DeclarativeEngine, crate::declarative::SpecError> {
    let source = resolve_spec_source(entry, config_dir)?;
    let compiled = CompiledSpec::from_yaml(&source, &system_env())?;
    let http = HttpClient::from_egress_config(entry.id.clone(), entry.egress.as_ref())?;
    Ok(DeclarativeEngine::new(compiled, http).with_overrides(entry.tier, entry.page_size))
}

/// `cwd` for an exec entry without one: when the first arg is a relative
/// script path (the `ddgs` built-in ships
/// `sdk/python/cauce_engine_sdk/ddgs_auto.py`), walk up from the process cwd
/// until the file is found so `cauce serve`/`cauce record` also work outside
/// the repo root.
fn resolve_exec_cwd(entry: &EngineEntry) -> Option<PathBuf> {
    let script = entry.args.first()?;
    if Path::new(script).is_absolute() {
        return None;
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(script).is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
