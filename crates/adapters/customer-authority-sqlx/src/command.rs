use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_customer_authority::{
    CommandFingerprint, CommandIdentity, CommandUnitOfWork, CustomerAccount,
    CustomerAuthorityCommit, CustomerAuthorityView, CustomerCode, CustomerId, CustomerIdentity,
    CustomerStatus, MergeTargetVersion, PortError, StoredCommit, WriteRepository,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, customer_partition,
    projection::{
        CustomerAuthorityOutboxPayload, CustomerAuthoritySnapshot, scope_key, scope_parts,
        status_name,
    },
};

#[derive(Debug, Clone)]
pub struct CustomerAuthorityWriteRepository {
    write: WritePool,
}

impl CustomerAuthorityWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { write: pool }
    }
}

#[async_trait]
impl WriteRepository for CustomerAuthorityWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        customer_id: CustomerId,
    ) -> Result<Option<CustomerAccount>, PortError> {
        load_customer_relational(&self.write, authority_scope, customer_id).await
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityCommandAdapter {
    store: CommandStore,
}

impl CustomerAuthorityCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for CustomerAuthorityCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let customer_id = fingerprint_customer_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<CustomerAccount>(
                CONTEXT_NAME,
                &customer_partition(customer_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: CustomerAuthorityCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let customer_id = request.aggregate.id();
        let partition = customer_partition(customer_id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let audit_action = audit_action(&request.identity.fingerprint);
        let idempotency_key = request.identity.idempotency_key.clone();
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: CustomerAuthorityOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: CustomerAuthorityOutboxPayload::Snapshot(CustomerAuthoritySnapshot {
                customer: CustomerAuthorityView::from(&request.aggregate),
            }),
        });
        let directory_draft = customer_directory_draft(&request.aggregate)?;
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(
                command_id,
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                events.len(),
            ),
            event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: CustomerAuthorityOutboxPayload::Directory(directory_draft),
        });

        let guard = CustomerAuthorityAtomicGuard {
            aggregate: request.aggregate.clone(),
            merge_target: request.merge_target,
        };
        let aggregate_id = aggregate_id(customer_id)?;
        let authority_scope = request.authority_scope.clone();
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            active_scope: authority_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"customer-authority-audit-v1"),
                action: audit_action.into(),
                occurred_at,
                details: json!({
                    "customer_id": customer_id.as_uuid(),
                    "expected_version": request.expected_version,
                    "merge_target_customer_id": guard.merge_target.map(|target| target.customer_id.as_uuid()),
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn customer_directory_draft(aggregate: &CustomerAccount) -> Result<ItemDescriptorDraft, PortError> {
    let record = aggregate.record_ref().map_err(|_| PortError::Internal)?;
    let lifecycle_state = match aggregate.status() {
        CustomerStatus::Prospect => "prospect",
        CustomerStatus::Onboarding => "onboarding",
        CustomerStatus::Active => "active",
        CustomerStatus::Suspended => "suspended",
        CustomerStatus::Churned => "churned",
    };
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.primary_authority().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!(
            "{} {}",
            aggregate.code().as_str(),
            aggregate.identity().trading_name()
        ),
    };
    value.validate().map_err(|_| PortError::Internal)?;
    Ok(value)
}

struct CustomerAuthorityAtomicGuard {
    aggregate: CustomerAccount,
    merge_target: Option<MergeTargetVersion>,
}

#[async_trait]
impl AtomicCommandGuard for CustomerAuthorityAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != customer_partition(self.aggregate.id())
        {
            return Err(CqrsSqlxError::GuardRejected(
                "customer authority guard metadata mismatch",
            ));
        }
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_customer_authority_write.account_guards
             WHERE customer_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        value.map(from_i64).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
             FROM erpx_customer_authority_write.account_guards
             WHERE customer_id = $1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "customer authority account already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "customer authority account guard is missing",
                ));
            }
            _ => {}
        }

        let duplicate_code: Option<Uuid> = sqlx::query_scalar(
            "SELECT customer_id
             FROM erpx_customer_authority_write.account_guards
             WHERE customer_code = $1 AND customer_id <> $2
             ORDER BY customer_id LIMIT 1 FOR UPDATE",
        )
        .bind(self.aggregate.code().as_str())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate_code.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "customer authority code already exists",
            ));
        }

        match (self.aggregate.merged_into(), self.merge_target) {
            (Some(survivor), Some(target)) if survivor == target.customer_id => {
                validate_merge_target(transaction, target).await?;
            }
            (None, None) => {}
            _ => {
                return Err(CqrsSqlxError::GuardRejected(
                    "customer authority merge guard does not match aggregate",
                ));
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != customer_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "customer authority applied guard context mismatch",
            ));
        }
        let (holding_id, company_id, _) = scope_parts(self.aggregate.primary_authority())?;
        sqlx::query(
            "INSERT INTO erpx_customer_authority_write.account_guards
                (customer_id, customer_code, primary_scope_key, primary_holding_id,
                 primary_company_id, customer_status, merged_into_customer_id,
                 aggregate_version, trading_name, legal_name, registration_number,
                 tax_identifier, country, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
             ON CONFLICT (customer_id) DO UPDATE SET
                customer_code = EXCLUDED.customer_code,
                primary_scope_key = EXCLUDED.primary_scope_key,
                primary_holding_id = EXCLUDED.primary_holding_id,
                primary_company_id = EXCLUDED.primary_company_id,
                customer_status = EXCLUDED.customer_status,
                merged_into_customer_id = EXCLUDED.merged_into_customer_id,
                aggregate_version = EXCLUDED.aggregate_version,
                trading_name = EXCLUDED.trading_name,
                legal_name = EXCLUDED.legal_name,
                registration_number = EXCLUDED.registration_number,
                tax_identifier = EXCLUDED.tax_identifier,
                country = EXCLUDED.country,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(self.aggregate.code().as_str())
        .bind(scope_key(self.aggregate.primary_authority())?)
        .bind(holding_id)
        .bind(company_id.ok_or(CqrsSqlxError::GuardRejected(
            "customer primary authority is not a company",
        ))?)
        .bind(status_name(self.aggregate.status()))
        .bind(self.aggregate.merged_into().map(CustomerId::as_uuid))
        .bind(to_i64(context.aggregate_version)?)
        .bind(self.aggregate.identity().trading_name())
        .bind(self.aggregate.identity().legal_name())
        .bind(self.aggregate.identity().registration_number())
        .bind(self.aggregate.identity().tax_identifier())
        .bind(self.aggregate.identity().country())
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;

        sqlx::query(
            "DELETE FROM erpx_customer_authority_write.authority_link_guards
             WHERE customer_id = $1",
        )
        .bind(self.aggregate.id().as_uuid())
        .execute(&mut **transaction)
        .await?;
        for authority in self.aggregate.authority_links() {
            let (holding_id, company_id, branch_id) = scope_parts(authority)?;
            sqlx::query(
                "INSERT INTO erpx_customer_authority_write.authority_link_guards
                    (customer_id, scope_key, scope_kind, holding_id, company_id,
                     branch_id, is_primary, aggregate_version, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(scope_key(authority)?)
            .bind(scope_kind(authority)?)
            .bind(holding_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(authority == self.aggregate.primary_authority())
            .bind(to_i64(context.aggregate_version)?)
            .bind(self.aggregate.updated_at())
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

async fn validate_merge_target(
    transaction: &mut Transaction<'_, Postgres>,
    target: MergeTargetVersion,
) -> Result<(), CqrsSqlxError> {
    let row = sqlx::query(
        "SELECT aggregate_version, merged_into_customer_id, customer_status
         FROM erpx_customer_authority_write.account_guards
         WHERE customer_id = $1 FOR UPDATE",
    )
    .bind(target.customer_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Err(CqrsSqlxError::GuardRejected(
            "customer authority merge target is missing",
        ));
    };
    let version: i64 = row.try_get("aggregate_version")?;
    let merged_into: Option<Uuid> = row.try_get("merged_into_customer_id")?;
    let status: String = row.try_get("customer_status")?;
    if u64::try_from(version).ok() != Some(target.version)
        || merged_into.is_some()
        || status == "churned"
    {
        return Err(CqrsSqlxError::GuardRejected(
            "customer authority merge target changed",
        ));
    }
    Ok(())
}

fn validate_guard_context(
    aggregate: &CustomerAccount,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != customer_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "customer authority guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &CustomerAuthorityCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_customer_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request
            .authority_scope
            .contains(request.aggregate.primary_authority())
        || fingerprint_scope(&request.identity.fingerprint)
            .is_some_and(|scope| !request.authority_scope.contains(scope))
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn aggregate_id(customer_id: CustomerId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(customer_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_customer_id(fingerprint: &CommandFingerprint) -> CustomerId {
    match fingerprint {
        CommandFingerprint::Create { customer_id, .. }
        | CommandFingerprint::UpdateIdentity { customer_id, .. }
        | CommandFingerprint::ChangeStatus { customer_id, .. }
        | CommandFingerprint::LinkAuthority { customer_id, .. }
        | CommandFingerprint::UnlinkAuthority { customer_id, .. } => *customer_id,
        CommandFingerprint::Merge {
            retired_customer_id,
            ..
        } => *retired_customer_id,
    }
}

fn fingerprint_scope(fingerprint: &CommandFingerprint) -> Option<&AuthorityScope> {
    match fingerprint {
        CommandFingerprint::Create {
            primary_authority, ..
        } => Some(primary_authority),
        CommandFingerprint::LinkAuthority { scope, .. }
        | CommandFingerprint::UnlinkAuthority { scope, .. } => Some(scope),
        _ => None,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.customer-authority.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::with_capacity(event_type.len() + 16);
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("a SHA-256-derived UUID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "customer_authority.customer.create",
        CommandFingerprint::UpdateIdentity { .. } => "customer_authority.identity.update",
        CommandFingerprint::ChangeStatus { .. } => "customer_authority.status.change",
        CommandFingerprint::LinkAuthority { .. } => "customer_authority.authority.link",
        CommandFingerprint::UnlinkAuthority { .. } => "customer_authority.authority.unlink",
        CommandFingerprint::Merge { .. } => "customer_authority.customer.merge",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<CustomerAccount>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected("customer authority account already exists")
        | CqrsSqlxError::GuardRejected("customer authority code already exists")
        | CqrsSqlxError::GuardRejected("customer authority merge target changed") => {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        CqrsSqlxError::GuardRejected("customer authority merge target is missing") => {
            PortError::NotFound
        }
        CqrsSqlxError::Database(error)
            if matches!(
                error
                    .as_database_error()
                    .and_then(|error| error.constraint()),
                Some("account_guards_customer_code_key") | Some("customer_authority_one_primary")
            ) =>
        {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

fn scope_kind(scope: &AuthorityScope) -> Result<&'static str, CqrsSqlxError> {
    match scope {
        AuthorityScope::Holding { .. } => Ok("holding"),
        AuthorityScope::Company { .. } => Ok("company"),
        AuthorityScope::Branch { .. } => Ok("branch"),
        AuthorityScope::Platform | AuthorityScope::Subtree { .. } => Err(
            CqrsSqlxError::GuardRejected("customer authority link is not exact"),
        ),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn from_i64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

async fn load_customer_relational(
    write: &WritePool,
    authority: &AuthorityScope,
    customer_id: CustomerId,
) -> Result<Option<CustomerAccount>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut transaction = write.begin().await.map_err(map_store_error)?;
    let link_rows = sqlx::query(
        "SELECT scope_kind,holding_id,company_id,branch_id,is_primary
         FROM erpx_customer_authority_write.authority_link_guards
         WHERE customer_id=$1 ORDER BY scope_key",
    )
    .bind(customer_id.as_uuid())
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    if link_rows.is_empty() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM erpx_customer_authority_write.account_guards
                           WHERE customer_id=$1)",
        )
        .bind(customer_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        return if exists {
            Err(PortError::Internal)
        } else {
            Ok(None)
        };
    }
    let mut links = Vec::with_capacity(link_rows.len());
    let mut primary = None;
    for row in link_rows {
        let scope = exact_scope_from_row(&row)?;
        if row
            .try_get::<bool, _>("is_primary")
            .map_err(|_| PortError::Internal)?
            && primary.replace(scope.clone()).is_some()
        {
            return Err(PortError::Internal);
        }
        links.push(scope);
    }
    if !links
        .iter()
        .any(|scope| authority.contains(scope) || scope.contains(authority))
    {
        return Err(PortError::Denied);
    }
    let row = sqlx::query(
        "SELECT aggregate_version,customer_code,trading_name,legal_name,
                registration_number,tax_identifier,country,customer_status,
                merged_into_customer_id,created_at,updated_at
         FROM erpx_customer_authority_write.account_guards WHERE customer_id=$1",
    )
    .bind(customer_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?
    .ok_or(PortError::Internal)?;
    let status = match row
        .try_get::<String, _>("customer_status")
        .map_err(|_| PortError::Internal)?
        .as_str()
    {
        "prospect" => CustomerStatus::Prospect,
        "onboarding" => CustomerStatus::Onboarding,
        "active" => CustomerStatus::Active,
        "suspended" => CustomerStatus::Suspended,
        "churned" => CustomerStatus::Churned,
        _ => return Err(PortError::Internal),
    };
    let merged_into = row
        .try_get::<Option<Uuid>, _>("merged_into_customer_id")
        .map_err(|_| PortError::Internal)?
        .map(CustomerId::try_from_uuid)
        .transpose()
        .map_err(|_| PortError::Internal)?;
    CustomerAccount::try_rehydrate(
        customer_id,
        u64::try_from(
            row.try_get::<i64, _>("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        CustomerCode::try_new(
            row.try_get::<String, _>("customer_code")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        CustomerIdentity::try_new(
            row.try_get::<String, _>("trading_name")
                .map_err(|_| PortError::Internal)?,
            row.try_get("legal_name").map_err(|_| PortError::Internal)?,
            row.try_get("registration_number")
                .map_err(|_| PortError::Internal)?,
            row.try_get("tax_identifier")
                .map_err(|_| PortError::Internal)?,
            row.try_get::<String, _>("country")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        status,
        primary.ok_or(PortError::Internal)?,
        links,
        merged_into,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

fn exact_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let kind: String = row.try_get("scope_kind").map_err(|_| PortError::Internal)?;
    let holding_id: Uuid = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id: Option<Uuid> = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    match (kind.as_str(), company_id, branch_id) {
        ("holding", None, None) => Ok(AuthorityScope::Holding { holding_id }),
        ("company", Some(company_id), None) => Ok(AuthorityScope::Company {
            holding_id,
            company_id,
        }),
        ("branch", Some(company_id), Some(branch_id)) => Ok(AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        }),
        _ => Err(PortError::Internal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_customer_authority::{CustomerCode, CustomerIdentity, CustomerStatus};

    fn customer_id(value: u128) -> CustomerId {
        CustomerId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        let first = CommandFingerprint::ChangeStatus {
            customer_id: customer_id(1),
            status: CustomerStatus::Active,
        };
        let changed = CommandFingerprint::ChangeStatus {
            customer_id: customer_id(1),
            status: CustomerStatus::Suspended,
        };
        assert_eq!(request_hash(&first).unwrap(), request_hash(&first).unwrap());
        assert_ne!(
            request_hash(&first).unwrap(),
            request_hash(&changed).unwrap()
        );
    }

    #[test]
    fn fingerprint_hash_includes_sensitive_identity_without_exposing_it() {
        let fingerprint = CommandFingerprint::Create {
            customer_id: customer_id(1),
            code: CustomerCode::try_new("CUS-1").unwrap(),
            identity: CustomerIdentity::try_new(
                "Acme",
                None,
                Some("registration-a".into()),
                Some("tax-a".into()),
                "TH",
            )
            .unwrap(),
            primary_authority: AuthorityScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
        };
        assert_eq!(
            request_hash(&fingerprint).unwrap(),
            request_hash(&fingerprint).unwrap()
        );
    }

    #[test]
    fn exact_scope_parts_fail_closed_for_platform_and_subtree() {
        assert!(scope_kind(&AuthorityScope::Platform).is_err());
        assert!(
            scope_kind(&AuthorityScope::Subtree {
                holding_id: Uuid::from_u128(1),
                organization_node_id: Uuid::from_u128(2),
            })
            .is_err()
        );
    }

    #[test]
    fn account_command_authority_is_relational_and_snapshot_free() {
        let production = include_str!("command.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("load_snapshot"));
        assert!(production.contains("authority_link_guards"));
        assert!(production.contains("commit_normalized"));
    }
}
