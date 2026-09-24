//! `ROUTES`: the single declaration of every HTTP surface (parent plan
//! section 6).
//!
//! The table lists every route the plan specifies, including surfaces later
//! waves implement: `wave` records the wave a row lands in and `requires`
//! the cargo feature it needs (`"ui"` for the HTMX pages, `"mcp"` for the
//! MCP endpoint). [`crate::app::build_router`] mounts the rows it has
//! handlers for and refuses to build when a wave-0 row is missing one, so a
//! route written into the plan but never wired (the v2 failure mode) fails
//! loudly. `tests/routes.rs` asserts this table, the live router and the
//! section-6 markdown table all agree.
//!
//! There is deliberately no Exa-shaped HTTP route (locked decision,
//! section 3): the only Exa consumer is the MCP `exa_search` tool alias.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

/// Surface kind of a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// JSON request/response (`/api/*`, `/health`, `/metrics`).
    Json,
    /// Server-rendered HTMX page.
    Html,
    /// `text/event-stream` endpoint.
    Sse,
    /// MCP streamable-HTTP endpoint (accepts every method once mounted).
    Mcp,
    /// Embedded static asset (`/favicon.ico`).
    Static,
}

/// One row of the section-6 wire table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteSpec {
    /// HTTP method: `"GET"`, `"POST"`, `"PUT"`, `"DELETE"`, or `"*"` (the
    /// MCP endpoint, which serves every method once mounted).
    pub method: &'static str,
    /// axum path pattern (`{param}` segments).
    pub path: &'static str,
    pub kind: RouteKind,
    /// Wave the route is introduced in (per the section-6 notes column).
    /// Rows ahead of the current wave are declared but not yet mounted.
    pub wave: u8,
    /// Cargo feature the route needs (see `app::feature_enabled`). `None`
    /// mounts in every build and every mode, including
    /// `cauce serve --headless`; `"ui"` rows additionally obey the runtime
    /// headless switch.
    pub requires: Option<&'static str>,
}

const fn json(method: &'static str, path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method,
        path,
        kind: RouteKind::Json,
        wave,
        requires: None,
    }
}

const fn sse(method: &'static str, path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method,
        path,
        kind: RouteKind::Sse,
        wave,
        requires: None,
    }
}

const fn html(path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method: "GET",
        path,
        kind: RouteKind::Html,
        wave,
        requires: Some("ui"),
    }
}

/// Every route of parent plan section 6. Mounted rows have a handler arm in
/// `app::handler_for`; rows without one are the declared future surface.
pub const ROUTES: &[RouteSpec] = &[
    // ---- wave 0: the JSON surface mounted by this step --------------------
    json("GET", "/api/search", 0),
    json("GET", "/api/history", 0),
    json("POST", "/api/click", 0),
    json("GET", "/api/stats", 0),
    json("GET", "/api/cache", 0),
    json("GET", "/api/cache/{key}", 0),
    json("DELETE", "/api/cache/{key}", 0),
    json("DELETE", "/api/cache", 0),
    json("GET", "/api/audit", 0),
    json("GET", "/health", 0),
    json("GET", "/api/config", 0),
    json("PUT", "/api/config", 0),
    // ---- wave 0 HTMX pages (W0-10 mounts them once templates exist) -------
    html("/", 0),
    html("/search", 0),
    // ---- wave 1 ------------------------------------------------------------
    json("GET", "/api/engines", 1),
    json("POST", "/api/engines/{id}/reset", 1),
    json("GET", "/metrics", 1),
    RouteSpec {
        method: "*",
        path: "/mcp",
        kind: RouteKind::Mcp,
        wave: 1,
        requires: Some("mcp"),
    },
    // ---- wave 2 ------------------------------------------------------------
    sse("GET", "/api/search/stream", 2),
    json("POST", "/api/engines/{id}/enable", 2),
    json("POST", "/api/engines/{id}/disable", 2),
    json("DELETE", "/api/history/{id}", 2),
    // The suggestions Url the W2-11 descriptor advertises; a wave-2
    // omission like the favicon (#150).
    json("GET", "/api/suggest", 2),
    html("/history", 2),
    html("/dashboard", 2),
    html("/cache", 2),
    html("/engines", 2),
    html("/audit", 2),
    html("/trace/{id}", 2),
    html("/settings", 2),
    html("/opensearch.xml", 2),
    // `GET /favicon.ico` was a wave-0 omission (#87): browsers request it on
    // every page load. It rides the `ui` gate like the pages — `rust-embed`
    // is a `ui` dependency and `--headless` serves no browser surface.
    RouteSpec {
        method: "GET",
        path: "/favicon.ico",
        kind: RouteKind::Static,
        wave: 2,
        requires: Some("ui"),
    },
    // ---- wave 4 ------------------------------------------------------------
    sse("POST", "/api/answer", 4),
    html("/answer", 4),
    // ---- wave 5 ------------------------------------------------------------
    json("POST", "/api/pages", 5),
    json("GET", "/api/pages/{url}", 5),
    json("GET", "/api/archive", 5),
    html("/archive", 5),
];
