//! FX-07 instance modes: the `Capabilities`/`InstanceInfo` wire types and
//! the server-side role derivation every admin gate shares (frontend plan
//! §7.4 — the server is authoritative; capability info never flows
//! client-side only).
//!
//! `server.public_instance` selects the mode. `local` (the default) is the
//! single-user deployment — today's behaviour bit-for-bit, and every
//! caller reports as `admin`. `public` splits callers by the
//! `[auth] admin_tokens` bearer credential: a match earns `role: "admin"`,
//! everything else is `role: "user"`. With no tokens configured the admin
//! surface is unreachable — deliberate fail-closed.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use cauce_core::config::Config;
use serde::Serialize;
use ts_rs::TS;

/// The instance's deployment mode (`server.public_instance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum InstanceMode {
    /// Single-user deployment (default): every caller is admin, history
    /// and stats are server-backed.
    Local,
    /// Shared deployment: per-user state lives client-side, admin
    /// surfaces require an `[auth] admin_tokens` bearer credential.
    Public,
}

/// The caller's role on this request — `admin` when the request carries a
/// configured admin token (or the instance is local), `user` otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

/// `GET /api/capabilities` payload (plan §7.4): the mode, the caller's
/// role, and the feature flags the SPA renders by. `flags.adminSurface`
/// is role-relative (admins see the ops surfaces); the rest describe the
/// instance for every caller.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub mode: InstanceMode,
    pub role: Role,
    pub flags: CapabilityFlags,
}

/// The capability flags under `Capabilities.flags` — kept a named wire
/// type so the ts-rs export preserves the §7.4 shape.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityFlags {
    /// Whether the caller may mount the admin surfaces (`/app/admin`,
    /// the ops `/api/*` routes). `role == admin`.
    pub admin_surface: bool,
    /// Whether the server keeps per-user search/answer history
    /// (`search_log`/`answer_log`/`clicks`). Local mode only.
    pub server_history: bool,
    /// Whether the archive surfaces (`POST /api/pages`, `/app/archive`)
    /// are live — `archive.enabled` AND a built fetcher.
    pub archiving: bool,
    /// Whether aggregate stats dashboards read server-wide telemetry.
    /// Local mode only — a public dashboard shows the instance card.
    pub shared_stats: bool,
}

/// `GET /api/instance` payload: the public-mode dashboard card fields
/// plus the SPA bootstrap knobs `GET /api/config` used to carry (config
/// itself goes admin-only in public mode, so the bootstrap reads here).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    /// Display name (`server.name`, default `"cauce"`).
    pub name: String,
    /// Build version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// Number of engines configured and enabled.
    pub engine_count: usize,
    /// Engine ids the SPA pin-checks its engine chips against.
    pub engine_ids: Vec<String>,
    /// Whether `POST /api/answer` is live (`ai.enabled` + provider built).
    pub ai_enabled: bool,
    /// Whether the results-page click beacon posts indexing requests
    /// (`archive.index_on_click` + a live archiver). Local mode only —
    /// public-mode clicks index into the browser-local store instead.
    pub index_on_click: bool,
}

/// Derive the request's role from the config + headers — the one place
/// token matching happens, shared by `require_admin` and the
/// `/api/capabilities` handler so the two can never disagree.
pub fn role_for(config: &Config, headers: &HeaderMap) -> Role {
    // Local mode is the single-user deployment: admin always.
    if !config.server.public_instance {
        return Role::Admin;
    }
    let Some(token) = bearer_token(headers) else {
        return Role::User;
    };
    // `!=` over a list this size is fine — admin_tokens is a handful of
    // static operator credentials, not a per-user store.
    if config.auth.admin_tokens.iter().any(|t| t.as_str() == token) {
        Role::Admin
    } else {
        Role::User
    }
}

/// `Authorization: Bearer <token>` — trimmed, `None` for any other shape.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}
