//! The bookkeeping half of a protocol pump: the `ai_http` span every
//! provider exchange runs inside, plus metrics + the `ai.provider_call`
//! audit row recorded exactly once per call — ok, error, or a receiver
//! that went away. Both pumps fold their own stream events; the
//! recording contract is shared (moved here in W4-05).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tracing::info_span;
use uuid::Uuid;

use crate::ai::{DEFAULT_ACTOR, PROVIDER_CALL_ACTION, Usage};
use crate::metrics::Metrics;
use crate::store::{AuditRow, Store};

/// Everything a protocol pump task needs once the request is built.
pub(crate) struct PumpCtx {
    pub(crate) client: reqwest::Client,
    pub(crate) model: String,
    pub(crate) provider_host: String,
    pub(crate) metrics: Metrics,
    pub(crate) audit: Option<Arc<dyn Store>>,
    pub(crate) actor: Option<String>,
    pub(crate) request_id: Option<Uuid>,
}

impl PumpCtx {
    /// The `ai_http` span wrapping one provider exchange; `status`,
    /// `ms` and `outcome` fill in as the call settles.
    pub(crate) fn span(&self, url: &reqwest::Url) -> tracing::Span {
        info_span!(
            "ai_http",
            model = %self.model,
            url = %url,
            status = tracing::field::Empty,
            ms = tracing::field::Empty,
            outcome = tracing::field::Empty,
        )
    }

    /// Close out a call: stamp the span, record
    /// `cauce_ai_requests_total`/`cauce_ai_tokens_total`/
    /// `cauce_ai_duration_ms`, then write the audit row. Called exactly
    /// once per pump run, whatever the outcome.
    pub(crate) async fn record(
        &self,
        started: Instant,
        span: &tracing::Span,
        outcome: &'static str,
        usage: Option<Usage>,
    ) {
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        span.record("ms", ms);
        span.record("outcome", outcome);
        self.metrics
            .record_ai_request(&self.model, outcome, started.elapsed(), usage);
        self.write_audit(ms, outcome, usage).await;
    }

    /// Emit the `cauce.audit` event and write the `ai.provider_call`
    /// row — same emit-then-persist contract as cauce-server's
    /// `observability::audit`, minus the handler layer. The row carries
    /// model/tokens/ms/request_id; never the prompt.
    async fn write_audit(&self, ms: u64, outcome: &'static str, usage: Option<Usage>) {
        let Some(store) = &self.audit else {
            return;
        };
        let row = AuditRow {
            id: None,
            ts: Utc::now(),
            actor: self.actor.clone().unwrap_or_else(|| DEFAULT_ACTOR.into()),
            action: PROVIDER_CALL_ACTION.to_string(),
            target: self.model.clone(),
            details: serde_json::json!({
                "provider": self.provider_host,
                "tokens": usage.map(|u| serde_json::json!({
                    "prompt": u.prompt_tokens,
                    "completion": u.completion_tokens,
                    "total": u.total_tokens,
                })),
                "ms": ms,
                "outcome": outcome,
            }),
            request_id: self.request_id,
        };
        match row.request_id {
            Some(request_id) => tracing::info!(
                target: "cauce.audit",
                audit = true,
                actor = %row.actor,
                action = %row.action,
                audit_target = %row.target,
                request_id = %request_id,
                "audit"
            ),
            None => tracing::info!(
                target: "cauce.audit",
                audit = true,
                actor = %row.actor,
                action = %row.action,
                audit_target = %row.target,
                "audit"
            ),
        }
        if let Err(e) = store.audit(row).await {
            tracing::error!(
                target: "cauce.audit",
                audit = true,
                error = %e,
                "audit write failed"
            );
        }
    }
}
