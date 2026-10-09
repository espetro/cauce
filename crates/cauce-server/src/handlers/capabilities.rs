//! `GET /api/capabilities` + `GET /api/instance` (FX-07, plan §7.4): the
//! bootstrap payloads the SPA renders its chrome and routes from. Both
//! are open reads — a public instance's capabilities are not secret (the
//! *gates* live on the routes they describe); the instance card is what
//! replaces `GET /api/config` for the SPA once config goes admin-only.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;

use crate::app::AppState;
use crate::capabilities::{
    Capabilities, CapabilityFlags, InstanceInfo, InstanceMode, Role, role_for,
};

/// `GET /api/capabilities`: mode + role + flags. Role is derived
/// per-request from the bearer credential, so the same URL answers
/// `role: "user"` for a visitor and `role: "admin"` for the operator.
pub async fn capabilities(State(state): State<AppState>, headers: HeaderMap) -> Json<Capabilities> {
    state.with_config(|cfg| {
        let role = role_for(cfg, &headers);
        let mode = if cfg.server.public_instance {
            InstanceMode::Public
        } else {
            InstanceMode::Local
        };
        let local = mode == InstanceMode::Local;
        Json(Capabilities {
            mode,
            role,
            flags: CapabilityFlags {
                admin_surface: role == Role::Admin,
                server_history: local,
                archiving: state.archiving(),
                shared_stats: local,
            },
        })
    })
}

/// `GET /api/instance`: the public dashboard card + SPA bootstrap knobs
/// (`ai.enabled`, `archive.index_on_click`, engine ids) that used to ride
/// `GET /api/config` — which goes admin-only in public mode.
pub async fn instance(State(state): State<AppState>) -> Json<InstanceInfo> {
    state.with_config(|cfg| {
        // Every configured id — the pin-check set the SPA used to read
        // off `/api/config` (`engines.map(id)`, disabled entries
        // included); `engine_count` reports only the enabled ones.
        let engine_ids: Vec<String> = cfg
            .engines
            .iter()
            .map(|e| e.id.as_str().to_string())
            .collect();
        let engine_count = cfg.engines.iter().filter(|e| e.enabled).count();
        Json(InstanceInfo {
            name: cfg.server.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            engine_count,
            engine_ids,
            // The `[ai].enabled` flag, matching the `aiEnabled` the SPA
            // used to read off `/api/config` — the disabled notice on
            // `/app/answer` needs it true even when the provider build
            // failed.
            ai_enabled: cfg.ai.enabled,
            // `cfg` is already held by `with_config`; the
            // `archive_index_on_click()` accessor would re-lock it
            // (std Mutex, non-reentrant) — check the flag here instead.
            index_on_click: cfg.archive.index_on_click && state.archiving(),
        })
    })
}
