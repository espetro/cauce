//! Shared English UI copy for the server pages.
//!
//! One shape across pages (`decisions.md`): `common` holds cross-page
//! literals; each page module exposes flat `pub const` items referenced from
//! templates as `crate::strings::<page>::NAME`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub mod common {
    pub const BRAND: &str = "cauce";
    pub const DASH: &str = "-";
    pub const NAV_SEARCH: &str = "search";
}

pub mod settings {
    pub const PAGE_TITLE: &str = "Settings";
    pub const EDITING: &str = "editing";
    pub const RESTART_NOTE: &str = "changes apply on restart";

    pub const SECTION_SEARCH: &str = "Search";
    pub const DEADLINE: &str = "Deadline (ms)";
    pub const TTL: &str = "Cache TTL (s)";
    pub const HEDGE: &str = "Hedge threshold (ms)";
    pub const HEDGE_WAVE: &str = "lands in wave 3";

    pub const SECTION_ENGINES: &str = "Engines";
    pub const ENGINES_PINNED: &str = "enabled flags are pinned by CAUCE_ENGINES";
    pub const ENABLED: &str = "enabled";
    pub const DISABLED: &str = "disabled";
    pub const TIER: &str = "tier";
    pub const TIER_DEFAULT: &str = "default";
    pub const PROXY: &str = "egress proxy";
    pub const PROXY_PLACEHOLDER: &str = "direct";
    pub const BROWSE_ENGINES: &str = "browse engines";

    pub const SECTION_ADMISSION: &str = "Admission";
    pub const MAX_WAIT: &str = "Max queue wait (ms)";
    pub const MAX_CONCURRENT: &str = "Max concurrent per engine";

    pub const SECTION_LOGGING: &str = "Logging";
    pub const RETENTION: &str = "JSONL retention (days)";

    pub const SECTION_CACHE: &str = "Cache";
    pub const ENTRIES: &str = "entries";
    pub const UNEXPIRED: &str = "unexpired";
    pub const NEWEST: &str = "newest";
    pub const DELETE_EXPIRED: &str = "delete expired";
    pub const DELETE_ALL: &str = "delete all";
    pub const CONFIRM_EXPIRED: &str = "Delete every expired cache entry?";
    pub const CONFIRM_ALL: &str =
        "Delete ALL cache entries? Every next search will hit the network.";
    pub const BROWSE_ENTRIES: &str = "browse entries";

    pub const SECTION_AI: &str = "AI answers";
    pub const AI_WAVE: &str = "lands in wave 4";
    pub const AI_BASE_URL: &str = "Base URL";
    pub const AI_API_KEY: &str = "API key";
    pub const AI_API_KEY_PLACEHOLDER: &str = "${env:BIFROST_API_KEY}";
    pub const AI_MODEL: &str = "Model";
    pub const AI_MODEL_PLACEHOLDER: &str = "model name";
    pub const MODELS_UNREACHABLE: &str = "model list unreachable; type a model name";
    pub const AI_PIPELINE_NOTE: &str = "(the answer pipeline arrives in wave 4)";

    pub const SET_BY: &str = "set by";
    pub const IS_SET: &str = "is set";
    pub const IS_NOT_SET: &str = "is not set";

    pub const SAVE: &str = "Save";
    pub const SAVED: &str = "saved";
    pub const NOT_SAVED: &str = "not saved:";
    pub const ERROR_ONE: &str = "error";
    pub const ERROR_MANY: &str = "errors";
    pub const COULD_NOT_SAVE: &str = "error: could not save";
    pub const NOSCRIPT: &str =
        "Saving needs JavaScript (the form issues PUT /api/config via htmx).";
}
