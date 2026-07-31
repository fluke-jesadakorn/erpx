//! Tenant-routed Legal/document read projection worker.

use std::collections::HashSet;

use erpx_domain::legal::{
    ContractStatus, LegalProjectionReconciliation, MAX_DOCUMENT_VERSIONS, MAX_LEGAL_PARTIES,
    MAX_LEGAL_PURPOSE_BYTES, MAX_LEGAL_REFERENCE_BYTES, MAX_LEGAL_TITLE_BYTES,
};
use erpx_infrastructure::{
    LegalAggregateType, LegalEventType, LegalOutboxPayload, LegalProjectionRebuildPreparation,
    LegalSourceEvent, LegalSourceEventCursor, LegalSourceEventPage,
    MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE, PersistenceError, RoutedDatabaseSide, RoutedTenantDatabase,
    SqlxLegalProjectionStore, TenantDatabaseRouter,
};
use thiserror::Error;

pub const LEGAL_PROJECTION_HANDLER_NAME: &str = "legal_documents_projection";
pub const LEGAL_PROJECTION_MAX_PAGE_SIZE: u32 = MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE;
pub const LEGAL_PROJECTION_MAX_EVENTS: u64 = erpx_infrastructure::MAX_LEGAL_PROJECTION_EVENTS;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LegalProjectionWorkerError {
    #[error("Legal projection configuration error: {0}")]
    Configuration(String),
    #[error("Legal projection {operation} failed")]
    Projection { operation: &'static str },
}

impl From<PersistenceError> for LegalProjectionWorkerError {
    fn from(_: PersistenceError) -> Self {
        Self::Projection {
            operation: "operation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionRequest {
    pub tenant_id: String,
    pub holding_ref: String,
    pub cursor: Option<LegalSourceEventCursor>,
    pub page_size: u32,
}

impl LegalProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        cursor: Option<LegalSourceEventCursor>,
        page_size: u32,
    ) -> Result<Self, LegalProjectionWorkerError> {
        let request = Self {
            tenant_id: tenant_id.into(),
            holding_ref: holding_ref.into(),
            cursor,
            page_size,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), LegalProjectionWorkerError> {
        validate_reference(&self.tenant_id, "tenant_id")?;
        validate_reference(&self.holding_ref, "holding_ref")?;
        if self.page_size == 0 || self.page_size > LEGAL_PROJECTION_MAX_PAGE_SIZE {
            return Err(LegalProjectionWorkerError::Configuration(format!(
                "Legal projection page size must be 1..={LEGAL_PROJECTION_MAX_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = &self.cursor
            && (cursor.aggregate_type.trim().is_empty()
                || cursor.aggregate_type.len() > 64
                || cursor.aggregate_id.is_nil()
                || cursor.aggregate_version == 0
                || !matches!(
                    cursor.aggregate_type.as_str(),
                    "legal_contract" | "legal_document" | "legal_access_history"
                ))
        {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection cursor is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionRunReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_seen: u64,
    pub events_applied: u64,
    pub next_cursor: Option<LegalSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionRebuildReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub events_replayed: u64,
    pub preparation: LegalProjectionRebuildPreparation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionReconciliationReport {
    pub tenant_id: String,
    pub holding_ref: String,
    pub reconciliation: LegalProjectionReconciliation,
}

#[derive(Debug, Clone)]
pub struct LegalProjectionRunner {
    router: TenantDatabaseRouter,
}

impl LegalProjectionRunner {
    pub fn new(router: TenantDatabaseRouter) -> Self {
        Self { router }
    }

    pub async fn project_page(
        &self,
        request: LegalProjectionRequest,
    ) -> Result<LegalProjectionRunReport, LegalProjectionWorkerError> {
        request.validate()?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxLegalProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => async {
                let mut cursor = request.cursor.clone();
                let mut events_seen = 0_u64;
                let mut events_applied = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), request.page_size)
                        .await?;
                    validate_source_event_page(cursor.as_ref(), &page, request.page_size)
                        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
                    events_seen = next_legal_event_count(events_seen, page.events.len())?;
                    for event in &page.events {
                        if store.project_event(event).await?.applied {
                            events_applied = events_applied.saturating_add(1);
                        }
                    }
                    if !page.has_more {
                        break;
                    }
                    cursor = page.next_cursor;
                }
                Ok::<LegalProjectionRunReport, PersistenceError>(LegalProjectionRunReport {
                    tenant_id: request.tenant_id.clone(),
                    holding_ref: request.holding_ref.clone(),
                    events_seen,
                    events_applied,
                    next_cursor: None,
                    has_more: false,
                })
            }
            .await
            .map_err(|_| redacted_error("projection page")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    pub async fn rebuild(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
        page_size: u32,
    ) -> Result<LegalProjectionRebuildReport, LegalProjectionWorkerError> {
        let request = LegalProjectionRequest::new(tenant_id, holding_ref, None, page_size)?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxLegalProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => async {
                let source_event_count = store.source_event_high_watermark().await?;
                if source_event_count > LEGAL_PROJECTION_MAX_EVENTS {
                    return Err(PersistenceError::InvalidData(
                        "Legal projection source event bound exceeded".into(),
                    ));
                }
                let preparation = store.clear_projection().await?;
                let mut cursor = None;
                let mut events_seen = 0_u64;
                let mut events_replayed = 0_u64;
                loop {
                    let page = store
                        .read_source_event_page(cursor.as_ref(), request.page_size)
                        .await?;
                    validate_source_event_page(cursor.as_ref(), &page, request.page_size)
                        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
                    events_seen = next_legal_event_count(events_seen, page.events.len())?;
                    for event in &page.events {
                        if store.project_event(event).await?.applied {
                            events_replayed = events_replayed.saturating_add(1);
                        }
                    }
                    if !page.has_more {
                        break;
                    }
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        return Err(PersistenceError::InvalidData(
                            "Legal source page omitted its continuation cursor".into(),
                        ));
                    }
                }
                let reconciliation = store.reconcile_projection().await?;
                if !reconciliation.is_consistent() {
                    return Err(PersistenceError::InvalidData(
                        "Legal projection rebuild did not converge with the authoritative source"
                            .into(),
                    ));
                }
                Ok::<LegalProjectionRebuildReport, PersistenceError>(LegalProjectionRebuildReport {
                    tenant_id: request.tenant_id.clone(),
                    holding_ref: request.holding_ref.clone(),
                    events_replayed,
                    preparation,
                })
            }
            .await
            .map_err(|_| redacted_error("projection rebuild")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    /// Reconcile authoritative Legal/document source events with the
    /// disposable read checkpoints without mutating either side.
    pub async fn reconcile(
        &self,
        tenant_id: impl Into<String>,
        holding_ref: impl Into<String>,
    ) -> Result<LegalProjectionReconciliationReport, LegalProjectionWorkerError> {
        let request = LegalProjectionRequest::new(tenant_id, holding_ref, None, 1)?;
        let (write, read) = self
            .route_store(&request.tenant_id, &request.holding_ref)
            .await?;
        let result = match SqlxLegalProjectionStore::from_pools(
            write.database_pool(),
            read.database_pool(),
            request.holding_ref.clone(),
        ) {
            Ok(store) => store
                .reconcile_projection()
                .await
                .map(|reconciliation| LegalProjectionReconciliationReport {
                    tenant_id: request.tenant_id.clone(),
                    holding_ref: request.holding_ref.clone(),
                    reconciliation,
                })
                .map_err(|_| redacted_error("reconciliation")),
            Err(_) => Err(redacted_error("store construction")),
        };
        close_routes(write, read).await;
        result
    }

    async fn route_store(
        &self,
        tenant_id: &str,
        holding_ref: &str,
    ) -> Result<(RoutedTenantDatabase, RoutedTenantDatabase), LegalProjectionWorkerError> {
        let write = self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Write)
            .await
            .map_err(|_| redacted_error("write database routing"))?;
        let read = match self
            .router
            .route(tenant_id, holding_ref, RoutedDatabaseSide::Read)
            .await
        {
            Ok(read) => read,
            Err(_) => {
                write.close().await;
                return Err(redacted_error("read database routing"));
            }
        };
        if !valid_route_fence(tenant_id, holding_ref, &write, &read) {
            write.close().await;
            read.close().await;
            return Err(redacted_error("database route binding"));
        }
        Ok((write, read))
    }
}

async fn close_routes(write: RoutedTenantDatabase, read: RoutedTenantDatabase) {
    write.close().await;
    read.close().await;
}

fn validate_reference(value: &str, field: &str) -> Result<(), LegalProjectionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(LegalProjectionWorkerError::Configuration(format!(
            "Legal projection {field} is invalid"
        )));
    }
    Ok(())
}

fn valid_route_fence(
    tenant_id: &str,
    holding_ref: &str,
    write: &RoutedTenantDatabase,
    read: &RoutedTenantDatabase,
) -> bool {
    write.side == RoutedDatabaseSide::Write
        && read.side == RoutedDatabaseSide::Read
        && write.tenant_id == tenant_id
        && read.tenant_id == tenant_id
        && write.holding_ref == holding_ref
        && read.holding_ref == holding_ref
        && !write.database_key.trim().is_empty()
        && !read.database_key.trim().is_empty()
        && write.database_key != read.database_key
}

fn validate_source_event_page(
    cursor: Option<&LegalSourceEventCursor>,
    page: &LegalSourceEventPage,
    page_size: u32,
) -> Result<(), LegalProjectionWorkerError> {
    if page.events.len() > usize::try_from(page_size).unwrap_or(usize::MAX) {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection source page exceeds its bound".into(),
        ));
    }
    if page.has_more && page.events.is_empty() {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection source page cannot advance without events".into(),
        ));
    }

    let mut seen_event_ids = HashSet::with_capacity(page.events.len());
    let mut previous = cursor.cloned();
    for event in &page.events {
        validate_source_event(event)?;
        if !seen_event_ids.insert(event.event_id) {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source page contains a duplicate event".into(),
            ));
        }
        let current = event_cursor(event);
        if previous
            .as_ref()
            .is_some_and(|previous| !cursor_is_after(&current, previous))
        {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source page cursor did not advance".into(),
            ));
        }
        if let Some(previous) = previous.as_ref() {
            let same_aggregate = previous.aggregate_type == current.aggregate_type
                && previous.aggregate_id == current.aggregate_id;
            let expected_version = if same_aggregate {
                previous.aggregate_version.saturating_add(1)
            } else {
                1
            };
            if current.aggregate_version != expected_version {
                return Err(LegalProjectionWorkerError::Configuration(
                    "Legal projection source aggregate versions are not contiguous".into(),
                ));
            }
        } else if current.aggregate_version != 1 {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source aggregate must start at version one".into(),
            ));
        }
        previous = Some(current);
    }

    match (page.has_more, page.next_cursor.as_ref(), page.events.last()) {
        (true, Some(next_cursor), Some(last_event)) if next_cursor == &event_cursor(last_event) => {
        }
        (true, _, _) => {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source page continuation is invalid".into(),
            ));
        }
        (false, None, _) => {}
        (false, Some(_), _) => {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source page has an unexpected continuation".into(),
            ));
        }
    }
    Ok(())
}

fn validate_source_event(event: &LegalSourceEvent) -> Result<(), LegalProjectionWorkerError> {
    if event.event_id.is_nil()
        || event.contract_version != erpx_domain::contracts::CONTRACT_VERSION
        || event.event_version != 1
        || event.aggregate_id.is_nil()
        || event.aggregate_version == 0
        || event.correlation_id.is_nil()
        || event.idempotency_key.trim().is_empty()
        || event.idempotency_key.len() > 512
        || event.idempotency_key.chars().any(char::is_control)
        || event.actor.principal_ref.trim().is_empty()
        || event.actor.principal_ref.len() > 255
        || event.actor.principal_ref.chars().any(char::is_control)
        || matches!(
            &event.tenant_scope,
            erpx_domain::contracts::TenantScope::Platform
        )
    {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection source event envelope is invalid".into(),
        ));
    }
    if event.causation_id.is_some_and(|value| value.is_nil()) {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection source causation ID is invalid".into(),
        ));
    }
    match (&event.aggregate_type, &event.event_type, &event.payload) {
        (
            LegalAggregateType::Contract,
            LegalEventType::ContractSaved,
            LegalOutboxPayload::ContractSaved(contract),
        ) if event.aggregate_id == contract.contract_id => {
            validate_scope(
                event,
                contract.holding_id,
                contract.company_id,
                contract.branch_id,
            )?;
            validate_contract_payload(event, contract)?;
        }
        (
            LegalAggregateType::Document,
            LegalEventType::DocumentSaved,
            LegalOutboxPayload::DocumentSaved(document),
        ) if event.aggregate_id == document.document_id => {
            validate_scope(
                event,
                document.holding_id,
                document.company_id,
                document.branch_id,
            )?;
            validate_document_payload(document)?;
        }
        (
            LegalAggregateType::AccessHistory,
            LegalEventType::AccessRecorded,
            LegalOutboxPayload::AccessRecorded(access),
        ) if event.aggregate_id == access.entry_id => {
            validate_scope(
                event,
                access.holding_id,
                access.company_id,
                access.branch_id,
            )?;
            validate_access_payload(event, access)?;
        }
        _ => {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection source type and payload are inconsistent".into(),
            ));
        }
    }
    Ok(())
}

fn validate_scope(
    event: &LegalSourceEvent,
    holding_id: uuid::Uuid,
    company_id: uuid::Uuid,
    branch_id: Option<uuid::Uuid>,
) -> Result<(), LegalProjectionWorkerError> {
    if holding_id.is_nil()
        || company_id.is_nil()
        || branch_id.is_some_and(|value| value.is_nil())
        || !event.tenant_scope.contains(&match branch_id {
            Some(branch_id) => erpx_domain::contracts::TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => erpx_domain::contracts::TenantScope::Company {
                holding_id,
                company_id,
            },
        })
    {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection source scope is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_contract_payload(
    event: &LegalSourceEvent,
    contract: &erpx_infrastructure::LegalContractEvent,
) -> Result<(), LegalProjectionWorkerError> {
    validate_bounded(
        &contract.reference,
        MAX_LEGAL_REFERENCE_BYTES,
        "contract reference",
    )?;
    validate_bounded(&contract.title, MAX_LEGAL_TITLE_BYTES, "contract title")?;
    validate_bounded(
        &contract.created_by,
        MAX_LEGAL_REFERENCE_BYTES,
        "contract creator",
    )?;
    validate_object_ref(&contract.object_ref, "contract object reference")?;
    validate_digest(&contract.content_sha256, "contract digest")?;
    if event.aggregate_version == 1 && contract.created_by != event.actor.principal_ref {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection contract creator binding is invalid".into(),
        ));
    }
    if contract.status == ContractStatus::Approved
        && contract.approved_by.as_deref() != Some(event.actor.principal_ref.as_str())
    {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection contract approval binding is invalid".into(),
        ));
    }
    if contract
        .approved_by
        .as_deref()
        .is_some_and(|approved_by| approved_by == contract.created_by)
    {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection contract approval separation is invalid".into(),
        ));
    }
    if contract.parties.len() > MAX_LEGAL_PARTIES {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection contract party bound is invalid".into(),
        ));
    }
    for party in &contract.parties {
        if party.party_id.is_nil() {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection contract party ID is invalid".into(),
            ));
        }
        validate_bounded(
            &party.role,
            MAX_LEGAL_REFERENCE_BYTES,
            "contract party role",
        )?;
    }
    Ok(())
}

fn validate_document_payload(
    document: &erpx_infrastructure::LegalDocumentEvent,
) -> Result<(), LegalProjectionWorkerError> {
    validate_bounded(&document.title, MAX_LEGAL_TITLE_BYTES, "document title")?;
    if document.versions.len() > MAX_DOCUMENT_VERSIONS
        || document.current_version
            != document
                .versions
                .last()
                .map_or(0, |version| version.version)
    {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection document versions are invalid".into(),
        ));
    }
    for (index, version) in document.versions.iter().enumerate() {
        if version.version != (index + 1) as u64 {
            return Err(LegalProjectionWorkerError::Configuration(
                "Legal projection document versions are not contiguous".into(),
            ));
        }
        validate_object_ref(&version.object_ref, "document object reference")?;
        validate_digest(&version.content_sha256, "document digest")?;
        validate_bounded(
            &version.created_by,
            MAX_LEGAL_REFERENCE_BYTES,
            "document version creator",
        )?;
    }
    Ok(())
}

fn validate_access_payload(
    event: &LegalSourceEvent,
    access: &erpx_infrastructure::LegalAccessHistoryEvent,
) -> Result<(), LegalProjectionWorkerError> {
    if access.entry_id.is_nil() || access.document_id.is_nil() || access.version == 0 {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection access identifiers are invalid".into(),
        ));
    }
    if access.actor_ref != event.actor.principal_ref {
        return Err(LegalProjectionWorkerError::Configuration(
            "Legal projection access actor binding is invalid".into(),
        ));
    }
    validate_bounded(&access.actor_ref, MAX_LEGAL_REFERENCE_BYTES, "access actor")?;
    validate_bounded(&access.action, MAX_LEGAL_REFERENCE_BYTES, "access action")?;
    validate_bounded(&access.purpose, MAX_LEGAL_PURPOSE_BYTES, "access purpose")?;
    Ok(())
}

fn validate_bounded(
    value: &str,
    max: usize,
    field: &'static str,
) -> Result<(), LegalProjectionWorkerError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(LegalProjectionWorkerError::Configuration(format!(
            "Legal projection {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_object_ref(value: &str, field: &'static str) -> Result<(), LegalProjectionWorkerError> {
    validate_bounded(value, MAX_LEGAL_REFERENCE_BYTES, field)?;
    if value.starts_with('/')
        || value.contains("..")
        || value.contains("://")
        || value.bytes().any(|character| b"?#%=".contains(&character))
        || !value
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || b"/._-".contains(&character))
    {
        return Err(LegalProjectionWorkerError::Configuration(format!(
            "Legal projection {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), LegalProjectionWorkerError> {
    if value.len() != 71
        || !value.starts_with("sha256:")
        || value[7..]
            .chars()
            .any(|character| !character.is_ascii_hexdigit())
    {
        return Err(LegalProjectionWorkerError::Configuration(format!(
            "Legal projection {field} is invalid"
        )));
    }
    Ok(())
}

fn event_cursor(event: &LegalSourceEvent) -> LegalSourceEventCursor {
    LegalSourceEventCursor {
        aggregate_type: event.aggregate_type.as_str().into(),
        aggregate_id: event.aggregate_id,
        aggregate_version: event.aggregate_version,
    }
}

fn cursor_is_after(candidate: &LegalSourceEventCursor, previous: &LegalSourceEventCursor) -> bool {
    candidate.aggregate_type > previous.aggregate_type
        || (candidate.aggregate_type == previous.aggregate_type
            && (candidate.aggregate_id > previous.aggregate_id
                || (candidate.aggregate_id == previous.aggregate_id
                    && candidate.aggregate_version > previous.aggregate_version)))
}

fn next_legal_event_count(events_seen: u64, page_len: usize) -> Result<u64, PersistenceError> {
    let page_len = u64::try_from(page_len)
        .map_err(|_| PersistenceError::InvalidData("Legal page event count is invalid".into()))?;
    let next = events_seen
        .checked_add(page_len)
        .ok_or_else(|| PersistenceError::InvalidData("Legal event count overflow".into()))?;
    if next > LEGAL_PROJECTION_MAX_EVENTS {
        return Err(PersistenceError::InvalidData(
            "Legal projection source event bound exceeded".into(),
        ));
    }
    Ok(next)
}

fn redacted_error(operation: &'static str) -> LegalProjectionWorkerError {
    tracing::error!(operation, "Legal projection worker seam failed");
    LegalProjectionWorkerError::Projection { operation }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, TenantScope};
    use erpx_infrastructure::{
        LegalAccessHistoryEvent, LegalAggregateType, LegalEventType, LegalOutboxPayload,
    };
    use uuid::Uuid;

    fn source_event(
        aggregate_id: Uuid,
        aggregate_version: u64,
        event_id: Uuid,
    ) -> LegalSourceEvent {
        let holding_id = Uuid::from_u128(1);
        let company_id = Uuid::from_u128(2);
        let now = Utc::now();
        LegalSourceEvent {
            event_id,
            contract_version: erpx_domain::contracts::CONTRACT_VERSION,
            event_version: 1,
            event_type: LegalEventType::AccessRecorded,
            aggregate_type: LegalAggregateType::AccessHistory,
            aggregate_id,
            aggregate_version,
            occurred_at: now,
            tenant_scope: TenantScope::Holding { holding_id },
            actor: ActorAttribution {
                principal_ref: "legal-worker".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::new_v4(),
            causation_id: None,
            idempotency_key: format!("legal-{event_id}"),
            payload: LegalOutboxPayload::AccessRecorded(LegalAccessHistoryEvent {
                entry_id: aggregate_id,
                document_id: Uuid::from_u128(3),
                version: 1,
                actor_ref: "legal-worker".into(),
                holding_id,
                company_id,
                branch_id: None,
                action: "read".into(),
                purpose: "projection-test".into(),
                observed_at: now,
            }),
        }
    }

    #[test]
    fn request_bounds_and_cursor_fail_closed() {
        assert!(LegalProjectionRequest::new("tenant", "holding", None, 0).is_err());
        assert!(LegalProjectionRequest::new("tenant/unsafe", "holding", None, 1).is_err());
        assert!(
            LegalProjectionRequest::new(
                "tenant",
                "holding",
                Some(LegalSourceEventCursor {
                    aggregate_type: "legal_contract".into(),
                    aggregate_id: Uuid::nil(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
        assert!(
            LegalProjectionRequest::new(
                "tenant",
                "holding",
                Some(LegalSourceEventCursor {
                    aggregate_type: "unknown".into(),
                    aggregate_id: Uuid::new_v4(),
                    aggregate_version: 1,
                }),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn worker_errors_are_redacted() {
        let error = redacted_error("projection page");
        assert_eq!(error.to_string(), "Legal projection projection page failed");
        assert!(!error.to_string().contains("postgres"));
    }

    #[test]
    fn projection_accepts_exactly_one_million_events() {
        assert_eq!(
            next_legal_event_count(LEGAL_PROJECTION_MAX_EVENTS - 1, 1)
                .expect("one million Legal events is supported"),
            LEGAL_PROJECTION_MAX_EVENTS
        );
        assert!(next_legal_event_count(LEGAL_PROJECTION_MAX_EVENTS, 1).is_err());
    }

    #[test]
    fn source_page_rejects_stalled_duplicate_and_noncontiguous_events() {
        let aggregate_id = Uuid::from_u128(10);
        let first = source_event(aggregate_id, 1, Uuid::from_u128(11));
        let cursor = event_cursor(&first);

        let stalled = LegalSourceEventPage {
            events: vec![first.clone()],
            next_cursor: Some(cursor.clone()),
            has_more: true,
        };
        assert!(validate_source_event_page(Some(&cursor), &stalled, 1).is_err());

        let duplicate = LegalSourceEventPage {
            events: vec![first.clone(), first.clone()],
            next_cursor: None,
            has_more: false,
        };
        assert!(validate_source_event_page(None, &duplicate, 2).is_err());

        let skipped_version = LegalSourceEventPage {
            events: vec![first, source_event(aggregate_id, 3, Uuid::from_u128(12))],
            next_cursor: None,
            has_more: false,
        };
        assert!(validate_source_event_page(None, &skipped_version, 2).is_err());
    }

    #[test]
    fn source_page_rejects_invalid_envelope_and_unexpected_final_cursor() {
        let event = source_event(Uuid::from_u128(20), 1, Uuid::from_u128(21));
        let cursor = event_cursor(&event);
        let unexpected_final_cursor = LegalSourceEventPage {
            events: vec![event.clone()],
            next_cursor: Some(cursor),
            has_more: false,
        };
        assert!(validate_source_event_page(None, &unexpected_final_cursor, 1).is_err());

        let mut invalid = event;
        invalid.correlation_id = Uuid::nil();
        let invalid_page = LegalSourceEventPage {
            events: vec![invalid],
            next_cursor: None,
            has_more: false,
        };
        assert!(validate_source_event_page(None, &invalid_page, 1).is_err());
    }

    #[test]
    fn source_page_rejects_access_actor_mismatch() {
        let mut event = source_event(Uuid::from_u128(40), 1, Uuid::from_u128(41));
        if let LegalOutboxPayload::AccessRecorded(access) = &mut event.payload {
            access.actor_ref = "different-actor".into();
        }
        let page = LegalSourceEventPage {
            events: vec![event],
            next_cursor: None,
            has_more: false,
        };
        assert!(validate_source_event_page(None, &page, 1).is_err());
    }

    #[test]
    fn source_page_rejects_scope_and_type_mismatch_before_database_routing() {
        let mut scope_mismatch = source_event(Uuid::from_u128(50), 1, Uuid::from_u128(51));
        scope_mismatch.tenant_scope = TenantScope::Company {
            holding_id: Uuid::from_u128(999),
            company_id: Uuid::from_u128(2),
        };
        assert!(
            validate_source_event_page(
                None,
                &LegalSourceEventPage {
                    events: vec![scope_mismatch],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err()
        );

        let mut type_mismatch = source_event(Uuid::from_u128(60), 1, Uuid::from_u128(61));
        type_mismatch.aggregate_type = LegalAggregateType::Contract;
        assert!(
            validate_source_event_page(
                None,
                &LegalSourceEventPage {
                    events: vec![type_mismatch],
                    next_cursor: None,
                    has_more: false,
                },
                1,
            )
            .is_err()
        );
    }
}
