use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_integration_contracts::StableRecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_WORK_PAGE_SIZE: u16 = 100;
pub const MAX_WORK_SOURCES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSourceMetadata {
    pub source_id: String,
    pub source_position: u64,
    pub observed_at: DateTime<Utc>,
    pub freshness: WorkFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItem {
    pub work_id: String,
    pub source_record: StableRecordRef,
    pub kind: String,
    pub stage: String,
    pub severity: String,
    pub assignee_ref: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub policy_version: String,
    pub source_command: Option<String>,
    pub source: WorkSourceMetadata,
    pub actionable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkQuery {
    pub principal_ref: String,
    pub scope: TenantScope,
    pub kinds: Vec<String>,
    pub stages: Vec<String>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSourcePage {
    pub source_id: String,
    pub items: Vec<WorkItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradedWorkSource {
    pub source_id: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnterpriseWorkPage {
    pub items: Vec<WorkItem>,
    pub degraded_sources: Vec<DegradedWorkSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WorkSourceError {
    #[error("source is not authorized")]
    Denied,
    #[error("source is unavailable")]
    Unavailable,
    #[error("source response is invalid")]
    Invalid,
}

#[async_trait]
pub trait WorkSourcePort: Send + Sync {
    fn source_id(&self) -> &'static str;
    async fn fetch(&self, query: &WorkQuery) -> Result<WorkSourcePage, WorkSourceError>;
}

#[derive(Default)]
pub struct EnterpriseHomeService {
    sources: Vec<Arc<dyn WorkSourcePort>>,
}

impl EnterpriseHomeService {
    pub fn try_new(sources: Vec<Arc<dyn WorkSourcePort>>) -> Result<Self, WorkSourceError> {
        let unique = sources
            .iter()
            .map(|source| source.source_id())
            .collect::<HashSet<_>>();
        if sources.is_empty()
            || sources.len() > MAX_WORK_SOURCES
            || unique.len() != sources.len()
            || unique.iter().any(|value| !valid_text(value))
        {
            return Err(WorkSourceError::Invalid);
        }
        Ok(Self { sources })
    }

    pub async fn fetch(&self, query: WorkQuery) -> Result<EnterpriseWorkPage, WorkSourceError> {
        if !valid_text(&query.principal_ref)
            || query.scope.validate().is_err()
            || query.page_size == 0
            || query.page_size > MAX_WORK_PAGE_SIZE
            || query
                .kinds
                .iter()
                .chain(&query.stages)
                .any(|value| !valid_text(value))
        {
            return Err(WorkSourceError::Invalid);
        }
        let mut items = Vec::new();
        let mut degraded_sources = Vec::new();
        let mut ids = HashSet::new();
        for source in &self.sources {
            match source.fetch(&query).await {
                Ok(page) => {
                    if page.source_id != source.source_id()
                        || page.items.len() > usize::from(query.page_size)
                        || page.items.iter().any(|item| {
                            item.source.source_id != page.source_id
                                || item.source_record.validate().is_err()
                                || !valid_text(&item.work_id)
                                || !valid_text(&item.kind)
                                || !valid_text(&item.stage)
                                || !valid_text(&item.policy_version)
                        })
                    {
                        degraded_sources.push(DegradedWorkSource {
                            source_id: source.source_id().into(),
                            retryable: false,
                        });
                        continue;
                    }
                    for mut item in page.items {
                        if item.source.freshness != WorkFreshness::Fresh {
                            item.actionable = false;
                        }
                        if ids.insert((item.source.source_id.clone(), item.work_id.clone())) {
                            items.push(item);
                        }
                    }
                }
                Err(WorkSourceError::Denied) => {
                    // Denied sources and counts are deliberately absent.
                }
                Err(WorkSourceError::Unavailable) => {
                    degraded_sources.push(DegradedWorkSource {
                        source_id: source.source_id().into(),
                        retryable: true,
                    });
                }
                Err(WorkSourceError::Invalid) => {
                    degraded_sources.push(DegradedWorkSource {
                        source_id: source.source_id().into(),
                        retryable: false,
                    });
                }
            }
        }
        items.sort_by_key(|item| {
            (
                item.due_at,
                item.source.source_id.clone(),
                item.work_id.clone(),
            )
        });
        items.truncate(usize::from(query.page_size));
        Ok(EnterpriseWorkPage {
            items,
            degraded_sources,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWorkCommand {
    pub source_id: String,
    pub work_id: String,
    pub command_type: String,
    pub expected_source_version: u64,
    pub idempotency_key: String,
    pub request_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWorkReceipt {
    pub source_id: String,
    pub source_receipt_ref: String,
    pub correlation_ref: String,
}

#[async_trait]
pub trait SourceWorkCommandPort: Send + Sync {
    async fn execute(
        &self,
        principal_ref: &str,
        scope: &TenantScope,
        command: SourceWorkCommand,
    ) -> Result<SourceWorkReceipt, WorkSourceError>;
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use erpx_integration_contracts::{
        StableRecordRef, catalog::ContextId, reference::RecordTypeId,
    };
    use erpx_kernel::RecordId;
    use uuid::Uuid;

    use super::*;

    struct Source {
        id: &'static str,
        result: Result<WorkSourcePage, WorkSourceError>,
    }
    #[async_trait]
    impl WorkSourcePort for Source {
        fn source_id(&self) -> &'static str {
            self.id
        }
        async fn fetch(&self, _query: &WorkQuery) -> Result<WorkSourcePage, WorkSourceError> {
            self.result.clone()
        }
    }
    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }
    fn record() -> StableRecordRef {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::new_v4()).unwrap(),
            ContextId::Finance,
            RecordTypeId::try_new("finance.invoice").unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn unavailable_source_degrades_while_denied_source_is_not_disclosed() {
        let observed = DateTime::from_timestamp(1_786_000_000, 0).unwrap();
        let service = EnterpriseHomeService::try_new(vec![
            Arc::new(Source {
                id: "finance",
                result: Ok(WorkSourcePage {
                    source_id: "finance".into(),
                    items: vec![WorkItem {
                        work_id: "approval-1".into(),
                        source_record: record(),
                        kind: "approval".into(),
                        stage: "pending".into(),
                        severity: "normal".into(),
                        assignee_ref: Some("reviewer".into()),
                        due_at: None,
                        policy_version: "policy-v3".into(),
                        source_command: Some("ApproveInvoice".into()),
                        source: WorkSourceMetadata {
                            source_id: "finance".into(),
                            source_position: 7,
                            observed_at: observed,
                            freshness: WorkFreshness::Stale,
                        },
                        actionable: true,
                    }],
                    next_cursor: None,
                }),
            }),
            Arc::new(Source {
                id: "legal",
                result: Err(WorkSourceError::Unavailable),
            }),
            Arc::new(Source {
                id: "hidden-module",
                result: Err(WorkSourceError::Denied),
            }),
        ])
        .unwrap();
        let page = service
            .fetch(WorkQuery {
                principal_ref: "reviewer".into(),
                scope: scope(),
                kinds: Vec::new(),
                stages: Vec::new(),
                page_size: 20,
                cursor: None,
            })
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert!(!page.items[0].actionable);
        assert_eq!(page.degraded_sources.len(), 1);
        assert_eq!(page.degraded_sources[0].source_id, "legal");
    }
}
