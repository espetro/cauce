//! Engine construction from `[[engines]]` config entries — the single
//! factory shared by `oxe serve` (all enabled engines) and `oxe record`
//! (one engine by id), so both resolve `command`/`args`/`env`/`cwd`/`tier`/
//! `page_size` the same way.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxe_core::config::{Config, EngineEntry, EngineKind};
use oxe_core::{Engine, Tier};
use tracing::warn;

use crate::exec::{ExecEngine, ExecSpec};
use crate::replay::Replay;

/// Construct every `enabled` engine in `cfg` (`Config::load` already
/// applied `OXE_ENGINES` pinning). Wave 0 knows `replay` and `exec`;
/// `declarative` lands in W1 and is skipped with a warning.
pub fn build_engines(cfg: &Config) -> Vec<Arc<dyn Engine>> {
    cfg.enabled_engines().filter_map(build_engine).collect()
}

/// Construct one engine from a config entry, regardless of its `enabled`
/// flag (`oxe record --engine <id>` reaches disabled entries too). Returns
/// `None` — with a warning — for entries this wave cannot run.
pub fn build_engine(entry: &EngineEntry) -> Option<Arc<dyn Engine>> {
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
        EngineKind::Declarative => {
            warn!(id = %entry.id, "declarative engines land in W1; skipped");
            None
        }
    }
}

/// `cwd` for an exec entry without one: when the first arg is a relative
/// script path (the `ddgs` built-in ships
/// `sdk/python/oxe_engine_sdk/ddgs_auto.py`), walk up from the process cwd
/// until the file is found so `oxe serve`/`oxe record` also work outside
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
