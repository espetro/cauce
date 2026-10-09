//! `export_bindings` — regenerates the TypeScript wire types under
//! `web/src/types/` from the `#[derive(TS)]` types (`cauce-core` response /
//! answer types, `cauce-server`'s `ApiError`). `mise run web` runs this test
//! and then `git diff --exit-code` to freshness-check the committed output;
//! run `cargo test -p cauce-server --test wire_types` after changing a wire
//! type. `.ts` files are the artifacts — nothing consumes them until the
//! step-3 rewrite (#214).
//!
//! `route_table_is_covered` — FX-01 enforcement: every `/api/*` `Json`/`Sse`
//! row in `ROUTES` must name its wire types in `WIRE_TABLE` below, and each
//! named type must have a committed `.ts` file. A new `/api/*` route fails
//! this test until its types are exported and mapped.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ts_rs::TS;

const OUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/web/src/types");

/// Both tests regenerate the same files and prepend banners to them —
/// run them serially or one test's banner pass can read a file the
/// other's export has truncated and commit banner-only corruption.
static EXPORT_LOCK: Mutex<()> = Mutex::new(());

/// The directory `export()` writes to. `web/src/types` under plain
/// `cargo test` (the [web] gate's regen path, where EXPORT_LOCK
/// serializes both tests inside one binary process). Under nextest each
/// test is its own process, the mutex cannot serialize them, and
/// `mise run validate` runs [test] concurrently with [web]'s
/// regen + `git diff` freshness check — so NEXTEST runs export to a
/// per-test scratch dir and never touch the committed tree.
fn out_dir() -> PathBuf {
    if std::env::var_os("NEXTEST").is_some() {
        std::env::temp_dir().join(format!("cauce-wire-types-{}", std::process::id()))
    } else {
        PathBuf::from(OUT_DIR)
    }
}

/// MPL-2.0 banner matching `web/build.mjs`'s `postBanner` convention.
const BANNER: &str = "// This Source Code Form is subject to the terms of the Mozilla Public\n\
                      // License, v. 2.0. If a copy of the MPL was not distributed with this\n\
                      // file, You can obtain one at <https://mozilla.org/MPL/2.0/>.\n\n";

/// `(method, path)` → the wire types a typed client needs for that route:
/// the request body first where one exists, then the response/envelope
/// type(s). Every enabled `/api/*` `Json`/`Sse` row of `ROUTES` must have
/// an entry; entries for routes behind a cargo feature must appear in the
/// feature's own block so feature-less builds keep agreeing.
const WIRE_TABLE: &[(&str, &str, &[&str])] = &[
    ("GET", "/api/search", &["SearchResponse"]),
    ("GET", "/api/search/stream", &["StreamMeta", "ResultsFrame"]),
    ("GET", "/api/history", &["HistoryItem"]),
    ("POST", "/api/click", &["ClickRow"]),
    ("GET", "/api/stats", &["StatsSnapshot"]),
    ("GET", "/api/cache", &["CachedSearch", "CacheListing"]),
    ("GET", "/api/cache/{key}", &["CachedSearch"]),
    ("DELETE", "/api/cache/{key}", &["CacheDeleteAck"]),
    ("DELETE", "/api/cache", &["CacheBulkDeleteAck"]),
    ("GET", "/api/audit", &["AuditRow"]),
    ("GET", "/api/config", &["Config"]),
    ("PUT", "/api/config", &["ConfigPutResponse"]),
    ("GET", "/api/engines", &["EngineView"]),
    ("POST", "/api/engines/{id}/reset", &["EngineHealthRow"]),
    ("POST", "/api/engines/{id}/enable", &["EngineToggleAck"]),
    ("POST", "/api/engines/{id}/disable", &["EngineToggleAck"]),
    ("DELETE", "/api/history/{id}", &["HistoryDeleteAck"]),
    ("DELETE", "/api/answer-log/{id}", &["AnswerLogDeleteAck"]),
    ("GET", "/api/answer-log/{id}", &["AnswerLogRow"]),
    ("GET", "/api/suggest", &["SuggestResponse"]),
    ("GET", "/api/report", &["ReportBundle"]),
    // FX-07: the instance-mode bootstrap pair.
    ("GET", "/api/capabilities", &["Capabilities"]),
    ("GET", "/api/instance", &["InstanceInfo"]),
    #[cfg(feature = "ai")]
    ("POST", "/api/answer", &["AnswerBody", "AnswerFrame"]),
    #[cfg(feature = "archive")]
    ("POST", "/api/pages", &["IndexBody", "PageRow"]),
    #[cfg(feature = "archive")]
    ("GET", "/api/pages/{url}", &["PageRow"]),
    #[cfg(feature = "archive")]
    ("DELETE", "/api/pages/{url}", &["PageDeleteAck"]),
    #[cfg(feature = "archive")]
    ("GET", "/api/archive", &["ArchiveResponse"]),
];

/// `RouteSpec::requires` satisfied by this build. Feature names map 1:1 to
/// cargo features of `cauce-server`; `None` mounts everywhere. `/api/*`
/// `Json`/`Sse` rows are gated on `ai`/`archive` only — anything else
/// (currently none) reports disabled so the coverage assert forces a
/// decision.
fn route_enabled(requires: Option<&'static str>) -> bool {
    let Some(f) = requires else { return true };
    if f == "ai" {
        return cfg!(feature = "ai");
    }
    if f == "archive" {
        return cfg!(feature = "archive");
    }
    false
}

fn export(out_dir: &Path) {
    let cfg = ts_rs::Config::new().with_out_dir(out_dir);
    // Top-level wire shapes; `export_all` emits every transitive
    // dependency (SearchResult, EngineId, EngineStatus, ...) as its own
    // file.
    for result in [
        cauce_core::SearchResponse::export_all(&cfg),
        cauce_core::StreamMeta::export_all(&cfg),
        cauce_core::ResultsFrame::export_all(&cfg),
        cauce_core::AnswerFrame::export_all(&cfg),
        // W7-04: the client-replayed thread turns on `AnswerBody` —
        // `answer.ts` imports this for the `history` payload.
        cauce_core::AnswerTurn::export_all(&cfg),
        cauce_server::ApiError::export_all(&cfg),
        // FX-01 wire table: every `/api/*` response shape.
        cauce_core::HistoryItem::export_all(&cfg),
        cauce_core::ClickRow::export_all(&cfg),
        cauce_core::StatsSnapshot::export_all(&cfg),
        cauce_core::CachedSearch::export_all(&cfg),
        cauce_core::AuditRow::export_all(&cfg),
        cauce_core::config::Config::export_all(&cfg),
        cauce_core::EngineHealthRow::export_all(&cfg),
        cauce_core::AnswerLogRow::export_all(&cfg),
        cauce_core::PageRow::export_all(&cfg),
        cauce_core::report::ReportBundle::export_all(&cfg),
        cauce_server::CacheListing::export_all(&cfg),
        cauce_server::CacheDeleteAck::export_all(&cfg),
        cauce_server::CacheBulkDeleteAck::export_all(&cfg),
        cauce_server::ConfigPutResponse::export_all(&cfg),
        cauce_server::EngineView::export_all(&cfg),
        cauce_server::EngineToggleAck::export_all(&cfg),
        cauce_server::HistoryDeleteAck::export_all(&cfg),
        cauce_server::AnswerLogDeleteAck::export_all(&cfg),
        cauce_server::SuggestResponse::export_all(&cfg),
        // FX-07: instance modes — `export_all` on `Capabilities` emits
        // `CapabilityFlags`/`InstanceMode`/`Role` transitively.
        cauce_server::Capabilities::export_all(&cfg),
        cauce_server::InstanceInfo::export_all(&cfg),
    ] {
        result.expect("binding export");
    }
    #[cfg(feature = "ai")]
    cauce_server::AnswerBody::export_all(&cfg).expect("binding export");
    #[cfg(feature = "archive")]
    {
        cauce_server::IndexBody::export_all(&cfg).expect("binding export");
        cauce_server::PageDeleteAck::export_all(&cfg).expect("binding export");
        cauce_server::ArchiveResponse::export_all(&cfg).expect("binding export");
    }
}

/// ts-rs writes no license header; prepend the MPL banner the repo
/// requires on committed files. Recurses so nested outputs
/// (`serde_json/JsonValue.ts` and friends) get it too.
fn banner_pass(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("web/src/types exists") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            banner_pass(&path);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("ts") {
            continue;
        }
        let body = std::fs::read_to_string(&path).unwrap();
        if !body.starts_with(BANNER) {
            std::fs::write(&path, format!("{BANNER}{body}")).unwrap();
        }
    }
}

#[test]
fn export_bindings() {
    let _guard = EXPORT_LOCK.lock().unwrap();
    let out = out_dir();
    export(&out);
    banner_pass(&out);
    assert!(
        out.join("SearchResult.ts").exists(),
        "expected SearchResult.ts to be written"
    );
}

#[test]
fn route_table_is_covered() {
    let _guard = EXPORT_LOCK.lock().unwrap();
    let out = out_dir();
    export(&out);
    banner_pass(&out);

    // Every enabled /api/* Json/Sse row has a WIRE_TABLE entry.
    let mut uncovered = Vec::new();
    for route in cauce_server::ROUTES {
        if !route.path.starts_with("/api/")
            || !matches!(
                route.kind,
                cauce_server::RouteKind::Json | cauce_server::RouteKind::Sse
            )
            || !route_enabled(route.requires)
        {
            continue;
        }
        if !WIRE_TABLE
            .iter()
            .any(|(m, p, _)| *m == route.method && *p == route.path)
        {
            uncovered.push(format!("{} {}", route.method, route.path));
        }
    }
    assert!(
        uncovered.is_empty(),
        "ROUTES /api/* rows missing WIRE_TABLE entries: {uncovered:?}"
    );

    // Every WIRE_TABLE entry maps a real route row, and each named type
    // has a committed .ts file under web/src/types.
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    for &(method, path, types) in WIRE_TABLE {
        let route = cauce_server::ROUTES
            .iter()
            .find(|r| r.method == method && r.path == path)
            .unwrap_or_else(|| panic!("WIRE_TABLE maps a route ROUTES lacks: {method} {path}"));
        if !route_enabled(route.requires) {
            stale.push(format!(
                "{method} {path} (feature {})",
                route.requires.unwrap_or("?")
            ));
            continue;
        }
        for ty in types {
            if !PathBuf::from(OUT_DIR).join(format!("{ty}.ts")).exists() {
                missing.push(format!("{method} {path} -> {ty}"));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "WIRE_TABLE rows for disabled routes: {stale:?}"
    );
    assert!(
        missing.is_empty(),
        "wire types declared but not exported: {missing:?}"
    );
}
