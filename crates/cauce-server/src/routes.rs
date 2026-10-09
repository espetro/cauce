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

/// FX-07 authorisation class of a route (`auth` column): which rows
/// demand the `[auth] admin_tokens` credential once
/// `server.public_instance` flips the instance public. Local mode mounts
/// both classes identically — the gate is a no-op there, which is what
/// "local is today's behaviour bit-for-bit" means on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuth {
    /// No credential, either mode.
    Open,
    /// `Authorization: Bearer <admin token>` required in public mode
    /// (401 `unauthorized` otherwise); transparent in local mode.
    Admin,
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
    /// FX-07 authz class (see [`RouteAuth`]).
    pub auth: RouteAuth,
}

impl RouteSpec {
    /// Mark the row admin-gated for public instances (FX-07).
    const fn admin(self) -> Self {
        Self {
            auth: RouteAuth::Admin,
            ..self
        }
    }
}

const fn json(method: &'static str, path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method,
        path,
        kind: RouteKind::Json,
        wave,
        requires: None,
        auth: RouteAuth::Open,
    }
}

const fn sse(method: &'static str, path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method,
        path,
        kind: RouteKind::Sse,
        wave,
        requires: None,
        auth: RouteAuth::Open,
    }
}

const fn html(path: &'static str, wave: u8) -> RouteSpec {
    RouteSpec {
        method: "GET",
        path,
        kind: RouteKind::Html,
        wave,
        requires: Some("ui"),
        auth: RouteAuth::Open,
    }
}

/// Every route of parent plan section 6. Mounted rows have a handler arm in
/// `app::handler_for`; rows without one are the declared future surface.
pub const ROUTES: &[RouteSpec] = &[
    // ---- wave 0: the JSON surface mounted by this step --------------------
    json("GET", "/api/search", 0),
    // FX-07: the history surface is server-side per-user state — admin
    // in public mode (where nothing writes it anyway), open locally.
    json("GET", "/api/history", 0).admin(),
    // `/api/click` feeds the same history rows — same class.
    json("POST", "/api/click", 0).admin(),
    json("GET", "/api/stats", 0).admin(),
    // The cache listing maps query hashes to stored results — on a
    // public instance that is everyone's shared query corpus, so both
    // the reads and the writes sit behind the admin token.
    json("GET", "/api/cache", 0).admin(),
    json("GET", "/api/cache/{key}", 0).admin(),
    json("DELETE", "/api/cache/{key}", 0).admin(),
    json("DELETE", "/api/cache", 0).admin(),
    json("GET", "/api/audit", 0).admin(),
    json("GET", "/health", 0),
    // `/api/config` (even redacted) exposes the operator's full
    // configuration — admin in public mode; the SPA boots off
    // `/api/instance` instead.
    json("GET", "/api/config", 0).admin(),
    json("PUT", "/api/config", 0).admin(),
    // ---- wave 0 HTMX pages (W0-10 mounts them once templates exist) -------
    html("/", 0),
    html("/search", 0),
    // ---- wave 1 ------------------------------------------------------------
    // Engine inventory/levers are operator surfaces (ids, params,
    // health internals) — admin in public mode; the public card gets
    // its count from `/api/instance`.
    json("GET", "/api/engines", 1).admin(),
    json("POST", "/api/engines/{id}/reset", 1).admin(),
    json("GET", "/metrics", 1).admin(),
    RouteSpec {
        method: "*",
        path: "/mcp",
        kind: RouteKind::Mcp,
        wave: 1,
        requires: Some("mcp"),
        // `/mcp` exposes mutating tools (`cache_invalidate`,
        // `fetch_and_index`) — admin in public mode.
        auth: RouteAuth::Admin,
    },
    // ---- wave 2 ------------------------------------------------------------
    sse("GET", "/api/search/stream", 2),
    json("POST", "/api/engines/{id}/enable", 2).admin(),
    json("POST", "/api/engines/{id}/disable", 2).admin(),
    json("DELETE", "/api/history/{id}", 2).admin(),
    // #254: the answer-log counterpart — one audited row delete, plus
    // the read surface `GET /answer/{id}` renders. History state —
    // admin in public mode like `/api/history`.
    json("DELETE", "/api/answer-log/{id}", 2).admin(),
    json("GET", "/api/answer-log/{id}", 2).admin(),
    // The suggestions Url the W2-11 descriptor advertises; a wave-2
    // omission like the favicon (#150).
    json("GET", "/api/suggest", 2),
    // #240: the support-report download (`Content-Disposition:
    // attachment` + the `X-Report-Issue-Url` share header). The bundle
    // contains store paths, engine internals and uptime — admin in
    // public mode.
    json("GET", "/api/report", 2).admin(),
    // FX-05: `/history`..`/audit` moved under `/app`; `/trace/{id}` is
    // the last HTMX ops page (no JSON twin yet). Ops surface — admin
    // in public mode like `/api/audit`.
    html("/trace/{id}", 2).admin(),
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
        auth: RouteAuth::Open,
    },
    // ---- wave 4 ------------------------------------------------------------
    // `/api/answer` rides the `ai` gate like `/mcp` rides `mcp`: a build
    // without the feature has no answer loop to stream from.
    RouteSpec {
        method: "POST",
        path: "/api/answer",
        kind: RouteKind::Sse,
        wave: 4,
        requires: Some("ai"),
        auth: RouteAuth::Open,
    },
    // The page stays on `ui` alone: in an `ai`-less build it renders the
    // disabled notice (and links `/settings`) rather than 404ing.
    html("/answer", 4),
    // #254: durable answer URLs — `/answer/{id}` renders the stored
    // `answer_log` row server-side (no stream), so back/forward and
    // history links never re-run the loop. `ui`-gated like `/answer`.
    html("/answer/{id}", 4),
    // ---- wave 5 ------------------------------------------------------------
    // W5-01 mounts the fetch-and-index pair; `/api/archive` and `/archive`
    // are the W5-02 listing surface. `requires` names the `archive` cargo
    // feature so a minimal build keeps them unmounted.
    RouteSpec {
        method: "POST",
        path: "/api/pages",
        kind: RouteKind::Json,
        wave: 5,
        requires: Some("archive"),
        // Indexing spends fetch budget and writes the shared store —
        // admin in public mode (the per-user index lives client-side).
        auth: RouteAuth::Admin,
    },
    RouteSpec {
        method: "GET",
        path: "/api/pages/{url}",
        kind: RouteKind::Json,
        wave: 5,
        requires: Some("archive"),
        // Archived content is the shared store keyed by URL — open.
        auth: RouteAuth::Open,
    },
    // The GET must probe first: the live router check seeds one `pages`
    // row, and DELETE's probe removes it.
    RouteSpec {
        method: "DELETE",
        path: "/api/pages/{url}",
        kind: RouteKind::Json,
        wave: 5,
        requires: Some("archive"),
        auth: RouteAuth::Admin,
    },
    RouteSpec {
        method: "GET",
        path: "/api/archive",
        kind: RouteKind::Json,
        wave: 5,
        requires: Some("archive"),
        // The archive search surface reads shared content — open.
        auth: RouteAuth::Open,
    },
    // ---- FX: frontend replacement (W8 epic; plan
    // `2026-09-28-frontend-replacement.md` §5.1) -------------------------
    // FX-07: the instance-mode bootstrap pair — open reads in both
    // modes (they carry no secrets; the gates live on what they
    // describe). `/api/capabilities` reports the per-request role.
    json("GET", "/api/capabilities", 8),
    json("GET", "/api/instance", 8),
    // FX-02: `GET /app` serves the Svelte SPA shell; `/app/{*rest}`
    // serves the embedded hashed assets and falls back to the shell for
    // client-side routes. `ui`-gated like the HTMX pages: rust-embed is
    // a `ui` dependency and `--headless` serves no browser surface.
    html("/app", 8),
    html("/app/{*rest}", 8),
];
