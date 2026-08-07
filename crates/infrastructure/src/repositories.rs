use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_application::{
    AddCustomerNoteCommand, ApplicationError, AuditEvent, AuditSink, CommandUnitOfWork,
    Customer360DetailReadStore, Customer360NoteWritePort, CustomerGlobalReadAuditDetailCursor,
    CustomerGlobalReadAuditDetailPage, CustomerGlobalReadAuditDetailQuery,
    CustomerGlobalReadAuditDetailStore, CustomerNoteWriteReceipt, CustomerReadStore,
    RoleAssignmentStore, SupportStore,
};
use erpx_domain::{
    access::{RoleAssignment, Scope},
    customer::{
        CUSTOMER_DETAIL_MAX_PAGE_SIZE, Customer360, Customer360DetailReadContext,
        Customer360DetailReadModel, Customer360Projection, CustomerAccessAuditSummaryReadDto,
        CustomerCommercialReadDto, CustomerContactsReadDto, CustomerDetailReadError,
        CustomerEntitlementsReadDto, CustomerGlobalReadAuditDetailReadDto, CustomerHealthReadDto,
        CustomerNote, CustomerNoteCursor, CustomerNoteReadDto, CustomerNotesPageReadDto,
        CustomerNotesReadDto, CustomerOnboardingReadDto, CustomerOrganizationReadDto,
        CustomerReadView, CustomerScopeBindingProjection, CustomerStatus,
        CustomerSupportSummaryReadDto, GlobalReadActor, GlobalReadAuditRecord, GlobalReadScope,
    },
    support::{
        Incident, IncidentStatus, SupportCapability, SupportCapabilityBinding,
        SupportCapabilityRevocation, SupportCapabilityRevocationReason, SupportMode,
        SupportSession, SupportTicket,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    ControlPlanePools, ControlPlaneWriteTransaction, IdempotencyReservation, PersistenceError,
};

mod finance;
mod marketing;
mod platform_integration;
mod tenant_registry;

pub use finance::FinanceBankProviderAccountBinding;
pub use marketing::{
    MARKETING_PUBLIC_ABUSE_WINDOW_SECONDS, MAX_MARKETING_PROVIDER_ACCOUNTS_PER_HOLDING,
    MAX_MARKETING_PUBLIC_ABUSE_LIMIT, MarketingProviderAccountBinding,
    MarketingProviderChannelRouteBinding, MarketingPublicAbuseWindowRequest,
};
pub use platform_integration::{
    MAX_PLATFORM_INTEGRATION_OBSERVATIONS, PlatformIntegrationObservation,
};
pub use tenant_registry::{
    MAX_TENANT_REGISTRY_PAGE_SIZE, ProjectionAuthorizationRequest, SqlxTenantStoreRegistry,
    TenantStoreRegistration, TenantStoreRegistrationDetail, TenantStoreRegistrationPage,
};

const SUPPORT_CAPABILITY_REVOKE_BATCH_SIZE: i64 = 1_000;

#[derive(Debug, Clone)]
pub struct SqlxControlPlane {
    pub(crate) pools: ControlPlanePools,
}

#[derive(Debug, Clone, Copy)]
pub struct ObjectAccessBindingRequest<'a> {
    pub tenant_id: &'a str,
    pub storage_namespace: &'a str,
    pub object_key: &'a str,
    pub customer_id: Uuid,
    pub scope: &'a Scope,
    pub actor_ref: &'a str,
    pub for_write: bool,
}

pub const MAX_PLATFORM_AUDIT_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformAuditEntry {
    pub audit_id: Uuid,
    pub actor_ref: String,
    pub action: String,
    pub purpose: String,
    pub correlation_id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub customer_scoped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformAuditPage {
    pub entries: Vec<PlatformAuditEntry>,
    pub has_more: bool,
    pub next_before: Option<(DateTime<Utc>, Uuid)>,
}

/// Read-side reconciliation evidence for the complete Customer 360 surface.
/// Counts are intentionally explicit so a green result cannot hide a missing
/// projection, stale payload, scope-binding drift, or note drift.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Customer360ReadReconciliation {
    pub source_customer_count: u64,
    pub read_customer_count: u64,
    pub missing_customer_count: u64,
    pub stale_customer_count: u64,
    pub orphan_customer_count: u64,
    pub binding_mismatch_count: u64,
    pub orphan_binding_count: u64,
    pub invalid_customer_source_count: u64,
    pub source_note_count: u64,
    pub read_note_count: u64,
    pub missing_note_count: u64,
    pub stale_note_count: u64,
    pub orphan_note_count: u64,
    pub access_audit_mismatch_count: u64,
    pub unprojected_audit_count: u64,
}

impl Customer360ReadReconciliation {
    pub fn healthy(&self) -> bool {
        self.missing_customer_count == 0
            && self.stale_customer_count == 0
            && self.orphan_customer_count == 0
            && self.binding_mismatch_count == 0
            && self.orphan_binding_count == 0
            && self.invalid_customer_source_count == 0
            && self.missing_note_count == 0
            && self.stale_note_count == 0
            && self.orphan_note_count == 0
            && self.access_audit_mismatch_count == 0
            && self.unprojected_audit_count == 0
    }
}

impl SqlxControlPlane {
    pub fn new(pools: ControlPlanePools) -> Self {
        Self { pools }
    }

    pub fn pools(&self) -> &ControlPlanePools {
        &self.pools
    }

    /// Fail closed unless the immutable, persisted cutover authority exists.
    ///
    /// Server and worker composition roots call this after migrations and
    /// before accepting or claiming any mutation.
    pub async fn require_erpx_command_owner(&self) -> Result<(), PersistenceError> {
        sqlx::query("SELECT erpx_control.require_erpx_command_owner()")
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    pub async fn command_ownership_mode(&self) -> Result<String, PersistenceError> {
        let mode = sqlx::query_scalar(
            "SELECT mode
             FROM erpx_control.command_ownership
             WHERE singleton",
        )
        .fetch_optional(&self.pools.write)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "authoritative ERPX command ownership is unavailable".into(),
            )
        })?;
        Ok(mode)
    }

    /// Read only the allow-listed audit columns needed by Platform Operations.
    /// Payload JSON and customer identifiers never cross this repository
    /// boundary. Audit events are append-only control-plane authority and do
    /// not have a separate read projection, so this query intentionally uses
    /// the authoritative write pool with a stable descending `(recorded_at,
    /// id)` cursor.
    pub async fn list_platform_audit_page(
        &self,
        before: Option<(DateTime<Utc>, Uuid)>,
        page_size: u32,
    ) -> Result<PlatformAuditPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_PLATFORM_AUDIT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "platform audit page size is outside its bound".into(),
            ));
        }
        let fetch_limit = i64::from(page_size) + 1;
        let (before_at, before_id) = before
            .map(|(recorded_at, audit_id)| (Some(recorded_at), Some(audit_id)))
            .unwrap_or((None, None));
        let rows = sqlx::query(
            "SELECT id, actor_ref, action, purpose, correlation_id, recorded_at,
                    customer_account_id IS NOT NULL AS customer_scoped
             FROM erpx_control.audit_events
             WHERE ($1::timestamptz IS NULL
                    OR (recorded_at, id) < ($1::timestamptz, $2::uuid))
             ORDER BY recorded_at DESC, id DESC
             LIMIT $3",
        )
        .bind(before_at)
        .bind(before_id)
        .bind(fetch_limit)
        .fetch_all(&self.pools.write)
        .await?;
        let has_more = rows.len() > usize::try_from(page_size).unwrap_or(usize::MAX);
        let mut entries =
            Vec::with_capacity(rows.len().min(usize::try_from(page_size).unwrap_or(0)));
        for row in rows {
            entries.push(PlatformAuditEntry {
                audit_id: row.try_get("id")?,
                actor_ref: row.try_get("actor_ref")?,
                action: row.try_get("action")?,
                purpose: row.try_get("purpose")?,
                correlation_id: row.try_get("correlation_id")?,
                recorded_at: row.try_get("recorded_at")?,
                customer_scoped: row.try_get("customer_scoped")?,
            });
            if entries.len() == usize::try_from(page_size).unwrap_or(usize::MAX) {
                break;
            }
        }
        let next_before = has_more
            .then(|| {
                entries
                    .last()
                    .map(|entry| (entry.recorded_at, entry.audit_id))
            })
            .flatten();
        Ok(PlatformAuditPage {
            entries,
            has_more,
            next_before,
        })
    }

    /// Bind an object key to its authoritative customer and organization
    /// scope. Uploads may create a binding once; downloads must find the same
    /// active binding. A conflicting key is never rebound.
    pub async fn authorize_object_binding(
        &self,
        request: &ObjectAccessBindingRequest<'_>,
    ) -> Result<bool, PersistenceError> {
        let (holding_id, company_id, branch_id) = match request.scope {
            Scope::Holding { holding_id } => (*holding_id, None, None),
            Scope::Company {
                holding_id,
                company_id,
            } => (*holding_id, Some(*company_id), None),
            Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => (*holding_id, Some(*company_id), Some(*branch_id)),
            Scope::Platform | Scope::Subtree { .. } => return Ok(false),
        };
        if request.tenant_id.trim().is_empty()
            || request.storage_namespace.trim().is_empty()
            || request.object_key.trim().is_empty()
            || request.actor_ref.trim().is_empty()
            || request.customer_id.is_nil()
        {
            return Ok(false);
        }
        let mut transaction = self.pools.write.begin().await?;
        if request.for_write {
            sqlx::query(
                "INSERT INTO erpx_control.object_access_bindings
                    (id, tenant_id, storage_namespace, object_key, customer_id,
                     holding_id, company_id, branch_id, created_by, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active')
                 ON CONFLICT (storage_namespace, object_key) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(request.tenant_id)
            .bind(request.storage_namespace)
            .bind(request.object_key)
            .bind(request.customer_id)
            .bind(holding_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(request.actor_ref)
            .execute(&mut *transaction)
            .await?;
        }
        let row = sqlx::query(
            "SELECT tenant_id, storage_namespace, customer_id, holding_id,
                    company_id, branch_id, status
             FROM erpx_control.object_access_bindings
             WHERE storage_namespace = $1 AND object_key = $2",
        )
        .bind(request.storage_namespace)
        .bind(request.object_key)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let Some(row) = row else {
            return Ok(false);
        };
        if row.try_get::<String, _>("tenant_id")? != request.tenant_id
            || row.try_get::<String, _>("storage_namespace")? != request.storage_namespace
            || row.try_get::<Uuid, _>("customer_id")? != request.customer_id
            || row.try_get::<String, _>("status")? != "active"
        {
            return Ok(false);
        }
        let bound_scope = match (
            row.try_get::<Uuid, _>("holding_id")?,
            row.try_get::<Option<Uuid>, _>("company_id")?,
            row.try_get::<Option<Uuid>, _>("branch_id")?,
        ) {
            (holding_id, None, None) => Scope::Holding { holding_id },
            (holding_id, Some(company_id), None) => Scope::Company {
                holding_id,
                company_id,
            },
            (holding_id, Some(company_id), Some(branch_id)) => Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            _ => return Ok(false),
        };
        Ok(request.scope.contains(&bound_scope) || bound_scope.contains(request.scope))
    }

    async fn read_customer_projection_view(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<Customer360Projection>, ApplicationError> {
        let row = sqlx::query(
            "SELECT payload, updated_at
             FROM erpx_control.customers
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.read)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;

        let Some(row) = row else {
            return Ok(CustomerReadView::absent(id));
        };
        let payload: Value = row
            .try_get("payload")
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        let updated_at: DateTime<Utc> = row
            .try_get("updated_at")
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        let projection = customer_projection_from_payload(payload).map_err(app_error)?;
        if projection.account.id != id {
            return Err(app_error(PersistenceError::InvalidData(
                "Customer 360 projection ID does not match its read-model key".into(),
            )));
        }
        Ok(CustomerReadView::present(
            projection.canonicalized(),
            id,
            updated_at,
            Utc::now(),
        ))
    }

    /// Fail-closed customer/organization binding used before customer-scoped
    /// commercial or support commands. Bindings are write-side authority;
    /// read-model counts are never used as an authorization substitute.
    pub async fn customer_scope_contains(
        &self,
        customer_id: Uuid,
        scope: &Scope,
    ) -> Result<bool, PersistenceError> {
        if customer_id.is_nil() {
            return Ok(false);
        }
        match scope {
            Scope::Platform => Ok(true),
            Scope::Holding { holding_id } => Ok(sqlx::query_scalar(
                "SELECT EXISTS(
                        SELECT 1 FROM erpx_control.customer_scope_bindings
                        WHERE customer_id = $1 AND holding_id = $2
                          AND company_id IS NULL AND branch_id IS NULL
                    )",
            )
            .bind(customer_id)
            .bind(holding_id)
            .fetch_one(&self.pools.write)
            .await?),
            Scope::Company {
                holding_id,
                company_id,
            } => Ok(sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1 FROM erpx_control.customer_scope_bindings
                    WHERE customer_id = $1 AND holding_id = $2
                      AND branch_id IS NULL
                      AND (company_id = $3 OR company_id IS NULL)
                )",
            )
            .bind(customer_id)
            .bind(holding_id)
            .bind(company_id)
            .fetch_one(&self.pools.write)
            .await?),
            Scope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Ok(sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1 FROM erpx_control.customer_scope_bindings
                    WHERE customer_id = $1 AND holding_id = $2
                      AND (
                        (company_id = $3 AND branch_id = $4)
                        OR (company_id = $3 AND branch_id IS NULL)
                        OR (company_id IS NULL AND branch_id IS NULL)
                      )
                )",
            )
            .bind(customer_id)
            .bind(holding_id)
            .bind(company_id)
            .bind(branch_id)
            .fetch_one(&self.pools.write)
            .await?),
            Scope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => {
                if sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                        SELECT 1 FROM erpx_control.customer_scope_bindings
                        WHERE customer_id = $1 AND holding_id = $2
                          AND company_id IS NULL AND branch_id IS NULL
                    )",
                )
                .bind(customer_id)
                .bind(holding_id)
                .fetch_one(&self.pools.write)
                .await?
                {
                    return Ok(true);
                }
                for company_id in company_ids {
                    let company_bound: bool = sqlx::query_scalar(
                        "SELECT EXISTS(
                            SELECT 1 FROM erpx_control.customer_scope_bindings
                            WHERE customer_id = $1 AND holding_id = $2
                              AND (
                                (company_id = $3 AND branch_id IS NULL)
                                OR (company_id IS NULL AND branch_id IS NULL)
                                OR (
                                  cardinality($4::uuid[]) > 0
                                  AND company_id = $3
                                  AND branch_id = ANY($4::uuid[])
                                )
                              )
                        )",
                    )
                    .bind(customer_id)
                    .bind(holding_id)
                    .bind(company_id)
                    .bind(branch_ids)
                    .fetch_one(&self.pools.write)
                    .await?;
                    if !company_bound {
                        return Ok(false);
                    }
                }
                for branch_id in branch_ids {
                    let branch_bound: bool = sqlx::query_scalar(
                        "SELECT EXISTS(
                            SELECT 1 FROM erpx_control.customer_scope_bindings
                            WHERE customer_id = $1 AND holding_id = $2
                              AND branch_id = $3
                              AND (
                                cardinality($4::uuid[]) = 0
                                OR company_id = ANY($4::uuid[])
                              )
                        )",
                    )
                    .bind(customer_id)
                    .bind(holding_id)
                    .bind(branch_id)
                    .bind(company_ids)
                    .fetch_one(&self.pools.write)
                    .await?;
                    if !branch_bound {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    pub async fn upsert_customer_scope_binding(
        &self,
        customer_id: Uuid,
        holding_id: Uuid,
        company_id: Option<Uuid>,
        branch_id: Option<Uuid>,
    ) -> Result<(), PersistenceError> {
        if customer_id.is_nil()
            || holding_id.is_nil()
            || branch_id.is_some_and(|branch| branch.is_nil())
            || (branch_id.is_some() && company_id.is_none())
        {
            return Err(PersistenceError::InvalidData(
                "customer scope binding is invalid".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.customer_scope_bindings
                (customer_id, holding_id, company_id, branch_id)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(customer_id)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    /// Persist only derived organization bindings on the control write pool.
    /// Read-model counts and payloads are never consulted for authorization.
    pub async fn upsert_customer_scope_bindings(
        &self,
        bindings: &[CustomerScopeBindingProjection],
    ) -> Result<(), PersistenceError> {
        let mut transaction = self.pools.write.begin().await?;
        for binding in bindings {
            validate_customer_scope_binding(binding)?;
            sqlx::query(
                "INSERT INTO erpx_control.customer_scope_bindings
                    (customer_id, holding_id, company_id, branch_id)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT DO NOTHING",
            )
            .bind(binding.customer_id)
            .bind(binding.holding_id)
            .bind(binding.company_id)
            .bind(binding.branch_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Replace the complete derived binding set for one customer on the
    /// authoritative write database. An empty set is meaningful: it revokes
    /// every previously-derived holding/company/branch binding. The caller is
    /// the Customer 360 projector, whose source is already an ordered
    /// customer outbox event; no independent authorization event is invented
    /// here.
    pub async fn reconcile_customer_scope_bindings(
        &self,
        customer_id: Uuid,
        bindings: &[CustomerScopeBindingProjection],
    ) -> Result<(), PersistenceError> {
        if customer_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "customer scope binding customer ID cannot be nil".into(),
            ));
        }
        for binding in bindings {
            validate_customer_scope_binding(binding)?;
            if binding.customer_id != customer_id {
                return Err(PersistenceError::InvalidData(
                    "customer scope binding customer ID does not match reconciliation key".into(),
                ));
            }
        }

        let mut transaction = self.pools.write.begin().await?;
        sqlx::query(
            "DELETE FROM erpx_control.customer_scope_bindings
             WHERE customer_id = $1",
        )
        .bind(customer_id)
        .execute(&mut *transaction)
        .await?;
        for binding in bindings {
            sqlx::query(
                "INSERT INTO erpx_control.customer_scope_bindings
                    (customer_id, holding_id, company_id, branch_id)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT DO NOTHING",
            )
            .bind(binding.customer_id)
            .bind(binding.holding_id)
            .bind(binding.company_id)
            .bind(binding.branch_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn check_ready(&self) -> Result<(), PersistenceError> {
        sqlx::query("SELECT 1").execute(&self.pools.write).await?;
        sqlx::query("SELECT 1").execute(&self.pools.read).await?;
        Ok(())
    }

    pub async fn begin_write(&self) -> Result<ControlPlaneWriteTransaction<'_>, PersistenceError> {
        Ok(ControlPlaneWriteTransaction::new(
            self.pools.write.begin().await?,
        ))
    }

    /// Project a complete customer 360 document into the control read
    /// database. Projectors, rather than request handlers, should call this
    /// method in production.
    pub async fn upsert_customer_read_model(
        &self,
        customer: &Customer360,
    ) -> Result<(), PersistenceError> {
        self.upsert_customer_projection_read_model(&Customer360Projection::from_legacy(customer))
            .await
    }

    /// Persist the enriched Customer 360 document on the rebuildable control
    /// read database and reconcile its derived scope authority on the control
    /// write database. The read payload remains non-authoritative; the
    /// write-side binding transaction is what makes revocation effective.
    pub async fn upsert_customer_projection_read_model(
        &self,
        projection: &Customer360Projection,
    ) -> Result<(), PersistenceError> {
        self.upsert_customer_projection_read_model_at(projection, Utc::now())
            .await
    }

    /// Apply a Customer 360 event at its source occurrence time. The
    /// timestamp fence prevents a delayed/replayed older event from replacing
    /// a newer read payload when a projector is recovering after a lease loss.
    pub async fn upsert_customer_projection_read_model_at(
        &self,
        projection: &Customer360Projection,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let projection = projection.clone().canonicalized();
        if projection.account.id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "customer projection account ID cannot be nil".into(),
            ));
        }
        let bindings = projection.scope_bindings();
        let payload = serde_json::to_value(&projection)?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.customers
                (id, account_code, trading_name, legal_name, country, status, billing_state, payload, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET
                account_code = EXCLUDED.account_code,
                trading_name = EXCLUDED.trading_name,
                legal_name = EXCLUDED.legal_name,
                country = EXCLUDED.country,
                status = EXCLUDED.status,
                billing_state = EXCLUDED.billing_state,
                payload = EXCLUDED.payload,
                updated_at = EXCLUDED.updated_at
             WHERE erpx_control.customers.updated_at <= EXCLUDED.updated_at",
        )
        .bind(projection.account.id)
        .bind(&projection.account.account_code)
        .bind(&projection.account.identity.trading_name)
        .bind(projection.account.identity.legal_name.as_deref())
        .bind(&projection.account.country)
        .bind(customer_status(projection.account.account_state))
        .bind(billing_state(projection.account.subscription_state))
        .bind(payload)
        .bind(occurred_at)
        .execute(&self.pools.read)
        .await?;
        // A delayed event must not regress the authoritative write-side
        // scope bindings even though it is harmlessly ignored by the read
        // model timestamp fence.
        if result.rows_affected() == 0 {
            return Ok(());
        }
        self.reconcile_customer_scope_bindings(projection.account.id, &bindings)
            .await?;
        Ok(())
    }

    /// Project one append-only note from the authoritative control outbox. The
    /// read row stores only the domain-redacted body and is immutable after its
    /// first successful insert; a same-ID/different-payload replay fails
    /// closed instead of silently rewriting history.
    pub async fn upsert_customer_note_read_model(
        &self,
        note: &CustomerNote,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        note.validate().map_err(|error| {
            PersistenceError::InvalidData(format!("invalid customer note: {error}"))
        })?;
        let read_note = note.read_dto().redacted_for_detail_read();
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.customer_notes
                (id, customer_id, author_ref, created_at, body, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(read_note.id)
        .bind(note.customer_id)
        .bind(&read_note.author_ref)
        .bind(read_note.created_at)
        .bind(&read_note.body)
        .bind(occurred_at)
        .execute(&self.pools.read)
        .await?;
        if inserted.rows_affected() == 0 {
            let existing = sqlx::query(
                "SELECT customer_id, author_ref, created_at, body
                 FROM erpx_control.customer_notes WHERE id = $1",
            )
            .bind(read_note.id)
            .fetch_optional(&self.pools.read)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData("customer note replay row disappeared".into())
            })?;
            let same = existing.try_get::<Uuid, _>("customer_id")? == note.customer_id
                && existing.try_get::<String, _>("author_ref")? == read_note.author_ref
                && existing.try_get::<DateTime<Utc>, _>("created_at")? == read_note.created_at
                && existing.try_get::<String, _>("body")? == read_note.body;
            if !same {
                return Err(PersistenceError::InvalidData(
                    "customer note replay conflicts with immutable read identity".into(),
                ));
            }
        }
        Ok(())
    }

    /// Read one complete, transport-neutral Customer 360 detail document.
    /// Customer scope is checked against the authoritative control write pool
    /// before any detail payload is assembled from the read pool. The caller
    /// remains responsible for recording the supplied authenticated global
    /// read context through the application audit contract.
    pub async fn get_customer_360_detail(
        &self,
        context: &Customer360DetailReadContext,
        notes_before: Option<CustomerNoteCursor>,
        notes_limit: u32,
    ) -> Result<CustomerReadView<Customer360DetailReadModel>, ApplicationError> {
        context
            .validate()
            .map_err(customer_detail_application_error)?;
        if !matches!(context.scope, Scope::Platform)
            && !self
                .customer_scope_contains(context.customer_id, &context.scope)
                .await
                .map_err(app_error)?
        {
            return Err(ApplicationError::Authorization(
                "customer scope is not bound to the requested customer".into(),
            ));
        }

        let projection_view = self
            .read_customer_projection_view(context.customer_id)
            .await?;
        let (projection, metadata) = match projection_view {
            CustomerReadView::Present { data, metadata } => (data, metadata),
            CustomerReadView::Absent { metadata } => {
                return Ok(CustomerReadView::Absent { metadata });
            }
            CustomerReadView::Unsupported { metadata, reason } => {
                return Ok(CustomerReadView::Unsupported { metadata, reason });
            }
        };
        let access_audit = self
            .get_customer_access_audit_summary(context.customer_id)
            .await?;
        let notes = self
            .get_customer_notes_page(context.customer_id, notes_before, notes_limit)
            .await?;
        let detail = Customer360DetailReadModel::from_authorized_projection(
            &projection,
            &metadata,
            access_audit,
            notes,
            context,
        )
        .map_err(customer_detail_application_error)?;
        Ok(CustomerReadView::Present {
            data: detail,
            metadata,
        })
    }

    /// Read a deterministic notes page using `(created_at DESC, id DESC)` as
    /// the stable ordering. Note bodies are redacted before they cross the
    /// customer detail read boundary; no signed object URL or credential is
    /// materialized by this method.
    pub async fn get_customer_notes_page(
        &self,
        id: Uuid,
        before: Option<CustomerNoteCursor>,
        limit: u32,
    ) -> Result<CustomerReadView<CustomerNotesPageReadDto>, ApplicationError> {
        if limit == 0 {
            return Err(customer_detail_application_error(
                CustomerDetailReadError::InvalidPageSize,
            ));
        }
        let projection_view = self.read_customer_projection_view(id).await?;
        let projection_metadata = match projection_view {
            CustomerReadView::Present { metadata, .. } => metadata,
            CustomerReadView::Absent { metadata } => {
                return Ok(CustomerReadView::Absent { metadata });
            }
            CustomerReadView::Unsupported { metadata, reason } => {
                return Ok(CustomerReadView::Unsupported { metadata, reason });
            }
        };
        let limit = limit.min(CUSTOMER_DETAIL_MAX_PAGE_SIZE);
        let fetch_limit = i64::from(limit) + 1;
        let rows = sqlx::query(
            "SELECT id, created_at, author_ref, body, updated_at
             FROM erpx_control.customer_notes
             WHERE customer_id = $1
               AND ($2::timestamptz IS NULL
                    OR (created_at, id) < ($2::timestamptz, $3::uuid))
             ORDER BY created_at DESC, id DESC
             LIMIT $4",
        )
        .bind(id)
        .bind(before.as_ref().map(|cursor| cursor.created_at))
        .bind(before.as_ref().map(|cursor| cursor.note_id))
        .bind(fetch_limit)
        .fetch_all(&self.pools.read)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;

        let mut updated_at = projection_metadata
            .projection_updated_at
            .unwrap_or_else(Utc::now);
        let mut notes = Vec::with_capacity(rows.len());
        for row in rows {
            let row_updated_at: DateTime<Utc> = row
                .try_get("updated_at")
                .map_err(|error| app_error(PersistenceError::Database(error)))?;
            if row_updated_at > updated_at {
                updated_at = row_updated_at;
            }
            let note = erpx_domain::customer::CustomerNoteReadDto {
                id: row
                    .try_get("id")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                author_ref: row
                    .try_get("author_ref")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                body: row
                    .try_get("body")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
            };
            notes.push(note.redacted_for_detail_read());
        }
        let has_more = notes.len() > limit as usize;
        if has_more {
            notes.truncate(limit as usize);
        }
        let next_before = has_more
            .then(|| {
                notes.last().map(|note| CustomerNoteCursor {
                    created_at: note.created_at,
                    note_id: note.id,
                })
            })
            .flatten();
        Ok(CustomerReadView::present(
            CustomerNotesPageReadDto { notes, next_before },
            id,
            updated_at,
            Utc::now(),
        ))
    }

    /// Persist only the safe incident summary on the rebuildable control read
    /// database. Timeline, broadcasts, mitigation, root cause, and
    /// corrective actions stay on the write-side aggregate payload.
    pub async fn upsert_incident_projection_read_model(
        &self,
        incident: &Incident,
        version: u64,
    ) -> Result<(), PersistenceError> {
        if incident.id.is_nil() || version == 0 || incident.commander_ref.trim().is_empty() {
            return Err(PersistenceError::InvalidData(
                "incident projection identity is invalid".into(),
            ));
        }
        let leads = serde_json::to_value(&incident.leads)?;
        let affected_services = serde_json::to_value(&incident.affected_services)?;
        let affected_customer_accounts =
            serde_json::to_value(&incident.affected_customer_accounts)?;
        let payload = serde_json::json!({
            "id": incident.id,
            "status": incident_status(incident.status),
            "commander_ref": incident.commander_ref,
            "leads": incident.leads,
            "affected_services": incident.affected_services,
            "affected_customer_accounts": incident.affected_customer_accounts,
            "version": version,
        });
        sqlx::query(
            "INSERT INTO erpx_control.incidents
                (id, status, commander_ref, leads, affected_services,
                 affected_customer_accounts, version, payload, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
             ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status,
                commander_ref = EXCLUDED.commander_ref,
                leads = EXCLUDED.leads,
                affected_services = EXCLUDED.affected_services,
                affected_customer_accounts = EXCLUDED.affected_customer_accounts,
                version = EXCLUDED.version,
                payload = EXCLUDED.payload,
                updated_at = now()
             WHERE erpx_control.incidents.version < EXCLUDED.version",
        )
        .bind(incident.id)
        .bind(incident_status(incident.status))
        .bind(&incident.commander_ref)
        .bind(leads)
        .bind(affected_services)
        .bind(affected_customer_accounts)
        .bind(as_i64(version, "incident projection version")?)
        .bind(payload)
        .execute(&self.pools.read)
        .await?;
        Ok(())
    }

    pub async fn clear_incident_projection_read_model(&self) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM erpx_control.incidents")
            .execute(&self.pools.read)
            .await?;
        Ok(())
    }

    pub async fn clear_customer_read_model(&self) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM erpx_control.customers")
            .execute(&self.pools.read)
            .await?;
        Ok(())
    }

    /// Clear only the rebuildable Customer 360 access-audit summary model.
    /// Authoritative global-read audit rows remain on the control write side.
    pub async fn clear_customer_access_audit_summary_read_model(
        &self,
    ) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM erpx_control.customer_access_audit_summaries")
            .execute(&self.pools.read)
            .await?;
        Ok(())
    }

    /// Idempotently upsert one aggregate access-audit summary on the control
    /// read database. The summary deliberately contains no raw audit payload,
    /// actor, scope, purpose, or correlation fields.
    pub async fn upsert_customer_access_audit_summary_read_model(
        &self,
        customer_id: Uuid,
        read_count: u64,
        last_access_at: Option<DateTime<Utc>>,
        customer_visible: bool,
    ) -> Result<(), PersistenceError> {
        if customer_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "customer access-audit summary customer ID cannot be nil".into(),
            ));
        }
        let read_count = i64::try_from(read_count).map_err(|_| {
            PersistenceError::InvalidData(
                "customer access-audit summary read count exceeds database range".into(),
            )
        })?;
        sqlx::query(
            "INSERT INTO erpx_control.customer_access_audit_summaries
                (customer_id, read_count, last_access_at, customer_visible)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (customer_id) DO UPDATE SET
                read_count = EXCLUDED.read_count,
                last_access_at = EXCLUDED.last_access_at,
                customer_visible = EXCLUDED.customer_visible,
                updated_at = CASE
                    WHEN erpx_control.customer_access_audit_summaries.read_count
                           IS DISTINCT FROM EXCLUDED.read_count
                      OR erpx_control.customer_access_audit_summaries.last_access_at
                           IS DISTINCT FROM EXCLUDED.last_access_at
                      OR erpx_control.customer_access_audit_summaries.customer_visible
                           IS DISTINCT FROM EXCLUDED.customer_visible
                    THEN now()
                    ELSE erpx_control.customer_access_audit_summaries.updated_at
                END",
        )
        .bind(customer_id)
        .bind(read_count)
        .bind(last_access_at)
        .bind(customer_visible)
        .execute(&self.pools.read)
        .await?;
        Ok(())
    }

    /// Rebuild the access-audit summary from authoritative write-side global
    /// reads. The source query is ordered by customer ID, and only customers
    /// already present in the read projection are written because the
    /// rebuildable summary has a read-side foreign key to `customers`.
    pub async fn rebuild_customer_access_audit_summary_read_model(
        &self,
    ) -> Result<u64, PersistenceError> {
        let source_rows = sqlx::query(
            "SELECT customer_id,
                    COUNT(*)::BIGINT AS read_count,
                    MAX(occurred_at) AS last_access_at
             FROM erpx_control.global_read_audits
             GROUP BY customer_id
             ORDER BY customer_id",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let customer_ids: Vec<Uuid> = source_rows
            .iter()
            .map(|row| row.try_get("customer_id"))
            .collect::<Result<_, sqlx::Error>>()?;
        let available_customer_ids: HashSet<Uuid> = if customer_ids.is_empty() {
            HashSet::new()
        } else {
            sqlx::query_scalar(
                "SELECT id
                 FROM erpx_control.customers
                 WHERE id = ANY($1)",
            )
            .bind(&customer_ids)
            .fetch_all(&self.pools.read)
            .await?
            .into_iter()
            .collect()
        };

        self.clear_customer_access_audit_summary_read_model()
            .await?;
        let mut written = 0_u64;
        for row in source_rows {
            let customer_id: Uuid = row.try_get("customer_id")?;
            if !available_customer_ids.contains(&customer_id) {
                continue;
            }
            let read_count: i64 = row.try_get("read_count")?;
            let read_count = positive_u64(read_count, "customer access-audit read count")?;
            let last_access_at: Option<DateTime<Utc>> = row.try_get("last_access_at")?;
            self.upsert_customer_access_audit_summary_read_model(
                customer_id,
                read_count,
                last_access_at,
                true,
            )
            .await?;
            written = written.saturating_add(1);
        }
        Ok(written)
    }

    /// Compare every rebuildable Customer 360 surface with its authoritative
    /// control-write sources. This is read-only evidence: it never repairs a
    /// mismatch and therefore cannot turn a stale projection into authority.
    pub async fn reconcile_customer_360_read_models(
        &self,
    ) -> Result<Customer360ReadReconciliation, PersistenceError> {
        let source_rows = sqlx::query(
            "SELECT aggregate_id, state
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'customer'
             ORDER BY aggregate_id",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let read_rows = sqlx::query("SELECT id, payload FROM erpx_control.customers")
            .fetch_all(&self.pools.read)
            .await?;
        let read_customers: HashMap<Uuid, Value> = read_rows
            .into_iter()
            .map(|row| Ok((row.try_get("id")?, row.try_get("payload")?)))
            .collect::<Result<_, sqlx::Error>>()?;
        let mut report = Customer360ReadReconciliation {
            source_customer_count: source_rows.len() as u64,
            read_customer_count: read_customers.len() as u64,
            ..Default::default()
        };
        let mut source_customer_ids = HashSet::with_capacity(source_rows.len());
        for row in source_rows {
            let customer_id: Uuid = row.try_get("aggregate_id")?;
            source_customer_ids.insert(customer_id);
            let source_state: Value = row.try_get("state")?;
            let Some(read_payload) = read_customers.get(&customer_id) else {
                report.missing_customer_count += 1;
                continue;
            };
            let source_projection = match customer_projection_from_payload(source_state) {
                Ok(projection) => projection.canonicalized(),
                Err(_) => {
                    report.invalid_customer_source_count += 1;
                    continue;
                }
            };
            let read_projection = match customer_projection_from_payload(read_payload.clone()) {
                Ok(projection) => projection.canonicalized(),
                Err(_) => {
                    report.stale_customer_count += 1;
                    continue;
                }
            };
            if source_projection.account.id != customer_id
                || read_projection.account.id != customer_id
                || source_projection != read_projection
            {
                report.stale_customer_count += 1;
            }
            let expected_bindings = source_projection.scope_bindings();
            let actual_bindings = self
                .customer_scope_bindings_for_reconciliation(customer_id)
                .await?;
            if expected_bindings != actual_bindings {
                report.binding_mismatch_count += 1;
            }
        }
        report.orphan_customer_count = read_customers
            .keys()
            .filter(|id| !source_customer_ids.contains(id))
            .count() as u64;

        let binding_customer_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT customer_id FROM erpx_control.customer_scope_bindings")
                .fetch_all(&self.pools.write)
                .await?;
        report.orphan_binding_count = binding_customer_ids
            .iter()
            .filter(|customer_id| !source_customer_ids.contains(customer_id))
            .count() as u64;

        let source_note_rows = sqlx::query(
            "SELECT aggregate_id, state
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'customer_note'
             ORDER BY aggregate_id",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let read_note_rows = sqlx::query(
            "SELECT id, customer_id, author_ref, created_at, body
             FROM erpx_control.customer_notes",
        )
        .fetch_all(&self.pools.read)
        .await?;
        let read_notes: HashMap<Uuid, (Uuid, CustomerNoteReadDto)> = read_note_rows
            .into_iter()
            .map(|row| {
                let customer_id: Uuid = row.try_get("customer_id")?;
                let note = CustomerNoteReadDto {
                    id: row.try_get("id")?,
                    author_ref: row.try_get("author_ref")?,
                    created_at: row.try_get("created_at")?,
                    body: row.try_get("body")?,
                };
                Ok((note.id, (customer_id, note)))
            })
            .collect::<Result<_, sqlx::Error>>()?;
        report.source_note_count = source_note_rows.len() as u64;
        report.read_note_count = read_notes.len() as u64;
        let mut source_note_ids = HashSet::with_capacity(source_note_rows.len());
        for row in source_note_rows {
            let note_id: Uuid = row.try_get("aggregate_id")?;
            source_note_ids.insert(note_id);
            let source_state: Value = row.try_get("state")?;
            let source_note = match serde_json::from_value::<CustomerNote>(source_state) {
                Ok(note) if note.validate().is_ok() => note,
                Err(_) => {
                    report.stale_note_count += 1;
                    continue;
                }
                Ok(_) => {
                    report.stale_note_count += 1;
                    continue;
                }
            };
            let Some((read_customer_id, read_note)) = read_notes.get(&note_id) else {
                report.missing_note_count += 1;
                continue;
            };
            if *read_customer_id != source_note.customer_id
                || source_note.id != note_id
                || source_note.read_dto().redacted_for_detail_read() != *read_note
            {
                report.stale_note_count += 1;
            }
        }
        report.orphan_note_count = read_notes
            .keys()
            .filter(|id| !source_note_ids.contains(id))
            .count() as u64;

        let audit_source_rows = sqlx::query(
            "SELECT customer_id, COUNT(*)::BIGINT AS read_count,
                    MAX(occurred_at) AS last_access_at
             FROM erpx_control.global_read_audits
             GROUP BY customer_id",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let audit_source: HashMap<Uuid, (u64, Option<DateTime<Utc>>)> = audit_source_rows
            .into_iter()
            .map(|row| {
                let customer_id: Uuid = row.try_get("customer_id")?;
                let read_count = positive_u64(row.try_get("read_count")?, "audit read count")?;
                let last_access_at = row.try_get("last_access_at")?;
                Ok((customer_id, (read_count, last_access_at)))
            })
            .collect::<Result<_, PersistenceError>>()?;
        let audit_read_rows = sqlx::query(
            "SELECT customer_id, read_count, last_access_at, customer_visible
             FROM erpx_control.customer_access_audit_summaries",
        )
        .fetch_all(&self.pools.read)
        .await?;
        for (customer_id, (read_count, last_access_at)) in &audit_source {
            if !read_customers.contains_key(customer_id) {
                report.unprojected_audit_count += 1;
            } else if !audit_read_rows.iter().any(|row| {
                row.try_get::<Uuid, _>("customer_id")
                    .is_ok_and(|id| id == *customer_id)
            }) {
                report.access_audit_mismatch_count += 1;
            } else if let Some(row) = audit_read_rows.iter().find(|row| {
                row.try_get::<Uuid, _>("customer_id")
                    .is_ok_and(|id| id == *customer_id)
            }) {
                let actual_count: i64 = row.try_get("read_count")?;
                let actual_last: Option<DateTime<Utc>> = row.try_get("last_access_at")?;
                let customer_visible: bool = row.try_get("customer_visible")?;
                if positive_u64(actual_count, "audit summary read count")? != *read_count
                    || actual_last != *last_access_at
                    || !customer_visible
                {
                    report.access_audit_mismatch_count += 1;
                }
            }
        }
        for row in &audit_read_rows {
            let customer_id: Uuid = row.try_get("customer_id")?;
            let actual_count =
                positive_u64(row.try_get("read_count")?, "audit summary read count")?;
            let actual_last: Option<DateTime<Utc>> = row.try_get("last_access_at")?;
            let customer_visible: bool = row.try_get("customer_visible")?;
            let is_empty_materialization =
                actual_count == 0 && actual_last.is_none() && customer_visible;
            if !audit_source.contains_key(&customer_id)
                && (!is_empty_materialization || !read_customers.contains_key(&customer_id))
            {
                report.access_audit_mismatch_count += 1;
            }
        }
        Ok(report)
    }

    async fn customer_scope_bindings_for_reconciliation(
        &self,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerScopeBindingProjection>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT holding_id, company_id, branch_id
             FROM erpx_control.customer_scope_bindings
             WHERE customer_id = $1
             ORDER BY holding_id, company_id NULLS FIRST, branch_id NULLS FIRST",
        )
        .bind(customer_id)
        .fetch_all(&self.pools.write)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(CustomerScopeBindingProjection {
                    customer_id,
                    holding_id: row.try_get("holding_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                })
            })
            .collect()
    }

    async fn customer_exists_on_write(&self, customer_id: Uuid) -> Result<bool, PersistenceError> {
        if customer_id.is_nil() {
            return Ok(false);
        }
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM erpx_control.aggregate_snapshots
                WHERE aggregate_type = 'customer' AND aggregate_id = $1
            )",
        )
        .bind(customer_id)
        .fetch_one(&self.pools.write)
        .await?)
    }

    pub async fn record_global_read(
        &self,
        record: &GlobalReadAuditRecord,
    ) -> Result<(), PersistenceError> {
        record
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(record)?;
        let scope = serde_json::to_value(&record.scope)?;
        sqlx::query(
            "INSERT INTO erpx_control.global_read_audits
                (id, actor_ref, session_ref, customer_id, module, scope, purpose,
                 occurred_at, correlation_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(Uuid::new_v4())
        .bind(&record.actor.principal_ref)
        .bind(record.actor.session_ref.as_deref())
        .bind(record.customer_id)
        .bind(&record.module)
        .bind(scope)
        .bind(&record.purpose)
        .bind(record.occurred_at)
        .bind(record.correlation_id)
        .bind(payload)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    pub async fn list_global_reads(
        &self,
        customer_id: Uuid,
        limit: u32,
    ) -> Result<Vec<GlobalReadAuditRecord>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT payload FROM erpx_control.global_read_audits
             WHERE customer_id = $1 ORDER BY occurred_at DESC LIMIT $2",
        )
        .bind(customer_id)
        .bind(i64::from(limit.max(1)))
        .fetch_all(&self.pools.write)
        .await?;
        rows.iter().map(json).collect()
    }

    pub async fn save_ticket_if_version(
        &self,
        ticket: &SupportTicket,
        expected_version: u64,
    ) -> Result<u64, PersistenceError> {
        let payload = serde_json::to_value(ticket)?;
        let next_version = expected_version
            .checked_add(1)
            .ok_or_else(|| PersistenceError::InvalidData("ticket version overflow".into()))?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_tickets
             SET status = $2, payload = $3, version = $4, updated_at = now()
             WHERE id = $1 AND version = $5",
        )
        .bind(ticket.id)
        .bind(ticket_status(ticket.status))
        .bind(payload)
        .bind(i64::try_from(next_version).map_err(|_| {
            PersistenceError::InvalidData("ticket version exceeds database range".into())
        })?)
        .bind(i64::try_from(expected_version).map_err(|_| {
            PersistenceError::InvalidData("ticket version exceeds database range".into())
        })?)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(next_version);
        }
        let actual = sqlx::query("SELECT version FROM erpx_control.support_tickets WHERE id = $1")
            .bind(ticket.id)
            .fetch_optional(&self.pools.write)
            .await?
            .map(|row| row.try_get::<i64, _>("version"))
            .transpose()?;
        Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "support_ticket".into(),
            aggregate_id: ticket.id,
            expected: Some(expected_version),
            actual: actual
                .map(|version| positive_u64(version, "ticket version"))
                .transpose()?,
        })
    }

    pub async fn save_session_if_version(
        &self,
        session: &SupportSession,
        expected_version: u64,
    ) -> Result<u64, PersistenceError> {
        let payload = serde_json::to_value(session)?;
        let next_version = expected_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("support session version overflow".into())
        })?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_sessions
             SET status = $2, expires_at = $3, payload = $4, version = $5, updated_at = now()
             WHERE id = $1 AND version = $6",
        )
        .bind(session.id)
        .bind(support_session_status(session))
        .bind(session.expires_at)
        .bind(payload)
        .bind(i64::try_from(next_version).map_err(|_| {
            PersistenceError::InvalidData("support session version exceeds database range".into())
        })?)
        .bind(i64::try_from(expected_version).map_err(|_| {
            PersistenceError::InvalidData("support session version exceeds database range".into())
        })?)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(next_version);
        }
        let actual = sqlx::query("SELECT version FROM erpx_control.support_sessions WHERE id = $1")
            .bind(session.id)
            .fetch_optional(&self.pools.write)
            .await?
            .map(|row| row.try_get::<i64, _>("version"))
            .transpose()?;
        Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "support_session".into(),
            aggregate_id: session.id,
            expected: Some(expected_version),
            actual: actual
                .map(|version| positive_u64(version, "support session version"))
                .transpose()?,
        })
    }

    async fn current_version(
        &self,
        table: &'static str,
        id: Uuid,
    ) -> Result<u64, PersistenceError> {
        let query = match table {
            "support_tickets" => "SELECT version FROM erpx_control.support_tickets WHERE id = $1",
            "support_sessions" => "SELECT version FROM erpx_control.support_sessions WHERE id = $1",
            _ => {
                return Err(PersistenceError::InvalidData(
                    "unknown aggregate table".into(),
                ));
            }
        };
        let row = sqlx::query(query)
            .bind(id)
            .fetch_optional(&self.pools.write)
            .await?
            .ok_or(PersistenceError::NotFound { entity: table, id })?;
        positive_u64(row.try_get::<i64, _>("version")?, "aggregate version")
    }

    pub async fn reserve_idempotency(
        &self,
        key: &str,
        request_hash: &str,
        command_id: Option<Uuid>,
    ) -> Result<IdempotencyReservation, PersistenceError> {
        let mut transaction = self.begin_write().await?;
        let reservation = transaction
            .reserve_idempotency(key, request_hash, command_id)
            .await?;
        transaction.commit().await?;
        Ok(reservation)
    }

    pub async fn complete_idempotency(
        &self,
        key: &str,
        response: Value,
    ) -> Result<(), PersistenceError> {
        let mut transaction = self.begin_write().await?;
        transaction.complete_idempotency(key, response).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn current_ticket_version(&self, id: Uuid) -> Result<u64, PersistenceError> {
        self.current_version("support_tickets", id).await
    }

    pub async fn current_session_version(&self, id: Uuid) -> Result<u64, PersistenceError> {
        self.current_version("support_sessions", id).await
    }

    /// Evaluate the write-side role chain for one permission and requested
    /// scope. The parent chain is loaded from authoritative aggregate rows
    /// and snapshots, so a missing or inactive parent fails closed.
    pub async fn principal_has_role(
        &self,
        principal_ref: &str,
        requested_scope: &Scope,
        required_permission: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        let rows = sqlx::query(
            "SELECT payload FROM erpx_control.role_assignments
             UNION ALL
             SELECT state AS payload
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'role_assignment'",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let assignments = rows
            .iter()
            .filter_map(|row| json::<RoleAssignment>(row).ok())
            .filter(|assignment| assignment.principal_ref == principal_ref)
            .collect::<Vec<_>>();
        let mut all = rows
            .iter()
            .filter_map(|row| json::<RoleAssignment>(row).ok())
            .map(|assignment| (assignment.id, assignment))
            .collect::<HashMap<_, _>>();
        for assignment in assignments {
            all.insert(assignment.id, assignment);
        }
        Ok(all.values().any(|assignment| {
            assignment.principal_ref == principal_ref
                && assignment
                    .permissions
                    .iter()
                    .any(|permission| permission == required_permission)
                && assignment.scope.contains(requested_scope)
                && effective_role_chain(assignment, &all, at, &mut Vec::new())
        }))
    }

    /// Check a named customer/platform role on the write side. Role names are
    /// policy data, but this helper keeps customer approval bound to the two
    /// roles allowed by the support contract instead of a client assertion.
    pub async fn principal_has_role_key(
        &self,
        principal_ref: &str,
        requested_scope: &Scope,
        role_keys: &[&str],
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        let rows = sqlx::query(
            "SELECT payload FROM erpx_control.role_assignments
             UNION ALL
             SELECT state AS payload
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'role_assignment'",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let all = rows
            .iter()
            .filter_map(|row| json::<RoleAssignment>(row).ok())
            .map(|assignment| (assignment.id, assignment))
            .collect::<HashMap<_, _>>();
        Ok(all.values().any(|assignment| {
            assignment.principal_ref == principal_ref
                && role_keys.iter().any(|role| *role == assignment.role_key)
                && assignment.scope.contains(requested_scope)
                && effective_role_chain(assignment, &all, at, &mut Vec::new())
        }))
    }

    /// Return the authoritative assignment and its immediate parent for
    /// callers that only need one edge of the role chain. The universal
    /// evaluator uses `effective_role_assignment_chain` below.
    pub async fn effective_role_assignment(
        &self,
        principal_ref: &str,
        requested_scope: &Scope,
        required_permission: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<(RoleAssignment, Option<RoleAssignment>)>, PersistenceError> {
        Ok(self
            .effective_role_assignment_chain(
                principal_ref,
                requested_scope,
                required_permission,
                at,
            )
            .await?
            .map(|(assignment, parents)| (assignment, parents.into_iter().next())))
    }

    /// Return the authoritative assignment and every validated parent in
    /// immediate-to-root order. The chain is built from write-side aggregate
    /// rows/snapshots after recursive scope, permission, authority, expiry,
    /// and cycle checks, so callers cannot accidentally evaluate only one
    /// parent edge.
    pub async fn effective_role_assignment_chain(
        &self,
        principal_ref: &str,
        requested_scope: &Scope,
        required_permission: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<(RoleAssignment, Vec<RoleAssignment>)>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT payload FROM erpx_control.role_assignments
             UNION ALL
             SELECT state AS payload
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'role_assignment'",
        )
        .fetch_all(&self.pools.write)
        .await?;
        let all = rows
            .iter()
            .filter_map(|row| json::<RoleAssignment>(row).ok())
            .map(|assignment| (assignment.id, assignment))
            .collect::<HashMap<_, _>>();
        let candidate = all.values().find(|assignment| {
            assignment.principal_ref == principal_ref
                && assignment
                    .permissions
                    .iter()
                    .any(|permission| permission == required_permission)
                && assignment.scope.contains(requested_scope)
                && effective_role_chain(assignment, &all, at, &mut Vec::new())
        });
        Ok(candidate.and_then(|assignment| {
            let mut parents = Vec::new();
            let mut next_parent_id = assignment.parent_assignment_id;
            let mut seen = HashSet::new();
            while let Some(parent_id) = next_parent_id {
                if !seen.insert(parent_id) {
                    return None;
                }
                let parent = all.get(&parent_id)?.clone();
                next_parent_id = parent.parent_assignment_id;
                parents.push(parent);
            }
            Some((assignment.clone(), parents))
        }))
    }

    pub async fn record_sod_decision(
        &self,
        decision: &SodDecision,
    ) -> Result<(), PersistenceError> {
        if decision.principal_ref.trim().is_empty()
            || decision.module_key.trim().is_empty()
            || decision.reviewer_ref.trim().is_empty()
            || decision.reason.trim().is_empty()
            || decision.valid_until <= decision.valid_from
        {
            return Err(PersistenceError::InvalidData(
                "invalid segregation-of-duties decision".into(),
            ));
        }
        let scope = serde_json::to_value(&decision.scope)?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.sod_decisions
                (principal_ref, module_key, scope, passed, reviewer_ref, reason,
                 valid_from, valid_until, decision_version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (principal_ref, module_key, scope) DO UPDATE SET
                passed = EXCLUDED.passed, reviewer_ref = EXCLUDED.reviewer_ref,
                reason = EXCLUDED.reason, valid_from = EXCLUDED.valid_from,
                valid_until = EXCLUDED.valid_until,
                decision_version = EXCLUDED.decision_version, created_at = now()
             WHERE erpx_control.sod_decisions.decision_version < EXCLUDED.decision_version",
        )
        .bind(&decision.principal_ref)
        .bind(&decision.module_key)
        .bind(scope)
        .bind(decision.passed)
        .bind(&decision.reviewer_ref)
        .bind(&decision.reason)
        .bind(decision.valid_from)
        .bind(decision.valid_until)
        .bind(i64::try_from(decision.decision_version).map_err(|_| {
            PersistenceError::InvalidData("SoD decision version exceeds database range".into())
        })?)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "sod_decision".into(),
                aggregate_id: Uuid::nil(),
                expected: Some(decision.decision_version),
                actual: None,
            });
        }
        Ok(())
    }

    pub async fn sod_decision_active(
        &self,
        principal_ref: &str,
        module_key: &str,
        scope: &Scope,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        let scope = serde_json::to_value(scope)?;
        let passed = sqlx::query_scalar::<_, bool>(
            "SELECT passed FROM erpx_control.sod_decisions
             WHERE principal_ref = $1 AND module_key = $2 AND scope = $3
               AND valid_from <= $4 AND $4 < valid_until
             ORDER BY decision_version DESC LIMIT 1",
        )
        .bind(principal_ref)
        .bind(module_key)
        .bind(scope)
        .bind(at)
        .fetch_optional(&self.pools.write)
        .await?;
        Ok(passed.unwrap_or(false))
    }

    /// Validate the immutable support-capability intersection at action time.
    /// The bearer is compared to the stored hash only; it is never persisted
    /// or returned. Agent role, billing, and module entitlement checks remain
    /// separate universal policy conditions at the caller boundary.
    pub async fn authorize_support_capability(
        &self,
        request: &SupportCapabilityAuthorizationRequest,
    ) -> Result<bool, PersistenceError> {
        let Ok(candidate_hash) = hex::decode(&request.secret_hex) else {
            return Ok(false);
        };
        if candidate_hash.len() != 32 {
            return Ok(false);
        }
        let Some(row) = sqlx::query(
            "SELECT c.secret_hash, c.customer_account_id, c.agent_ref, c.device_ref,
                    c.access_request_id AS session_id, c.ticket_id, c.mode, c.permissions, c.scope, c.module,
                    c.issued_at, c.expires_at, c.status, c.revoked_at,
                    r.status AS request_status, t.status AS ticket_status
             FROM erpx_control.support_capabilities c
             JOIN erpx_control.support_access_requests r ON r.id = c.access_request_id
             JOIN erpx_control.support_tickets t ON t.id = c.ticket_id
             WHERE c.id = $1",
        )
        .bind(request.capability_id)
        .fetch_optional(&self.pools.write)
        .await?
        else {
            return Ok(false);
        };
        let stored_hash: Vec<u8> = row.try_get("secret_hash")?;
        let candidate_digest = Sha256::digest(&candidate_hash);
        let hash_matches = stored_hash.as_slice().ct_eq(candidate_digest.as_slice());
        let permissions: Vec<String> = serde_json::from_value(row.try_get("permissions")?)?;
        Ok(bool::from(hash_matches)
            && row.try_get::<Uuid, _>("customer_account_id")? == request.customer_account_id
            && row.try_get::<String, _>("agent_ref")? == request.agent_ref
            && row.try_get::<String, _>("device_ref")? == request.device_ref
            && row.try_get::<Uuid, _>("session_id")? == request.session_id
            && row.try_get::<Uuid, _>("ticket_id")? == request.ticket_id
            && row.try_get::<String, _>("mode")? == request.mode
            && row.try_get::<String, _>("scope")? == request.scope
            && row.try_get::<String, _>("module")? == request.module
            && permissions
                .iter()
                .any(|permission| permission == &request.permission)
            && row.try_get::<DateTime<Utc>, _>("issued_at")? <= request.at
            && request.at < row.try_get::<DateTime<Utc>, _>("expires_at")?
            && row.try_get::<String, _>("status")? == "active"
            && row
                .try_get::<Option<DateTime<Utc>>, _>("revoked_at")?
                .is_none()
            && row.try_get::<String, _>("request_status")? == "approved"
            && row.try_get::<String, _>("ticket_status")? != "closed")
    }

    /// Mirror the approved-support request into the capability-specific
    /// control tables. The session aggregate remains the command source; this
    /// row is the immutable approval binding required by capability issuance.
    pub async fn persist_support_access_request(
        &self,
        session: &SupportSession,
    ) -> Result<(), PersistenceError> {
        let payload = serde_json::to_value(session)?;
        sqlx::query(
            "INSERT INTO erpx_control.support_access_requests
                (id, ticket_id, customer_account_id, agent_ref,
                 represented_tenant_user, mode, permissions, scope, reason,
                 requested_at, expires_at, status, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'requested', 1, $12)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(session.id)
        .bind(session.ticket_id)
        .bind(session.customer_account_id)
        .bind(&session.agent_ref)
        .bind(&session.represented_tenant_user)
        .bind(support_mode(session.mode))
        .bind(serde_json::to_value(&session.permissions)?)
        .bind(&session.scope)
        .bind(&session.reason)
        .bind(session.requested_at)
        .bind(session.expires_at)
        .bind(payload)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    /// Record a single immutable customer approval decision. The approval
    /// token itself is never persisted; callers provide a bounded digest as
    /// `approval_ref`.
    pub async fn approve_support_access_request(
        &self,
        session: &SupportSession,
        approval_ref: &str,
    ) -> Result<(), PersistenceError> {
        if approval_ref.trim().is_empty() || approval_ref.len() > 128 {
            return Err(PersistenceError::InvalidData(
                "support approval reference is invalid".into(),
            ));
        }
        let payload = serde_json::to_value(session)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_access_requests
             SET status = 'approved', approved_by = $2, approved_at = $3,
                 approval_ref = $4, version = version + 1, payload = $5,
                 updated_at = now()
             WHERE id = $1 AND status = 'requested' AND approved_at IS NULL
               AND requested_at <= $3 AND $3 < expires_at",
        )
        .bind(session.id)
        .bind(session.approved_by.as_deref().ok_or_else(|| {
            PersistenceError::InvalidData("approved support session has no approver".into())
        })?)
        .bind(session.approved_at.ok_or_else(|| {
            PersistenceError::InvalidData("approved support session has no timestamp".into())
        })?)
        .bind(approval_ref)
        .bind(payload)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "support_access_request".into(),
                aggregate_id: session.id,
                expected: Some(1),
                actual: None,
            });
        }
        Ok(())
    }

    /// Persist only the hash-bearing capability representation. Raw bearer
    /// material is owned by the caller for one response and cannot be rebuilt
    /// from this row.
    pub async fn persist_support_capability(
        &self,
        capability: &SupportCapability,
    ) -> Result<(), PersistenceError> {
        let mut payload = serde_json::to_value(capability)?;
        if let Some(object) = payload.as_object_mut() {
            object.remove("secret_hash");
        }
        let hash: Vec<u8> = serde_json::from_value(serde_json::to_value(capability.secret_hash())?)
            .map_err(|_| {
                PersistenceError::InvalidData("support capability hash is invalid".into())
            })?;
        if hash.len() != 32 {
            return Err(PersistenceError::InvalidData(
                "support capability hash must be exactly 32 bytes".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.support_capabilities
                (id, access_request_id, customer_account_id, agent_ref, device_ref,
                 ticket_id, mode, permissions, scope, module, issued_at, expires_at,
                 secret_hash, reissued_from, status, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, 'active', 1, $15)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(capability.id)
        .bind(capability.binding.session_id)
        .bind(capability.binding.customer_account_id)
        .bind(&capability.binding.agent_ref)
        .bind(&capability.binding.device_ref)
        .bind(capability.binding.ticket_id)
        .bind(support_mode(capability.binding.mode))
        .bind(serde_json::to_value(&capability.binding.permissions)?)
        .bind(&capability.binding.scope)
        .bind(&capability.binding.module)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(hash)
        .bind(capability.reissued_from)
        .bind(payload)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    /// Commit the mirrored customer approval and its hash-only capability in
    /// one control-plane transaction. The authoritative support aggregate and
    /// outbox command are committed by the application boundary before this
    /// mirror is called; keeping this mirror atomic prevents an approved
    /// request from being left without its capability binding when the insert
    /// fails.
    pub async fn persist_approved_support_capability(
        &self,
        session: &SupportSession,
        capability: &SupportCapability,
        approval_ref: &str,
    ) -> Result<(), PersistenceError> {
        if approval_ref.trim().is_empty() || approval_ref.len() > 128 {
            return Err(PersistenceError::InvalidData(
                "support approval reference is invalid".into(),
            ));
        }
        let approver = session.approved_by.as_deref().ok_or_else(|| {
            PersistenceError::InvalidData("approved support session has no approver".into())
        })?;
        let approved_at = session.approved_at.ok_or_else(|| {
            PersistenceError::InvalidData("approved support session has no timestamp".into())
        })?;
        let payload = serde_json::to_value(session)?;
        let mut capability_payload = serde_json::to_value(capability)?;
        if let Some(object) = capability_payload.as_object_mut() {
            object.remove("secret_hash");
        }
        let hash: Vec<u8> = serde_json::from_value(serde_json::to_value(capability.secret_hash())?)
            .map_err(|_| {
                PersistenceError::InvalidData("support capability hash is invalid".into())
            })?;
        if hash.len() != 32 {
            return Err(PersistenceError::InvalidData(
                "support capability hash must be exactly 32 bytes".into(),
            ));
        }

        let mut tx = self.pools.write.begin().await?;
        let updated = sqlx::query(
            "UPDATE erpx_control.support_access_requests
             SET status = 'approved', approved_by = $2, approved_at = $3,
                 approval_ref = $4, version = version + 1, payload = $5,
                 updated_at = now()
             WHERE id = $1 AND status = 'requested' AND approved_at IS NULL
               AND requested_at <= $3 AND $3 < expires_at",
        )
        .bind(session.id)
        .bind(approver)
        .bind(approved_at)
        .bind(approval_ref)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            let existing = sqlx::query(
                "SELECT status, approved_by, approved_at, approval_ref
                 FROM erpx_control.support_access_requests WHERE id = $1 FOR UPDATE",
            )
            .bind(session.id)
            .fetch_optional(&mut *tx)
            .await?;
            let already_committed = existing.is_some_and(|row| {
                row.try_get::<String, _>("status").ok().as_deref() == Some("approved")
                    && row
                        .try_get::<Option<String>, _>("approved_by")
                        .ok()
                        .flatten()
                        == Some(approver.to_owned())
                    && row
                        .try_get::<Option<DateTime<Utc>>, _>("approved_at")
                        .ok()
                        .flatten()
                        == Some(approved_at)
                    && row
                        .try_get::<Option<String>, _>("approval_ref")
                        .ok()
                        .flatten()
                        == Some(approval_ref.to_owned())
            });
            if !already_committed {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "support_access_request".into(),
                    aggregate_id: session.id,
                    expected: Some(1),
                    actual: None,
                });
            }
        }

        sqlx::query(
            "INSERT INTO erpx_control.support_capabilities
                (id, access_request_id, customer_account_id, agent_ref, device_ref,
                 ticket_id, mode, permissions, scope, module, issued_at, expires_at,
                 secret_hash, reissued_from, status, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, 'active', 1, $15)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(capability.id)
        .bind(capability.binding.session_id)
        .bind(capability.binding.customer_account_id)
        .bind(&capability.binding.agent_ref)
        .bind(&capability.binding.device_ref)
        .bind(capability.binding.ticket_id)
        .bind(support_mode(capability.binding.mode))
        .bind(serde_json::to_value(&capability.binding.permissions)?)
        .bind(&capability.binding.scope)
        .bind(&capability.binding.module)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(hash)
        .bind(capability.reissued_from)
        .bind(capability_payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Rehydrate the hash-only support capability from the authoritative
    /// control write database. The bearer itself is never read from storage.
    pub async fn load_support_capability(
        &self,
        id: Uuid,
    ) -> Result<Option<SupportCapability>, PersistenceError> {
        let Some(row) = sqlx::query(
            "SELECT id, customer_account_id, agent_ref, device_ref,
                    access_request_id, ticket_id, mode, permissions, scope, module,
                    issued_at, expires_at, secret_hash, reissued_from,
                    status, revoked_at, revocation_reason
             FROM erpx_control.support_capabilities
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.write)
        .await?
        else {
            return Ok(None);
        };
        let permissions: Vec<String> = serde_json::from_value(row.try_get("permissions")?)?;
        let binding = SupportCapabilityBinding::new(
            row.try_get("customer_account_id")?,
            row.try_get::<String, _>("agent_ref")?,
            row.try_get::<String, _>("device_ref")?,
            row.try_get("access_request_id")?,
            row.try_get("ticket_id")?,
            parse_support_mode(row.try_get::<String, _>("mode")?.as_str())?,
            permissions,
            row.try_get::<String, _>("scope")?,
            row.try_get::<String, _>("module")?,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let secret_hash: Vec<u8> = row.try_get("secret_hash")?;
        let secret_hash: [u8; 32] = secret_hash
            .try_into()
            .map_err(|_| PersistenceError::InvalidData("support capability hash length".into()))?;
        let status: String = row.try_get("status")?;
        let revocation = match status.as_str() {
            "active" | "expired" => None,
            "revoked" => Some(SupportCapabilityRevocation {
                reason: parse_support_capability_revocation_reason(
                    row.try_get::<Option<String>, _>("revocation_reason")?
                        .as_deref()
                        .ok_or_else(|| {
                            PersistenceError::InvalidData(
                                "revoked support capability has no reason".into(),
                            )
                        })?,
                )?,
                revoked_at: row
                    .try_get::<Option<DateTime<Utc>>, _>("revoked_at")?
                    .ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "revoked support capability has no timestamp".into(),
                        )
                    })?,
            }),
            _ => {
                return Err(PersistenceError::InvalidData(
                    "unknown support capability status".into(),
                ));
            }
        };
        SupportCapability::from_persisted(
            row.try_get("id")?,
            binding,
            row.try_get("issued_at")?,
            row.try_get("expires_at")?,
            secret_hash,
            row.try_get("reissued_from")?,
            revocation,
        )
        .map(Some)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
    }

    /// Terminate every live capability issued from a revoked support session.
    /// Each transaction drains at most a bounded batch, while the loop ensures
    /// a large session cannot leave active capabilities behind. The active
    /// predicate makes repeated session-revocation delivery idempotent.
    pub async fn revoke_support_capabilities_for_session(
        &self,
        session_id: Uuid,
        revoked_at: DateTime<Utc>,
    ) -> Result<u64, PersistenceError> {
        if session_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "support session id must not be nil".into(),
            ));
        }
        let mut total = 0;
        loop {
            let result = sqlx::query(
                "WITH candidates AS (
                     SELECT id
                     FROM erpx_control.support_capabilities
                     WHERE access_request_id = $1 AND status = 'active'
                       AND revoked_at IS NULL
                     ORDER BY id
                     FOR UPDATE SKIP LOCKED
                     LIMIT $3
                 )
                 UPDATE erpx_control.support_capabilities AS capability
                 SET status = 'revoked', revoked_at = $2,
                     revocation_reason = 'session_revoked', version = version + 1,
                     updated_at = $2
                 FROM candidates
                 WHERE capability.id = candidates.id",
            )
            .bind(session_id)
            .bind(revoked_at)
            .bind(SUPPORT_CAPABILITY_REVOKE_BATCH_SIZE)
            .execute(&self.pools.write)
            .await?;
            let batch = result.rows_affected();
            total += batch;
            if batch < SUPPORT_CAPABILITY_REVOKE_BATCH_SIZE as u64 {
                return Ok(total);
            }
        }
    }

    /// Terminate live capabilities for a terminal support ticket. Closing a
    /// ticket is a control-plane revocation event. Each transaction drains at
    /// most a bounded batch, while the loop ensures a large ticket cannot
    /// leave active capabilities behind. The active predicate makes duplicate
    /// lifecycle delivery idempotent.
    pub async fn revoke_support_capabilities_for_ticket(
        &self,
        ticket_id: Uuid,
        revoked_at: DateTime<Utc>,
    ) -> Result<u64, PersistenceError> {
        if ticket_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "support ticket id must not be nil".into(),
            ));
        }
        let mut total = 0;
        loop {
            let result = sqlx::query(
                "WITH candidates AS (
                     SELECT id
                     FROM erpx_control.support_capabilities
                     WHERE ticket_id = $1 AND status = 'active' AND revoked_at IS NULL
                     ORDER BY id
                     FOR UPDATE SKIP LOCKED
                     LIMIT $3
                 )
                 UPDATE erpx_control.support_capabilities AS capability
                 SET status = 'revoked', revoked_at = $2,
                     revocation_reason = 'ticket_closed', version = version + 1,
                     updated_at = $2
                 FROM candidates
                 WHERE capability.id = candidates.id",
            )
            .bind(ticket_id)
            .bind(revoked_at)
            .bind(SUPPORT_CAPABILITY_REVOKE_BATCH_SIZE)
            .execute(&self.pools.write)
            .await?;
            let batch = result.rows_affected();
            total += batch;
            if batch < SUPPORT_CAPABILITY_REVOKE_BATCH_SIZE as u64 {
                return Ok(total);
            }
        }
    }

    pub async fn record_worker_heartbeat(
        &self,
        worker_id: Uuid,
        observed_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "worker heartbeat id must not be nil".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.worker_heartbeats
                (worker_id, last_seen_at, updated_at, status)
             VALUES ($1, $2, $2, 'ready')
             ON CONFLICT (worker_id) DO UPDATE SET
                last_seen_at = EXCLUDED.last_seen_at,
                updated_at = EXCLUDED.updated_at,
                status = EXCLUDED.status",
        )
        .bind(worker_id)
        .bind(observed_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    pub async fn load_worker_heartbeat(
        &self,
        worker_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "worker heartbeat id must not be nil".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT last_seen_at
             FROM erpx_control.worker_heartbeats
             WHERE worker_id = $1 AND status = 'ready'",
        )
        .bind(worker_id)
        .fetch_optional(&self.pools.write)
        .await?;
        row.map(|row| row.try_get("last_seen_at"))
            .transpose()
            .map_err(PersistenceError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SodDecision {
    pub principal_ref: String,
    pub module_key: String,
    pub scope: Scope,
    pub passed: bool,
    pub reviewer_ref: String,
    pub reason: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub decision_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportCapabilityAuthorizationRequest {
    pub capability_id: Uuid,
    pub customer_account_id: Uuid,
    pub agent_ref: String,
    pub device_ref: String,
    pub session_id: Uuid,
    pub ticket_id: Uuid,
    pub mode: String,
    pub permission: String,
    pub scope: String,
    pub module: String,
    pub secret_hex: String,
    pub at: DateTime<Utc>,
}

fn effective_role_chain(
    assignment: &RoleAssignment,
    all: &HashMap<Uuid, RoleAssignment>,
    at: DateTime<Utc>,
    path: &mut Vec<Uuid>,
) -> bool {
    if !assignment.is_active_at(at) || path.contains(&assignment.id) {
        return false;
    }
    let Some(parent_id) = assignment.parent_assignment_id else {
        return true;
    };
    let Some(parent) = all.get(&parent_id) else {
        return false;
    };
    if !parent.is_active_at(at)
        || !parent.scope.contains(&assignment.scope)
        || assignment.authority_level > parent.authority_level
        || assignment
            .permissions
            .iter()
            .any(|permission| !parent.permissions.contains(permission))
    {
        return false;
    }
    path.push(assignment.id);
    let effective = effective_role_chain(parent, all, at, path);
    path.pop();
    effective
}

fn app_error(error: PersistenceError) -> ApplicationError {
    ApplicationError::Repository(error.to_string())
}

fn customer_detail_application_error(error: CustomerDetailReadError) -> ApplicationError {
    ApplicationError::Repository(error.to_string())
}

fn validate_customer_scope_binding(
    binding: &CustomerScopeBindingProjection,
) -> Result<(), PersistenceError> {
    if binding.customer_id.is_nil()
        || binding.holding_id.is_nil()
        || binding.company_id.is_some_and(|id| id.is_nil())
        || binding.branch_id.is_some_and(|id| id.is_nil())
        || (binding.branch_id.is_some() && binding.company_id.is_none())
    {
        return Err(PersistenceError::InvalidData(
            "customer scope binding is invalid".into(),
        ));
    }
    Ok(())
}

fn json<T: DeserializeOwned>(row: &PgRow) -> Result<T, PersistenceError> {
    Ok(serde_json::from_value(row.try_get::<Value, _>("payload")?)?)
}

fn customer_projection_from_payload(
    payload: Value,
) -> Result<Customer360Projection, PersistenceError> {
    let looks_enriched = payload
        .get("account")
        .and_then(Value::as_object)
        .is_some_and(|account| account.contains_key("identity"))
        || payload.get("organization").is_some();
    if looks_enriched {
        return serde_json::from_value(payload).map_err(|error| {
            PersistenceError::InvalidData(format!(
                "invalid enriched Customer 360 projection: {error}"
            ))
        });
    }

    let legacy = serde_json::from_value::<Customer360>(payload).map_err(|error| {
        PersistenceError::InvalidData(format!("invalid Customer 360 projection payload: {error}"))
    })?;
    Ok(Customer360Projection::from_legacy(&legacy))
}

fn customer_from_projection_payload(row: &PgRow) -> Result<Customer360, PersistenceError> {
    let payload: Value = row.try_get("payload")?;
    match serde_json::from_value::<Customer360>(payload.clone()) {
        Ok(customer) => Ok(customer),
        Err(legacy_error) => serde_json::from_value::<Customer360Projection>(payload)
            .map(|projection| projection.to_legacy())
            .map_err(|projection_error| {
                PersistenceError::InvalidData(format!(
                    "invalid Customer 360 projection payload (legacy: {legacy_error}; enriched: {projection_error})"
                ))
            }),
    }
}

/// The database key is the read-model identity boundary. A projection payload
/// is rebuildable data and must never be allowed to substitute another
/// customer's identity merely because its JSON happens to deserialize.
/// Keep the failure deliberately generic so malformed persisted payloads do
/// not echo customer data, registration numbers, or other sensitive fields.
fn customer_from_projection_row(row: &PgRow) -> Result<Customer360, PersistenceError> {
    let read_model_id: Uuid = row.try_get("id")?;
    let customer = customer_from_projection_payload(row)?;
    if customer.account.id != read_model_id {
        return Err(PersistenceError::InvalidData(
            "customer read projection identity does not match read-model key".into(),
        ));
    }
    Ok(customer)
}

fn json_field<T: DeserializeOwned>(row: &PgRow, field: &str) -> Result<T, PersistenceError> {
    Ok(serde_json::from_value(row.try_get::<Value, _>(field)?)?)
}

fn positive_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn positive_provider_priority(value: i32) -> Result<u32, PersistenceError> {
    u32::try_from(value)
        .ok()
        .filter(|value| (1..=erpx_application::MAX_MARKETING_PROVIDER_PRIORITY).contains(value))
        .ok_or_else(|| PersistenceError::InvalidData("provider priority is invalid".into()))
}

fn customer_status(status: CustomerStatus) -> &'static str {
    match status {
        CustomerStatus::Prospect => "prospect",
        CustomerStatus::Onboarding => "onboarding",
        CustomerStatus::Active => "active",
        CustomerStatus::Suspended => "suspended",
        CustomerStatus::Churned => "churned",
    }
}

fn billing_state(state: erpx_domain::commercial::BillingState) -> &'static str {
    match state {
        erpx_domain::commercial::BillingState::Trialing => "trialing",
        erpx_domain::commercial::BillingState::Active => "active",
        erpx_domain::commercial::BillingState::PastDue => "past_due",
        erpx_domain::commercial::BillingState::Grace => "grace",
        erpx_domain::commercial::BillingState::ReadOnly => "read_only",
        erpx_domain::commercial::BillingState::Canceled => "canceled",
    }
}

fn ticket_status(status: erpx_domain::support::TicketStatus) -> &'static str {
    match status {
        erpx_domain::support::TicketStatus::New => "new",
        erpx_domain::support::TicketStatus::Triaged => "triaged",
        erpx_domain::support::TicketStatus::WaitingForCustomer => "waiting_for_customer",
        erpx_domain::support::TicketStatus::AccessRequested => "access_requested",
        erpx_domain::support::TicketStatus::Approved => "approved",
        erpx_domain::support::TicketStatus::InProgress => "in_progress",
        erpx_domain::support::TicketStatus::Resolved => "resolved",
        erpx_domain::support::TicketStatus::CustomerConfirmed => "customer_confirmed",
        erpx_domain::support::TicketStatus::Closed => "closed",
    }
}

fn incident_status(status: IncidentStatus) -> &'static str {
    match status {
        IncidentStatus::Investigating => "investigating",
        IncidentStatus::Identified => "identified",
        IncidentStatus::Monitoring => "monitoring",
        IncidentStatus::Resolved => "resolved",
        IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn support_session_status(session: &SupportSession) -> &'static str {
    if session.revoked_at.is_some() {
        "revoked"
    } else if session.approved_at.is_some() {
        "approved"
    } else {
        "requested"
    }
}

fn support_mode(mode: SupportMode) -> &'static str {
    match mode {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::TenantMutation => "tenant_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}

fn parse_support_mode(value: &str) -> Result<SupportMode, PersistenceError> {
    match value {
        "diagnostic_read" => Ok(SupportMode::DiagnosticRead),
        "tenant_mutation" => Ok(SupportMode::TenantMutation),
        "impersonation" => Ok(SupportMode::Impersonation),
        _ => Err(PersistenceError::InvalidData("unknown support mode".into())),
    }
}

fn parse_support_capability_revocation_reason(
    value: &str,
) -> Result<SupportCapabilityRevocationReason, PersistenceError> {
    match value {
        "expired" => Ok(SupportCapabilityRevocationReason::Expired),
        "customer_requested" => Ok(SupportCapabilityRevocationReason::CustomerRequested),
        "agent_role_expired" => Ok(SupportCapabilityRevocationReason::AgentRoleExpired),
        "parent_role_expired" => Ok(SupportCapabilityRevocationReason::ParentRoleExpired),
        "session_revoked" => Ok(SupportCapabilityRevocationReason::SessionRevoked),
        "ticket_closed" => Ok(SupportCapabilityRevocationReason::TicketClosed),
        "customer_suspended" => Ok(SupportCapabilityRevocationReason::CustomerSuspended),
        "administrative" => Ok(SupportCapabilityRevocationReason::Administrative),
        "reissued" => Ok(SupportCapabilityRevocationReason::Reissued),
        _ => Err(PersistenceError::InvalidData(
            "unknown support capability revocation reason".into(),
        )),
    }
}

#[async_trait]
impl CustomerReadStore for SqlxControlPlane {
    async fn list_customers(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<Customer360>, ApplicationError> {
        let search = search.filter(|value| !value.trim().is_empty());
        let rows = sqlx::query(
            "SELECT id, payload
             FROM erpx_control.customers
             WHERE $1::text IS NULL
                OR account_code ILIKE '%' || $1 || '%'
                OR trading_name ILIKE '%' || $1 || '%'
                OR coalesce(legal_name, '') ILIKE '%' || $1 || '%'
             ORDER BY trading_name, id",
        )
        .bind(search)
        .fetch_all(&self.pools.read)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        rows.iter()
            .map(customer_from_projection_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(app_error)
    }

    async fn get_customer(&self, id: Uuid) -> Result<Option<Customer360>, ApplicationError> {
        let row = sqlx::query("SELECT id, payload FROM erpx_control.customers WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        row.as_ref()
            .map(customer_from_projection_row)
            .transpose()
            .map_err(app_error)
    }
}

#[async_trait]
impl Customer360DetailReadStore for SqlxControlPlane {
    async fn get_customer_organization(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOrganizationReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerOrganizationReadDto::from(&projection)))
    }

    async fn get_customer_contacts(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerContactsReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerContactsReadDto::from(&projection)))
    }

    async fn get_customer_commercial(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerCommercialReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerCommercialReadDto::from(&projection)))
    }

    async fn get_customer_entitlements(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerEntitlementsReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerEntitlementsReadDto::from(&projection)))
    }

    async fn get_customer_onboarding(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerOnboardingReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerOnboardingReadDto::from(&projection)))
    }

    async fn get_customer_health(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerHealthReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerHealthReadDto::from(&projection)))
    }

    async fn get_customer_support_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerSupportSummaryReadDto>, ApplicationError> {
        Ok(self
            .read_customer_projection_view(id)
            .await?
            .map(|projection| CustomerSupportSummaryReadDto::from(&projection)))
    }

    async fn get_customer_access_audit_summary(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerAccessAuditSummaryReadDto>, ApplicationError> {
        let projection_view = self.read_customer_projection_view(id).await?;
        let projection_metadata = match projection_view {
            CustomerReadView::Present { metadata, .. } => metadata,
            CustomerReadView::Absent { metadata } => {
                return Ok(CustomerReadView::Absent { metadata });
            }
            CustomerReadView::Unsupported { metadata, reason } => {
                return Ok(CustomerReadView::Unsupported { metadata, reason });
            }
        };
        let row = sqlx::query(
            "SELECT read_count, last_access_at, customer_visible, updated_at
             FROM erpx_control.customer_access_audit_summaries
             WHERE customer_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.read)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        let fallback_updated_at = projection_metadata
            .projection_updated_at
            .unwrap_or_else(Utc::now);
        let (data, updated_at) = if let Some(row) = row {
            let read_count = positive_u64(
                row.try_get("read_count")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                "customer access audit read count",
            )
            .map_err(app_error)?;
            (
                CustomerAccessAuditSummaryReadDto {
                    read_count,
                    last_access_at: row
                        .try_get("last_access_at")
                        .map_err(|error| app_error(PersistenceError::Database(error)))?,
                    customer_visible: row
                        .try_get("customer_visible")
                        .map_err(|error| app_error(PersistenceError::Database(error)))?,
                },
                row.try_get("updated_at")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
            )
        } else {
            (
                CustomerAccessAuditSummaryReadDto {
                    read_count: 0,
                    last_access_at: None,
                    customer_visible: true,
                },
                fallback_updated_at,
            )
        };
        Ok(CustomerReadView::present(data, id, updated_at, Utc::now()))
    }

    async fn get_customer_notes(
        &self,
        id: Uuid,
    ) -> Result<CustomerReadView<CustomerNotesReadDto>, ApplicationError> {
        Ok(self
            .get_customer_notes_page(id, None, CUSTOMER_DETAIL_MAX_PAGE_SIZE)
            .await?
            .map(|page| CustomerNotesReadDto { notes: page.notes }))
    }
}

#[async_trait]
impl Customer360NoteWritePort for SqlxControlPlane {
    async fn append_customer_note(
        &self,
        command: AddCustomerNoteCommand,
    ) -> Result<CustomerNoteWriteReceipt, ApplicationError> {
        if !self
            .customer_exists_on_write(command.note.customer_id)
            .await
            .map_err(app_error)?
        {
            return Err(ApplicationError::NotFound);
        }
        if !matches!(command.note.scope, Scope::Platform)
            && !self
                .customer_scope_contains(command.note.customer_id, &command.note.scope)
                .await
                .map_err(app_error)?
        {
            return Err(ApplicationError::Authorization(
                "customer note scope is not bound to the requested customer".into(),
            ));
        }
        let mutation = command.into_mutation()?;
        let receipt = CommandUnitOfWork::execute_command(self, mutation).await?;
        let mut note_receipt: CustomerNoteWriteReceipt =
            serde_json::from_value(receipt.response)
                .map_err(|error| ApplicationError::Repository(error.to_string()))?;
        note_receipt.replayed = receipt.replayed;
        Ok(note_receipt)
    }
}

#[async_trait]
impl CustomerGlobalReadAuditDetailStore for SqlxControlPlane {
    async fn get_customer_global_read_audit_details(
        &self,
        query: &CustomerGlobalReadAuditDetailQuery,
    ) -> Result<CustomerReadView<CustomerGlobalReadAuditDetailPage>, ApplicationError> {
        query
            .validate()
            .map_err(|error| app_error(PersistenceError::InvalidData(error)))?;
        let projection_view = self
            .read_customer_projection_view(query.customer_id)
            .await?;
        let projection_metadata = match projection_view {
            CustomerReadView::Present { metadata, .. } => metadata,
            CustomerReadView::Absent { metadata } => {
                return Ok(CustomerReadView::Absent { metadata });
            }
            CustomerReadView::Unsupported { metadata, reason } => {
                return Ok(CustomerReadView::Unsupported { metadata, reason });
            }
        };
        let latest_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT MAX(occurred_at)
             FROM erpx_control.global_read_audits
             WHERE customer_id = $1
               AND (
                    $2::boolean
                    OR (scope->>'holding_id' IS NULL
                        AND scope->>'company_id' IS NULL
                        AND scope->>'branch_id' IS NULL)
                    OR (
                        $3::uuid IS NOT NULL
                        AND $4::uuid IS NULL
                        AND scope->>'holding_id' = $3::text
                    )
                    OR (
                        $4::uuid IS NOT NULL
                        AND $5::uuid IS NULL
                        AND scope->>'holding_id' = $3::text
                        AND scope->>'company_id' = $4::text
                    )
                    OR (
                        $5::uuid IS NOT NULL
                        AND scope->>'holding_id' = $3::text
                        AND scope->>'company_id' = $4::text
                        AND scope->>'branch_id' = $5::text
                    )
               )",
        )
        .bind(query.customer_id)
        .bind(query.security_admin_scope.platform)
        .bind(query.security_admin_scope.holding_id)
        .bind(query.security_admin_scope.company_id)
        .bind(query.security_admin_scope.branch_id)
        .fetch_one(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        let limit = query.bounded_limit();
        let fetch_limit = limit.saturating_add(1);
        let rows = sqlx::query(
            "SELECT id, actor_ref, session_ref, customer_id, module, scope,
                    purpose, occurred_at, correlation_id
             FROM erpx_control.global_read_audits
             WHERE customer_id = $1
               AND (
                    $2::boolean
                    OR (scope->>'holding_id' IS NULL
                        AND scope->>'company_id' IS NULL
                        AND scope->>'branch_id' IS NULL)
                    OR (
                        $3::uuid IS NOT NULL
                        AND $4::uuid IS NULL
                        AND scope->>'holding_id' = $3::text
                    )
                    OR (
                        $4::uuid IS NOT NULL
                        AND $5::uuid IS NULL
                        AND scope->>'holding_id' = $3::text
                        AND scope->>'company_id' = $4::text
                    )
                    OR (
                        $5::uuid IS NOT NULL
                        AND scope->>'holding_id' = $3::text
                        AND scope->>'company_id' = $4::text
                        AND scope->>'branch_id' = $5::text
                    )
               )
               AND ($6::timestamptz IS NULL
                    OR (occurred_at, id) < ($6::timestamptz, $7::uuid))
             ORDER BY occurred_at DESC, id DESC
             LIMIT $8",
        )
        .bind(query.customer_id)
        .bind(query.security_admin_scope.platform)
        .bind(query.security_admin_scope.holding_id)
        .bind(query.security_admin_scope.company_id)
        .bind(query.security_admin_scope.branch_id)
        .bind(query.before.as_ref().map(|cursor| cursor.occurred_at))
        .bind(query.before.as_ref().map(|cursor| cursor.audit_id))
        .bind(i64::from(fetch_limit))
        .fetch_all(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let row_customer_id: Uuid = row
                .try_get("customer_id")
                .map_err(|error| app_error(PersistenceError::Database(error)))?;
            if row_customer_id != query.customer_id {
                return Err(app_error(PersistenceError::InvalidData(
                    "global-read audit customer key mismatch".into(),
                )));
            }
            let scope: GlobalReadScope = serde_json::from_value(
                row.try_get("scope")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
            )
            .map_err(|error| app_error(PersistenceError::Serialization(error)))?;
            let audit_id: Uuid = row
                .try_get("id")
                .map_err(|error| app_error(PersistenceError::Database(error)))?;
            let actor = GlobalReadActor {
                principal_ref: row
                    .try_get("actor_ref")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                session_ref: row
                    .try_get("session_ref")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
            };
            entries.push(CustomerGlobalReadAuditDetailReadDto {
                audit_id,
                actor,
                customer_id: row_customer_id,
                module: row
                    .try_get("module")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                scope,
                purpose: row
                    .try_get("purpose")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                occurred_at: row
                    .try_get("occurred_at")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
                correlation_id: row
                    .try_get("correlation_id")
                    .map_err(|error| app_error(PersistenceError::Database(error)))?,
            });
        }

        let has_more = entries.len() > limit as usize;
        if has_more {
            entries.truncate(limit as usize);
        }
        let next_before = has_more
            .then(|| {
                entries
                    .last()
                    .map(|entry| CustomerGlobalReadAuditDetailCursor {
                        occurred_at: entry.occurred_at,
                        audit_id: entry.audit_id,
                    })
            })
            .flatten();
        let updated_at = latest_at
            .or(projection_metadata.projection_updated_at)
            .unwrap_or_else(Utc::now);
        Ok(CustomerReadView::present(
            CustomerGlobalReadAuditDetailPage {
                entries,
                next_before,
            },
            query.customer_id,
            updated_at,
            Utc::now(),
        ))
    }
}

#[async_trait]
impl SupportStore for SqlxControlPlane {
    async fn create_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
        let payload = serde_json::to_value(&ticket).map_err(|error| app_error(error.into()))?;
        sqlx::query(
            "INSERT INTO erpx_control.support_tickets
                (id, customer_account_id, status, severity, version, payload)
             VALUES ($1, $2, $3, $4, 1, $5)",
        )
        .bind(ticket.id)
        .bind(ticket.customer_account_id)
        .bind(ticket_status(ticket.status))
        .bind(&ticket.severity)
        .bind(payload)
        .execute(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        Ok(())
    }

    async fn get_ticket(&self, id: Uuid) -> Result<Option<SupportTicket>, ApplicationError> {
        let row = sqlx::query("SELECT payload FROM erpx_control.support_tickets WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pools.write)
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        if let Some(row) = row {
            return json(&row).map(Some).map_err(app_error);
        }
        let snapshot = sqlx::query(
            "SELECT state FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'support_ticket' AND aggregate_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        snapshot
            .as_ref()
            .map(|row| json_field(row, "state"))
            .transpose()
            .map_err(app_error)
    }

    async fn save_ticket(&self, ticket: SupportTicket) -> Result<(), ApplicationError> {
        let version = self
            .current_ticket_version(ticket.id)
            .await
            .map_err(app_error)?;
        self.save_ticket_if_version(&ticket, version)
            .await
            .map(|_| ())
            .map_err(app_error)
    }

    async fn create_session(&self, session: SupportSession) -> Result<(), ApplicationError> {
        let payload = serde_json::to_value(&session).map_err(|error| app_error(error.into()))?;
        sqlx::query(
            "INSERT INTO erpx_control.support_sessions
                (id, ticket_id, customer_account_id, status, expires_at, version, payload)
             VALUES ($1, $2, $3, $4, $5, 1, $6)",
        )
        .bind(session.id)
        .bind(session.ticket_id)
        .bind(session.customer_account_id)
        .bind(support_session_status(&session))
        .bind(session.expires_at)
        .bind(payload)
        .execute(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        Ok(())
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<SupportSession>, ApplicationError> {
        let row = sqlx::query("SELECT payload FROM erpx_control.support_sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pools.write)
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        if let Some(row) = row {
            return json(&row).map(Some).map_err(app_error);
        }
        let snapshot = sqlx::query(
            "SELECT state FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'support_session' AND aggregate_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        snapshot
            .as_ref()
            .map(|row| json_field(row, "state"))
            .transpose()
            .map_err(app_error)
    }

    async fn save_session(&self, session: SupportSession) -> Result<(), ApplicationError> {
        let version = self
            .current_session_version(session.id)
            .await
            .map_err(app_error)?;
        self.save_session_if_version(&session, version)
            .await
            .map(|_| ())
            .map_err(app_error)
    }
}

const DEVELOPMENT_ONLY_ROLE_PERMISSIONS: [&str; 2] =
    ["development.mfa.bypass", "development.sod.self-approve"];

fn contains_development_only_role_permission(permissions: &[String]) -> bool {
    permissions
        .iter()
        .any(|permission| DEVELOPMENT_ONLY_ROLE_PERMISSIONS.contains(&permission.as_str()))
}

#[async_trait]
impl RoleAssignmentStore for SqlxControlPlane {
    async fn create(&self, assignment: RoleAssignment) -> Result<(), ApplicationError> {
        if contains_development_only_role_permission(&assignment.permissions) {
            return Err(app_error(PersistenceError::InvalidData(
                "development-only authorization exceptions cannot be granted through role lifecycle"
                    .into(),
            )));
        }
        let payload = serde_json::to_value(&assignment).map_err(|error| app_error(error.into()))?;
        let mut transaction = self
            .pools
            .write
            .begin()
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignments
                (id, principal_ref, role_key, parent_assignment_id, valid_from, valid_until, assignment_version, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(assignment.id)
        .bind(&assignment.principal_ref)
        .bind(&assignment.role_key)
        .bind(assignment.parent_assignment_id)
        .bind(assignment.valid_from)
        .bind(assignment.valid_until)
        .bind(i32::try_from(assignment.assignment_version).map_err(|_| {
            app_error(PersistenceError::InvalidData(
                "assignment version exceeds database range".into(),
            ))
        })?)
        .bind(payload)
        .execute(&mut *transaction)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        sqlx::query(
            "INSERT INTO erpx_control.role_assignment_target_policies
                (assignment_id, policy_kind, environment_gate, configured_by)
             VALUES ($1, 'unrestricted', NULL, $2)",
        )
        .bind(assignment.id)
        .bind(&assignment.grantor)
        .execute(&mut *transaction)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        transaction
            .commit()
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<RoleAssignment>, ApplicationError> {
        let row = sqlx::query("SELECT payload FROM erpx_control.role_assignments WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pools.write)
            .await
            .map_err(|error| app_error(PersistenceError::Database(error)))?;
        if let Some(row) = row {
            return json(&row).map(Some).map_err(app_error);
        }
        let snapshot = sqlx::query(
            "SELECT state FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = 'role_assignment' AND aggregate_id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        snapshot
            .as_ref()
            .map(|row| json_field(row, "state"))
            .transpose()
            .map_err(app_error)
    }

    async fn list(&self, limit: u32) -> Result<Vec<RoleAssignment>, ApplicationError> {
        let rows = sqlx::query(
            "SELECT payload FROM erpx_control.role_assignments
             ORDER BY valid_until, id LIMIT $1",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        rows.iter()
            .map(json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(app_error)
    }
}

#[async_trait]
impl AuditSink for SqlxControlPlane {
    async fn record(&self, event: AuditEvent) -> Result<(), ApplicationError> {
        let payload = serde_json::to_value(AuditEventPayload::from(&event))
            .map_err(|error| app_error(error.into()))?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(&event.actor_ref)
        .bind(event.customer_account_id)
        .bind(&event.action)
        .bind(&event.purpose)
        .bind(event.correlation_id)
        .bind(payload)
        .execute(&self.pools.write)
        .await
        .map_err(|error| app_error(PersistenceError::Database(error)))?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct AuditEventPayload<'a> {
    actor_ref: &'a str,
    customer_account_id: Option<Uuid>,
    action: &'a str,
    purpose: &'a str,
    correlation_id: Uuid,
}

impl<'a> From<&'a AuditEvent> for AuditEventPayload<'a> {
    fn from(event: &'a AuditEvent) -> Self {
        Self {
            actor_ref: &event.actor_ref,
            customer_account_id: event.customer_account_id,
            action: &event.action,
            purpose: &event.purpose,
            correlation_id: event.correlation_id,
        }
    }
}

fn validate_tenant_reference(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be opaque, path-safe, and non-empty"
        )));
    }
    Ok(())
}

fn validate_public_hostname(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 253
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | ':')
        })
    {
        return Err(PersistenceError::InvalidData(
            "Marketing public hostname is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finance_provider_route_references_use_opaque_validation() {
        for (field, value) in [
            ("provider_account_ref", ""),
            ("provider_key", "provider/key"),
            ("tenant_id", "tenant id"),
            ("holding_ref", "holding:ref"),
            ("credential_key_ref", "credential\\ref"),
        ] {
            assert!(
                validate_tenant_reference(value, field).is_err(),
                "malformed {field} should be rejected"
            );
        }
        assert!(validate_tenant_reference("provider-test", "provider_key").is_ok());
    }

    #[test]
    fn finance_provider_binding_debug_redacts_credential_reference() {
        let binding = FinanceBankProviderAccountBinding {
            provider_account_ref: "account-ref".into(),
            provider_key: "provider-test".into(),
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            holding_id: Uuid::new_v4(),
            credential_key_ref: "secret-manager-key-ref".into(),
            state: "active".into(),
            version: 1,
        };
        let debug = format!("{binding:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret-manager-key-ref"));
    }

    #[test]
    fn public_hostname_validation_rejects_authority_and_control_values() {
        assert!(validate_public_hostname("example.com").is_ok());
        assert!(validate_public_hostname("example.com:443").is_err());
        assert!(validate_public_hostname("example.com/path").is_err());
        assert!(validate_public_hostname("example.com\nattacker").is_err());
    }

    #[test]
    fn customer_360_reconciliation_is_unhealthy_when_audit_is_unprojected() {
        let mut report = Customer360ReadReconciliation::default();
        assert!(report.healthy());
        report.unprojected_audit_count = 1;
        assert!(!report.healthy());
    }

    #[test]
    fn customer_360_reconciliation_is_unhealthy_when_scope_binding_is_orphaned() {
        let mut report = Customer360ReadReconciliation::default();
        assert!(report.healthy());
        report.orphan_binding_count = 1;
        assert!(!report.healthy());
    }

    #[test]
    fn normal_role_lifecycle_rejects_development_authorization_exceptions() {
        assert!(contains_development_only_role_permission(&[
            "platform.tenant.read".into(),
            "development.mfa.bypass".into(),
        ]));
        assert!(contains_development_only_role_permission(&[
            "development.sod.self-approve".into(),
        ]));
        assert!(!contains_development_only_role_permission(&[
            "platform.tenant.read".into(),
            "tenant.root-access.approve".into(),
        ]));
    }

    #[test]
    fn platform_integration_observations_are_bounded_and_categorical() {
        let checked_at = Utc::now();
        let valid = PlatformIntegrationObservation {
            integration_key: "object_storage".into(),
            state: "ready".into(),
            capability: "Tenant object storage".into(),
            checked_at,
        };
        assert!(valid.validate().is_ok());
        assert!(
            PlatformIntegrationObservation {
                integration_key: "object/storage".into(),
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            PlatformIntegrationObservation {
                state: "secret-provider-error".into(),
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            PlatformIntegrationObservation {
                capability: "x".repeat(257),
                ..valid
            }
            .validate()
            .is_err()
        );
    }
}
