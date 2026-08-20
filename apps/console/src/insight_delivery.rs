//! Typed, same-origin delivery boundary for authorized operational insights.
//!
//! Insight values are derived only from bounded-context owner records already
//! accepted by the authorized work composition. Browser input is limited to
//! presentation paging, locale, and consistency; session authority remains
//! server-owned.

#[cfg(any(feature = "server", test))]
use chrono::DateTime;
use dioxus::prelude::dioxus_fullstack;
#[cfg(feature = "server")]
use dioxus::prelude::dioxus_server;
use erpx_delivery_contract::{DeliveryError, DeliveryErrorMeta};
#[cfg(any(feature = "server", test))]
use erpx_ui_core::{MAX_WORK_PAGE_SIZE, WorkQueueView};
use erpx_ui_core::{ReadModelAvailability, ReadModelFreshness, WorkDomain};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const INSIGHT_METRIC_COUNT: usize = 4;
const MAX_INSIGHT_PAGE_SIZE: u32 = 8;
#[cfg(any(feature = "server", test))]
const MAX_INSIGHT_TEXT_BYTES: usize = 512;
#[cfg(any(feature = "server", test))]
const MAX_INSIGHT_REF_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightConsistency {
    Eventual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedInsightRequest {
    pub cursor: Option<String>,
    pub page_size: u32,
    pub locale: String,
    pub consistency: InsightConsistency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightMetricKind {
    ActiveWork,
    Approvals,
    Exceptions,
    Overdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightUnit {
    Items,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsightMetric {
    pub key: String,
    pub kind: InsightMetricKind,
    pub value: u32,
    pub unit: InsightUnit,
    pub source: String,
    pub source_count: u32,
    pub measured_at: String,
    pub freshness: ReadModelFreshness,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsightSourceEvidence {
    pub domain: WorkDomain,
    pub availability: ReadModelAvailability,
    pub freshness: ReadModelFreshness,
    pub item_count: u32,
    pub measured_at: String,
    pub has_projection_evidence: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedInsightPage {
    pub scope: String,
    pub measured_at: String,
    pub availability: ReadModelAvailability,
    pub freshness: ReadModelFreshness,
    pub reason_code: Option<String>,
    pub metrics: Vec<InsightMetric>,
    pub sources: Vec<InsightSourceEvidence>,
    pub projection_cursors: Vec<crate::work_delivery::WorkProjectionCursor>,
    pub page_size: u32,
    pub next_cursor: Option<String>,
    pub redacted_fields: Vec<String>,
    pub authorization_context_version: Option<u64>,
    pub correlation_id: Uuid,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn error_meta(retryable: bool, retry_after_ms: Option<u64>) -> DeliveryErrorMeta {
    DeliveryErrorMeta {
        correlation_id: Uuid::new_v4(),
        retry_after_ms,
        retryable,
    }
}

pub fn valid_insight_cursor(value: &str) -> bool {
    value
        .strip_prefix("offset:")
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|offset| offset < INSIGHT_METRIC_COUNT)
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
fn valid_request(request: &AuthorizedInsightRequest) -> bool {
    matches!(request.locale.as_str(), "en" | "th")
        && request.page_size > 0
        && request.page_size <= MAX_INSIGHT_PAGE_SIZE
        && request.cursor.as_deref().is_none_or(valid_insight_cursor)
}

#[cfg(any(feature = "server", test))]
fn valid_text(value: &str, maximum: usize, allow_empty: bool) -> bool {
    (allow_empty || !value.trim().is_empty())
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

#[cfg(any(feature = "server", test))]
fn valid_reason(value: &str) -> bool {
    valid_text(value, MAX_INSIGHT_REF_BYTES, false)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(any(feature = "server", test))]
fn projection_matches(domain: WorkDomain, source: &str) -> bool {
    match domain {
        WorkDomain::Operations => source.starts_with("operations."),
        WorkDomain::Support => source.starts_with("support."),
        WorkDomain::Finance => source.starts_with("finance."),
        WorkDomain::People => source.starts_with("people."),
        WorkDomain::Maintenance => source.starts_with("maintenance."),
        WorkDomain::Procurement => source.starts_with("procurement."),
    }
}

#[cfg(any(feature = "server", test))]
impl InsightMetric {
    fn is_bounded(&self) -> bool {
        valid_reason(&self.key)
            && valid_reason(&self.source)
            && self.source_count <= erpx_ui_core::MAX_WORK_SOURCE_CONTRIBUTIONS as u32
            && valid_text(&self.measured_at, 64, false)
            && DateTime::parse_from_rfc3339(&self.measured_at).is_ok()
            && self.target_path.starts_with("/work")
            && valid_text(&self.target_path, MAX_INSIGHT_TEXT_BYTES, false)
    }
}

#[cfg(any(feature = "server", test))]
impl InsightSourceEvidence {
    fn is_bounded(&self) -> bool {
        self.item_count <= erpx_ui_core::MAX_WORK_ITEMS as u32
            && valid_text(&self.measured_at, 64, false)
            && DateTime::parse_from_rfc3339(&self.measured_at).is_ok()
            && self.reason_code.as_deref().is_none_or(valid_reason)
    }
}

#[cfg(any(feature = "server", test))]
impl AuthorizedInsightPage {
    fn is_bounded(&self) -> bool {
        self.page_size > 0
            && self.page_size <= MAX_INSIGHT_PAGE_SIZE
            && self.metrics.len() <= self.page_size as usize
            && self.metrics.len() <= INSIGHT_METRIC_COUNT
            && self.sources.len() <= erpx_ui_core::MAX_WORK_SOURCE_CONTRIBUTIONS
            && self.projection_cursors.len() <= erpx_ui_core::MAX_WORK_SOURCE_CONTRIBUTIONS
            && self.redacted_fields.len() <= 16
            && valid_text(&self.scope, MAX_INSIGHT_TEXT_BYTES, false)
            && valid_text(&self.measured_at, 64, false)
            && DateTime::parse_from_rfc3339(&self.measured_at).is_ok()
            && self.reason_code.as_deref().is_none_or(valid_reason)
            && self.next_cursor.as_deref().is_none_or(valid_insight_cursor)
            && self.metrics.iter().all(InsightMetric::is_bounded)
            && self.sources.iter().all(InsightSourceEvidence::is_bounded)
            && self.redacted_fields.iter().all(|value| valid_reason(value))
    }
}

#[cfg(any(feature = "server", test))]
fn page_reason(
    availability: ReadModelAvailability,
    sources: &[InsightSourceEvidence],
) -> Option<String> {
    match availability {
        ReadModelAvailability::Unavailable if sources.is_empty() => {
            Some("no_authorized_insight_contributor".into())
        }
        ReadModelAvailability::Unavailable => sources
            .iter()
            .find_map(|source| source.reason_code.clone())
            .or_else(|| Some("all_contributors_unavailable".into())),
        ReadModelAvailability::Partial => Some("contributor_partial".into()),
        ReadModelAvailability::Available | ReadModelAvailability::Empty => None,
    }
}

#[cfg(any(feature = "server", test))]
fn build_page(
    work: crate::work_delivery::AuthorizedWorkPage,
    request: &AuthorizedInsightRequest,
) -> Result<AuthorizedInsightPage, DeliveryError> {
    let measured_at = work.queue.meta.as_of.clone();
    let source_count = work
        .queue
        .contributions
        .iter()
        .filter(|source| source.availability != ReadModelAvailability::Unavailable)
        .count() as u32;
    let sources = work
        .queue
        .contributions
        .iter()
        .map(|source| InsightSourceEvidence {
            domain: source.domain,
            availability: source.availability,
            freshness: source.freshness,
            item_count: source.item_count,
            measured_at: measured_at.clone(),
            has_projection_evidence: work
                .projection_cursors
                .iter()
                .any(|cursor| projection_matches(source.domain, &cursor.source)),
            reason_code: source.reason_code.clone(),
        })
        .collect::<Vec<_>>();
    let all_metrics = if work.queue.meta.availability == ReadModelAvailability::Unavailable {
        Vec::new()
    } else {
        let counters = work.queue.counters;
        [
            (
                "active_work",
                InsightMetricKind::ActiveWork,
                counters.total,
                "/work",
            ),
            (
                "approvals",
                InsightMetricKind::Approvals,
                counters.approvals,
                "/work?view=approvals",
            ),
            (
                "exceptions",
                InsightMetricKind::Exceptions,
                counters.exceptions,
                "/work",
            ),
            (
                "overdue",
                InsightMetricKind::Overdue,
                counters.overdue,
                "/work?view=overdue",
            ),
        ]
        .into_iter()
        .map(|(key, kind, value, target_path)| InsightMetric {
            key: key.into(),
            kind,
            value,
            unit: InsightUnit::Items,
            source: "authorized_work_contributors".into(),
            source_count,
            measured_at: measured_at.clone(),
            freshness: work.queue.meta.freshness,
            target_path: target_path.into(),
        })
        .collect::<Vec<_>>()
    };
    let offset = request
        .cursor
        .as_deref()
        .and_then(|cursor| cursor.strip_prefix("offset:"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let has_more = offset.saturating_add(request.page_size as usize) < all_metrics.len();
    let metrics = all_metrics
        .into_iter()
        .skip(offset)
        .take(request.page_size as usize)
        .collect::<Vec<_>>();
    let availability = work.queue.meta.availability;
    let page = AuthorizedInsightPage {
        scope: work.queue.meta.scope,
        measured_at,
        availability,
        freshness: work.queue.meta.freshness,
        reason_code: page_reason(availability, &sources),
        metrics,
        sources,
        projection_cursors: work.projection_cursors,
        page_size: request.page_size,
        next_cursor: has_more.then(|| format!("offset:{}", offset + request.page_size as usize)),
        redacted_fields: vec![
            "access_token".into(),
            "customer_identity".into(),
            "owner_internal_payload".into(),
            "metric_record_details".into(),
        ],
        authorization_context_version: work.authorization_context_version,
        correlation_id: work.correlation_id,
    };
    if !page.is_bounded() {
        return Err(DeliveryError::Internal(error_meta(false, None)));
    }
    Ok(page)
}

#[dioxus::prelude::server(endpoint = "/api/server-fn/insights/list")]
pub async fn list_authorized_insights(
    request: AuthorizedInsightRequest,
) -> dioxus::prelude::ServerFnResult<Result<AuthorizedInsightPage, DeliveryError>> {
    if !valid_request(&request) {
        return Ok(Err(DeliveryError::Validation(error_meta(false, None))));
    }
    #[cfg(feature = "server")]
    {
        let work =
            match crate::work_delivery::server::list(crate::work_delivery::AuthorizedWorkRequest {
                view: WorkQueueView::All,
                domain: None,
                severity: None,
                cursor: None,
                page_size: MAX_WORK_PAGE_SIZE,
                locale: request.locale.clone(),
                consistency: crate::work_delivery::WorkConsistency::Eventual,
            })
            .await
            {
                Ok(work) => work,
                Err(error) => return Ok(Err(error)),
            };
        return Ok(build_page(work, &request));
    }
    #[allow(unreachable_code)]
    Ok(Err(DeliveryError::Unavailable(error_meta(true, Some(500)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_core::{
        WorkQueueCounters, WorkQueueMeta, WorkQueueReadModel, WorkSourceContribution,
    };

    fn request() -> AuthorizedInsightRequest {
        AuthorizedInsightRequest {
            cursor: None,
            page_size: 4,
            locale: "en".into(),
            consistency: InsightConsistency::Eventual,
        }
    }

    fn work_page(availability: ReadModelAvailability) -> crate::work_delivery::AuthorizedWorkPage {
        let successful = availability != ReadModelAvailability::Unavailable;
        crate::work_delivery::AuthorizedWorkPage {
            queue: WorkQueueReadModel {
                view: WorkQueueView::All,
                meta: WorkQueueMeta {
                    scope: "platform".into(),
                    as_of: "2026-08-21T12:00:00Z".into(),
                    freshness: if successful {
                        ReadModelFreshness::Current
                    } else {
                        ReadModelFreshness::Unknown
                    },
                    availability,
                },
                counters: WorkQueueCounters {
                    total: 7,
                    overdue: 2,
                    due_today: 1,
                    approvals: 3,
                    exceptions: 4,
                    waiting: 1,
                },
                items: Vec::new(),
                contributions: vec![WorkSourceContribution {
                    domain: WorkDomain::Support,
                    label: "must not be copied into insight presentation".into(),
                    availability: if successful {
                        ReadModelAvailability::Available
                    } else {
                        ReadModelAvailability::Unavailable
                    },
                    freshness: if successful {
                        ReadModelFreshness::Current
                    } else {
                        ReadModelFreshness::Unknown
                    },
                    item_count: if successful { 7 } else { 0 },
                    message: "owner message stays outside insight metrics".into(),
                    reason_code: (!successful).then(|| "owner_unavailable".into()),
                }],
                page_size: MAX_WORK_PAGE_SIZE,
                returned_count: 0,
                next_cursor: None,
            },
            projection_cursors: Vec::new(),
            redacted_fields: Vec::new(),
            authorization_context_version: Some(9),
            correlation_id: Uuid::nil(),
        }
    }

    #[test]
    fn insight_request_accepts_only_bounded_filters() {
        assert!(valid_request(&request()));
        let mut invalid = request();
        invalid.cursor = Some("https://example.test/leak".into());
        assert!(!valid_request(&invalid));
        invalid = request();
        invalid.page_size = MAX_INSIGHT_PAGE_SIZE + 1;
        assert!(!valid_request(&invalid));
        invalid = request();
        invalid.locale = "th-TH".into();
        assert!(!valid_request(&invalid));
    }

    #[test]
    fn metrics_are_derived_only_when_an_authoritative_source_responds() {
        let available =
            build_page(work_page(ReadModelAvailability::Available), &request()).unwrap();
        assert_eq!(available.metrics.len(), 4);
        assert_eq!(available.metrics[0].value, 7);
        assert_eq!(available.metrics[3].value, 2);
        assert_eq!(available.metrics[0].source_count, 1);

        let unavailable =
            build_page(work_page(ReadModelAvailability::Unavailable), &request()).unwrap();
        assert!(unavailable.metrics.is_empty());
        assert_eq!(
            unavailable.reason_code.as_deref(),
            Some("owner_unavailable")
        );
    }

    #[test]
    fn insight_metric_pagination_is_bounded_and_stable() {
        let mut second_page = request();
        second_page.page_size = 2;
        let first = build_page(work_page(ReadModelAvailability::Available), &second_page).unwrap();
        assert_eq!(first.metrics.len(), 2);
        assert_eq!(first.next_cursor.as_deref(), Some("offset:2"));
        second_page.cursor = first.next_cursor;
        let second = build_page(work_page(ReadModelAvailability::Available), &second_page).unwrap();
        assert_eq!(second.metrics[0].kind, InsightMetricKind::Exceptions);
        assert!(second.next_cursor.is_none());
    }
}
