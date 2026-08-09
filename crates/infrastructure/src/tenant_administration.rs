//! Control-plane persistence for the all-tenant registry and short-lived
//! platform tenant-administration contexts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use erpx_domain::commercial::{
    BillingGraceExtension, BillingGraceExtensionGrant, BillingLifecycle, BillingState,
};
use erpx_domain::tenant_administration::{
    CustomerTenantBinding, TenantAdminContext, TenantAdministrationError,
};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::{PersistenceError, SqlxControlPlane, tenant_holding_scope_id};

pub const MAX_TENANT_SUMMARY_PAGE_SIZE: u32 = 100;
pub const MAX_TENANT_CONTROL_HISTORY_PAGE_SIZE: u32 = 100;

const SAFE_TENANT_CONTROL_OUTBOX_ACTIONS: &[&str] = &[
    "corporate_identity.draft.saved",
    "corporate_identity.published",
    "corporate_identity.rolled_back",
    "tenant.agent.enrolled",
    "tenant.agent.enrollment.issued",
    "tenant.agent.observation.recorded",
    "tenant.agent.offline",
    "tenant.configuration.active",
    "tenant.configuration.approved",
    "tenant.configuration.proposed",
    "tenant.configuration.rejected",
    "tenant.infrastructure.profile.updated",
    "tenant.root_access.activated",
    "tenant.root_access.rejected",
    "tenant.root_access.requested",
    "tenant.root_access.revoked",
];

const SAFE_TENANT_CONTROL_AUDIT_ACTIONS: &[&str] = &[
    "organization.units.discover",
    "platform.tenant.enter",
    "platform.tenant.exit",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantControlHistoryCursor {
    pub occurred_at: DateTime<Utc>,
    pub event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantControlHistoryEntry {
    pub event_id: Uuid,
    pub event_category: String,
    pub safe_action: String,
    pub aggregate_version: u64,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantControlHistoryPage {
    pub entries: Vec<TenantControlHistoryEntry>,
    pub has_more: bool,
    pub next_cursor: Option<TenantControlHistoryCursor>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TenantRegistryFilter {
    pub search: Option<String>,
    pub lifecycle: Option<String>,
    pub billing_state: Option<String>,
    pub plan: Option<String>,
    pub module: Option<String>,
    pub health: Option<String>,
    pub after_tenant_id: Option<String>,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantRegistrySummary {
    pub tenant_id: String,
    pub customer_id: Option<Uuid>,
    pub root_holding_id: Option<Uuid>,
    pub customer_name: String,
    pub tenant_state: String,
    pub configuration_state: String,
    pub subscription_state: String,
    pub plan_key: String,
    pub enabled_modules: Vec<String>,
    pub purchased_seats: u32,
    pub used_seats: u32,
    pub company_count: u32,
    pub branch_count: u32,
    pub renewal_at: Option<DateTime<Utc>>,
    pub overdue: bool,
    pub projection_freshness: String,
    pub storage_state: String,
    pub backup_state: String,
    pub integration_health: String,
    pub health: String,
    pub projected_at: DateTime<Utc>,
}

impl TenantRegistrySummary {
    pub fn enterable(&self) -> bool {
        self.configuration_state == "configured"
            && self.customer_id.is_some()
            && self.root_holding_id.is_some()
            && self.tenant_state == "active"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantRegistryPage {
    pub tenants: Vec<TenantRegistrySummary>,
    pub has_more: bool,
    pub next_after_tenant_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantStripeSubscriptionBinding {
    pub customer_id: Uuid,
    pub tenant_id: String,
    pub subscription_id: Uuid,
    pub stripe_customer_ref: String,
    pub stripe_subscription_ref: Option<String>,
    pub currency: String,
    pub item_quantities: std::collections::BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantInvoiceSummary {
    pub invoice_id: String,
    pub state: String,
    pub currency: String,
    pub amount_due_minor: u64,
    pub amount_paid_minor: u64,
    pub due_at: Option<DateTime<Utc>>,
    pub hosted_invoice_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantInvoicePage {
    pub invoices: Vec<TenantInvoiceSummary>,
    pub has_more: bool,
    pub next_after_invoice_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct TenantAdminContextIssue<'a> {
    pub tenant_id: &'a str,
    pub principal_ref: &'a str,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub reason: &'a str,
    pub correlation_id: Uuid,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct TenantBillingGraceExtensionGrant<'a> {
    pub tenant_id: &'a str,
    pub delinquency_cycle_id: Uuid,
    pub extension: chrono::Duration,
    pub granted_at: DateTime<Utc>,
    pub granted_by: &'a str,
    pub reason: &'a str,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct SqlxTenantAdministrationStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxTenantAdministrationStore {
    pub async fn tenant_id_for_root_holding(
        &self,
        root_holding_id: Uuid,
    ) -> Result<Option<String>, PersistenceError> {
        if root_holding_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "root holding ID is required".into(),
            ));
        }
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT tenant_id
             FROM erpx_control.customer_tenant_bindings
             WHERE root_holding_id=$1
             ORDER BY tenant_id
             LIMIT 2",
        )
        .bind(root_holding_id)
        .fetch_all(&self.control.pools.write)
        .await?;
        match rows.as_slice() {
            [] => Ok(None),
            [tenant_id] => Ok(Some(tenant_id.clone())),
            _ => Err(PersistenceError::InvalidData(
                "root holding is bound to multiple tenants".into(),
            )),
        }
    }

    pub async fn list_control_history(
        &self,
        tenant_id: &str,
        cursor: Option<TenantControlHistoryCursor>,
        page_size: u32,
    ) -> Result<TenantControlHistoryPage, PersistenceError> {
        validate_reference(tenant_id, "tenant_id", 128)?;
        if page_size == 0 || page_size > MAX_TENANT_CONTROL_HISTORY_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "tenant control history page size must be between 1 and 100".into(),
            ));
        }
        let limit = i64::from(page_size) + 1;
        let cursor_at = cursor.map(|value| value.occurred_at);
        let cursor_id = cursor.map(|value| value.event_id);
        let rows = sqlx::query(
            "WITH safe_history AS (
                 SELECT id AS event_id,
                        CASE
                          WHEN event_type LIKE 'tenant.root_access.%' THEN 'root_access'
                          WHEN event_type LIKE 'corporate_identity.%' THEN 'corporate_identity'
                          ELSE 'infrastructure'
                        END AS event_category,
                        event_type AS safe_action,
                        aggregate_version,
                        COALESCE(actor->>'principal_ref', 'system') AS actor_ref,
                        correlation_id,
                        occurred_at
                 FROM erpx_control.outbox_events
                 WHERE tenant_scope->>'tenant_id' = $1
                   AND event_type = ANY($4)
                 UNION ALL
                 SELECT id AS event_id,
                        CASE WHEN module_key='organization' THEN 'organization' ELSE 'administration' END,
                        action AS safe_action,
                        0::BIGINT AS aggregate_version,
                        platform_actor_ref AS actor_ref,
                        correlation_id,
                        occurred_at
                 FROM erpx_control.tenant_platform_access_audit
                 WHERE tenant_id = $1
                   AND action = ANY($5)
             )
             SELECT event_id, event_category, safe_action, aggregate_version,
                    actor_ref, correlation_id, occurred_at
             FROM safe_history
             WHERE $2::TIMESTAMPTZ IS NULL
                OR (occurred_at, event_id) < ($2, $3)
             ORDER BY occurred_at DESC, event_id DESC
             LIMIT $6",
        )
        .bind(tenant_id)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(SAFE_TENANT_CONTROL_OUTBOX_ACTIONS)
        .bind(SAFE_TENANT_CONTROL_AUDIT_ACTIONS)
        .bind(limit)
        .fetch_all(&self.control.pools.write)
        .await?;

        let mut entries = rows
            .iter()
            .map(|row| {
                let aggregate_version = u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
                    .map_err(|_| {
                        PersistenceError::InvalidData(
                            "tenant control history aggregate version is invalid".into(),
                        )
                    })?;
                Ok(TenantControlHistoryEntry {
                    event_id: row.try_get("event_id")?,
                    event_category: row.try_get("event_category")?,
                    safe_action: row.try_get("safe_action")?,
                    aggregate_version,
                    actor_ref: row.try_get("actor_ref")?,
                    correlation_id: row.try_get("correlation_id")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let has_more = entries.len() > page_size as usize;
        if has_more {
            entries.truncate(page_size as usize);
        }
        let next_cursor = has_more && !entries.is_empty();
        Ok(TenantControlHistoryPage {
            next_cursor: next_cursor.then(|| {
                let last = entries.last().expect("non-empty history page");
                TenantControlHistoryCursor {
                    occurred_at: last.occurred_at,
                    event_id: last.event_id,
                }
            }),
            entries,
            has_more,
        })
    }

    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    /// Create only an exact customer/tenant/root-holding binding. Mutable names
    /// are never used as ownership evidence.
    pub async fn bind_customer_tenant(
        &self,
        binding: &CustomerTenantBinding,
        bound_by: &str,
    ) -> Result<(), PersistenceError> {
        binding.validate().map_err(tenant_administration_error)?;
        validate_reference(bound_by, "bound_by", 255)?;
        let registry_holding_ref: Option<String> = sqlx::query_scalar(
            "SELECT holding_ref
             FROM erpx_control.tenant_registry
             WHERE tenant_id = $1",
        )
        .bind(&binding.tenant_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        let Some(registry_holding_ref) = registry_holding_ref else {
            return Err(PersistenceError::InvalidData(
                "customer tenant binding references an unknown tenant".into(),
            ));
        };
        if registry_holding_ref != binding.holding_ref
            || tenant_holding_scope_id(&registry_holding_ref) != binding.root_holding_id
        {
            return Err(PersistenceError::InvalidData(
                "customer tenant binding does not exactly match the tenant root holding".into(),
            ));
        }
        let exact_customer_root: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM erpx_control.customer_scope_bindings
                WHERE customer_id = $1
                  AND holding_id = $2
                  AND company_id IS NULL
                  AND branch_id IS NULL
             )",
        )
        .bind(binding.customer_id)
        .bind(binding.root_holding_id)
        .fetch_one(&self.control.pools.write)
        .await?;
        if !exact_customer_root {
            return Err(PersistenceError::InvalidData(
                "customer tenant binding has no exact root-holding evidence".into(),
            ));
        }
        let mut transaction = self.control.pools.write.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_control.customer_tenant_bindings
                (customer_id, tenant_id, root_holding_id, holding_ref,
                 binding_state, bound_by)
             VALUES ($1, $2, $3, $4, 'configured', $5)",
        )
        .bind(binding.customer_id)
        .bind(&binding.tenant_id)
        .bind(binding.root_holding_id)
        .bind(&binding.holding_ref)
        .bind(bound_by)
        .execute(&mut *transaction)
        .await?;
        // Migration 0070 backfills profiles for existing bindings. Create the
        // same safe ERPX-managed default atomically for every later binding so
        // no tenant can exist without an explicit execution route.
        sqlx::query(
            "INSERT INTO erpx_control.tenant_infrastructure_profiles
                (tenant_id, root_holding_id, deployment_mode,
                 write_database_ref, read_database_ref, storage_ref,
                 backup_profile_ref, profile_version, updated_by)
             VALUES ($1, $2, 'erpx_managed', $3, $4, $5, $6, 1, $7)",
        )
        .bind(&binding.tenant_id)
        .bind(binding.root_holding_id)
        .bind(managed_infrastructure_ref(&binding.tenant_id, "write"))
        .bind(managed_infrastructure_ref(&binding.tenant_id, "read"))
        .bind(managed_infrastructure_ref(&binding.tenant_id, "storage"))
        .bind(managed_infrastructure_ref(&binding.tenant_id, "backup"))
        .bind(bound_by)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.tenant_summary_projection
             SET customer_id = $2,
                 root_holding_id = $3,
                 configuration_state = 'configured',
                 projected_at = now()
             WHERE tenant_id = $1",
        )
        .bind(&binding.tenant_id)
        .bind(binding.customer_id)
        .bind(binding.root_holding_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn customer_tenant_binding(
        &self,
        tenant_id: &str,
    ) -> Result<Option<CustomerTenantBinding>, PersistenceError> {
        validate_reference(tenant_id, "tenant_id", 128)?;
        let row = sqlx::query(
            "SELECT customer_id, tenant_id, root_holding_id, holding_ref
             FROM erpx_control.customer_tenant_bindings
             WHERE tenant_id = $1 AND binding_state = 'configured'",
        )
        .bind(tenant_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(|row| {
            CustomerTenantBinding::new(
                row.try_get("customer_id")?,
                row.try_get::<String, _>("tenant_id")?,
                row.try_get("root_holding_id")?,
                row.try_get::<String, _>("holding_ref")?,
            )
            .map_err(tenant_administration_error)
        })
        .transpose()
    }

    pub async fn begin_context(
        &self,
        request: TenantAdminContextIssue<'_>,
    ) -> Result<TenantAdminContext, PersistenceError> {
        let binding = self
            .customer_tenant_binding(request.tenant_id)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "tenant configuration is incomplete and cannot be entered".into(),
                )
            })?;
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM erpx_control.tenant_registry WHERE tenant_id = $1",
        )
        .bind(request.tenant_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        if state.as_deref() != Some("active") {
            return Err(PersistenceError::InvalidData(
                "only an active configured tenant can be entered".into(),
            ));
        }
        let context = TenantAdminContext::issue(
            &binding,
            request.principal_ref,
            request.device_id,
            request.session_id,
            request.reason,
            request.correlation_id,
            request.at,
        )
        .map_err(tenant_administration_error)?;
        let context_hash = context_hash(context.id);
        let mut transaction = self.control.pools.write.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_control.platform_tenant_admin_contexts
                (context_id_hash, principal_ref, device_id, session_id,
                 tenant_id, customer_id, root_holding_id, reason,
                 correlation_id, issued_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(&context_hash)
        .bind(&context.principal_ref)
        .bind(context.device_id)
        .bind(context.session_id)
        .bind(&context.tenant_id)
        .bind(context.customer_id)
        .bind(context.root_holding_id)
        .bind(&context.reason)
        .bind(context.correlation_id)
        .bind(context.issued_at)
        .bind(context.expires_at)
        .execute(&mut *transaction)
        .await?;
        insert_access_audit(
            &mut transaction,
            &context,
            "platform",
            "platform.tenant.enter",
            request.at,
        )
        .await?;
        transaction.commit().await?;
        Ok(context)
    }

    pub async fn get_context(
        &self,
        context_id: Uuid,
    ) -> Result<Option<TenantAdminContext>, PersistenceError> {
        if context_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "tenant administration context ID is required".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT principal_ref, device_id, session_id, tenant_id,
                    customer_id, root_holding_id, reason, correlation_id,
                    issued_at, expires_at, revoked_at
             FROM erpx_control.platform_tenant_admin_contexts
             WHERE context_id_hash = $1",
        )
        .bind(context_hash(context_id))
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(|row| {
            let context = TenantAdminContext {
                id: context_id,
                principal_ref: row.try_get("principal_ref")?,
                device_id: row.try_get("device_id")?,
                session_id: row.try_get("session_id")?,
                tenant_id: row.try_get("tenant_id")?,
                customer_id: row.try_get("customer_id")?,
                root_holding_id: row.try_get("root_holding_id")?,
                reason: row.try_get("reason")?,
                correlation_id: row.try_get("correlation_id")?,
                issued_at: row.try_get("issued_at")?,
                expires_at: row.try_get("expires_at")?,
                revoked_at: row.try_get("revoked_at")?,
            };
            context
                .validate_shape()
                .map_err(tenant_administration_error)?;
            Ok(context)
        })
        .transpose()
    }

    pub async fn end_context(
        &self,
        context_id: Uuid,
        principal_ref: &str,
        device_id: Uuid,
        session_id: Uuid,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        validate_reference(reason, "revocation_reason", 512)?;
        let Some(context) = self.get_context(context_id).await? else {
            return Ok(false);
        };
        let mut transaction = self.control.pools.write.begin().await?;
        let result = sqlx::query(
            "UPDATE erpx_control.platform_tenant_admin_contexts
             SET revoked_at = $6, revocation_reason = $5
             WHERE context_id_hash = $1
               AND principal_ref = $2
               AND device_id = $3
               AND session_id = $4
               AND revoked_at IS NULL",
        )
        .bind(context_hash(context_id))
        .bind(principal_ref)
        .bind(device_id)
        .bind(session_id)
        .bind(reason)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        let ended = result.rows_affected() == 1;
        if ended {
            insert_access_audit(
                &mut transaction,
                &context,
                "platform",
                "platform.tenant.exit",
                at,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(ended)
    }

    pub async fn record_context_access(
        &self,
        context: &TenantAdminContext,
        module_key: &str,
        action: &str,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_reference(module_key, "module_key", 128)?;
        validate_reference(action, "action", 255)?;
        let mut transaction = self.control.pools.write.begin().await?;
        insert_access_audit(&mut transaction, context, module_key, action, at).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_tenants(
        &self,
        filter: &TenantRegistryFilter,
    ) -> Result<TenantRegistryPage, PersistenceError> {
        validate_registry_filter(filter)?;
        let page_size = if filter.page_size == 0 {
            25
        } else {
            filter.page_size
        };
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT tenant_id, customer_id, root_holding_id, customer_name,
                    tenant_state, configuration_state, subscription_state,
                    plan_key, enabled_modules, purchased_seats, used_seats,
                    company_count, branch_count, renewal_at, overdue,
                    projection_freshness, storage_state, backup_state,
                    integration_health, health, projected_at
             FROM erpx_control.tenant_summary_projection
             WHERE TRUE",
        );
        if let Some(search) = normalized(&filter.search) {
            query
                .push(" AND to_tsvector('simple', customer_name || ' ' || tenant_id) @@ plainto_tsquery('simple', ")
                .push_bind(search)
                .push(")");
        }
        push_equal_filter(&mut query, "tenant_state", &filter.lifecycle);
        push_equal_filter(&mut query, "subscription_state", &filter.billing_state);
        push_equal_filter(&mut query, "plan_key", &filter.plan);
        push_equal_filter(&mut query, "health", &filter.health);
        if let Some(module) = normalized(&filter.module) {
            query
                .push(" AND ")
                .push_bind(module)
                .push(" = ANY(enabled_modules)");
        }
        if let Some(after) = normalized(&filter.after_tenant_id) {
            query.push(" AND tenant_id > ").push_bind(after);
        }
        query
            .push(" ORDER BY tenant_id LIMIT ")
            .push_bind(i64::from(page_size) + 1);
        let rows = query.build().fetch_all(&self.control.pools.write).await?;
        let has_more = rows.len() > page_size as usize;
        let tenants = rows
            .into_iter()
            .take(page_size as usize)
            .map(registry_summary_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_after_tenant_id = has_more
            .then(|| tenants.last().map(|tenant| tenant.tenant_id.clone()))
            .flatten();
        Ok(TenantRegistryPage {
            tenants,
            has_more,
            next_after_tenant_id,
        })
    }

    pub async fn get_tenant_summary(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantRegistrySummary>, PersistenceError> {
        validate_reference(tenant_id, "tenant_id", 128)?;
        let row = sqlx::query(
            "SELECT tenant_id, customer_id, root_holding_id, customer_name,
                    tenant_state, configuration_state, subscription_state,
                    plan_key, enabled_modules, purchased_seats, used_seats,
                    company_count, branch_count, renewal_at, overdue,
                    projection_freshness, storage_state, backup_state,
                    integration_health, health, projected_at
             FROM erpx_control.tenant_summary_projection
             WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(registry_summary_from_row).transpose()
    }

    pub async fn tenant_subscription_binding(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantStripeSubscriptionBinding>, PersistenceError> {
        validate_reference(tenant_id, "tenant_id", 128)?;
        let row = sqlx::query(
            "SELECT customer_id, tenant_id, subscription_id,
                    stripe_customer_ref, stripe_subscription_ref,
                    currency::text AS currency, item_quantities
             FROM erpx_control.stripe_subscription_bindings
             WHERE tenant_id = $1
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(subscription_binding_from_row).transpose()
    }

    pub async fn list_invoices(
        &self,
        tenant_id: &str,
        after_invoice_id: Option<&str>,
        page_size: u32,
    ) -> Result<TenantInvoicePage, PersistenceError> {
        validate_reference(tenant_id, "tenant_id", 128)?;
        if page_size == 0 || page_size > 100 {
            return Err(PersistenceError::InvalidData(
                "tenant invoice page size is outside its bound".into(),
            ));
        }
        if let Some(after) = after_invoice_id {
            validate_reference(after, "after_invoice_id", 255)?;
        }
        let rows = sqlx::query(
            "SELECT invoice_id, state, currency::text AS currency,
                    amount_due_minor, amount_paid_minor, due_at,
                    hosted_invoice_available
             FROM erpx_control.tenant_invoice_summary_projection
             WHERE tenant_id = $1
               AND ($2::text IS NULL OR invoice_id < $2)
             ORDER BY invoice_id DESC
             LIMIT $3",
        )
        .bind(tenant_id)
        .bind(after_invoice_id)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&self.control.pools.write)
        .await?;
        let has_more = rows.len() > page_size as usize;
        let invoices = rows
            .into_iter()
            .take(page_size as usize)
            .map(invoice_summary_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_after_invoice_id = has_more
            .then(|| invoices.last().map(|invoice| invoice.invoice_id.clone()))
            .flatten();
        Ok(TenantInvoicePage {
            invoices,
            has_more,
            next_after_invoice_id,
        })
    }

    /// Grant the only billing override available to platform staff. The
    /// lifecycle and immutable extension evidence are updated atomically;
    /// the database unique constraint enforces one grant per delinquency
    /// cycle even under concurrent requests.
    pub async fn grant_billing_grace_extension(
        &self,
        request: TenantBillingGraceExtensionGrant<'_>,
    ) -> Result<BillingGraceExtension, PersistenceError> {
        validate_reference(request.tenant_id, "tenant_id", 128)?;
        validate_reference(request.granted_by, "granted_by", 255)?;
        validate_reference(request.reason, "reason", 512)?;
        let mut transaction = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT binding.customer_id, binding.subscription_id,
                    lifecycle.payload, lifecycle.version
             FROM erpx_control.stripe_subscription_bindings AS binding
             JOIN erpx_control.billing_lifecycles AS lifecycle
               ON lifecycle.customer_id = binding.customer_id
              AND lifecycle.subscription_id = binding.subscription_id
             WHERE binding.tenant_id = $1
             ORDER BY binding.updated_at DESC
             LIMIT 1
             FOR UPDATE OF lifecycle",
        )
        .bind(request.tenant_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("tenant subscription is missing".into()))?;
        let customer_id: Uuid = row.try_get("customer_id")?;
        let subscription_id: Uuid = row.try_get("subscription_id")?;
        let mut lifecycle: BillingLifecycle = serde_json::from_value(row.try_get("payload")?)?;
        if lifecycle.state != BillingState::Grace {
            return Err(PersistenceError::InvalidData(
                "billing grace can only be extended during the writable grace period".into(),
            ));
        }
        let previous_grace_until = lifecycle.grace_until.ok_or_else(|| {
            PersistenceError::InvalidData("billing lifecycle has no grace deadline".into())
        })?;
        let extension = BillingGraceExtension::grant(BillingGraceExtensionGrant {
            customer_id,
            subscription_id,
            delinquency_cycle_id: request.delinquency_cycle_id,
            previous_grace_until,
            extension: request.extension,
            granted_at: request.granted_at,
            granted_by: request.granted_by.into(),
            reason: request.reason.into(),
            correlation_id: request.correlation_id,
        })
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        lifecycle.grace_until = Some(extension.extended_until);
        let payload = serde_json::to_value(&lifecycle)?;
        sqlx::query(
            "INSERT INTO erpx_control.billing_grace_extensions
                (id, customer_id, subscription_id, delinquency_cycle_id,
                 previous_grace_until, extended_until, granted_at,
                 granted_by, reason, correlation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(extension.id)
        .bind(extension.customer_id)
        .bind(extension.subscription_id)
        .bind(extension.delinquency_cycle_id)
        .bind(extension.previous_grace_until)
        .bind(extension.extended_until)
        .bind(extension.granted_at)
        .bind(&extension.granted_by)
        .bind(&extension.reason)
        .bind(extension.correlation_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.billing_lifecycles
             SET grace_until = $3, payload = $4, version = version + 1,
                 updated_at = now()
             WHERE customer_id = $1 AND subscription_id = $2",
        )
        .bind(customer_id)
        .bind(subscription_id)
        .bind(extension.extended_until)
        .bind(payload)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.tenant_summary_projection
             SET subscription_state = 'grace', overdue = TRUE,
                 projected_at = now()
             WHERE tenant_id = $1",
        )
        .bind(request.tenant_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, payload)
             VALUES ($1, $2, $3, 'commercial.billing.grace_extended',
                     $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(request.granted_by)
        .bind(customer_id)
        .bind(request.reason)
        .bind(request.correlation_id)
        .bind(serde_json::json!({
            "tenant_id": request.tenant_id,
            "subscription_id": subscription_id,
            "delinquency_cycle_id": request.delinquency_cycle_id,
            "extended_until": extension.extended_until,
        }))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(extension)
    }
}

fn context_hash(context_id: Uuid) -> Vec<u8> {
    Sha256::digest(context_id.as_bytes()).to_vec()
}

fn managed_infrastructure_ref(tenant_id: &str, resource: &str) -> String {
    let digest = Sha256::digest(format!("{tenant_id}:{resource}").as_bytes());
    format!("erpx-managed:registry/{}", hex::encode(digest))
}

async fn insert_access_audit(
    transaction: &mut sqlx::Transaction<'_, Postgres>,
    context: &TenantAdminContext,
    module_key: &str,
    action: &str,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_control.tenant_platform_access_audit
            (id, tenant_id, customer_id, root_holding_id,
             platform_actor_ref, module_key, action, reason,
             administrative_context_hash, correlation_id, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(Uuid::new_v4())
    .bind(&context.tenant_id)
    .bind(context.customer_id)
    .bind(context.root_holding_id)
    .bind(&context.principal_ref)
    .bind(module_key)
    .bind(action)
    .bind(&context.reason)
    .bind(context_hash(context.id))
    .bind(context.correlation_id)
    .bind(at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn registry_summary_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<TenantRegistrySummary, PersistenceError> {
    Ok(TenantRegistrySummary {
        tenant_id: row.try_get("tenant_id")?,
        customer_id: row.try_get("customer_id")?,
        root_holding_id: row.try_get("root_holding_id")?,
        customer_name: row.try_get("customer_name")?,
        tenant_state: row.try_get("tenant_state")?,
        configuration_state: row.try_get("configuration_state")?,
        subscription_state: row.try_get("subscription_state")?,
        plan_key: row.try_get("plan_key")?,
        enabled_modules: row.try_get("enabled_modules")?,
        purchased_seats: nonnegative_u32(row.try_get("purchased_seats")?, "purchased seats")?,
        used_seats: nonnegative_u32(row.try_get("used_seats")?, "used seats")?,
        company_count: nonnegative_u32(row.try_get("company_count")?, "company count")?,
        branch_count: nonnegative_u32(row.try_get("branch_count")?, "branch count")?,
        renewal_at: row.try_get("renewal_at")?,
        overdue: row.try_get("overdue")?,
        projection_freshness: row.try_get("projection_freshness")?,
        storage_state: row.try_get("storage_state")?,
        backup_state: row.try_get("backup_state")?,
        integration_health: row.try_get("integration_health")?,
        health: row.try_get("health")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn subscription_binding_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<TenantStripeSubscriptionBinding, PersistenceError> {
    let item_quantities = serde_json::from_value(row.try_get("item_quantities")?)?;
    Ok(TenantStripeSubscriptionBinding {
        customer_id: row.try_get("customer_id")?,
        tenant_id: row.try_get("tenant_id")?,
        subscription_id: row.try_get("subscription_id")?,
        stripe_customer_ref: row.try_get("stripe_customer_ref")?,
        stripe_subscription_ref: row.try_get("stripe_subscription_ref")?,
        currency: row.try_get::<String, _>("currency")?.trim().to_owned(),
        item_quantities,
    })
}

fn invoice_summary_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<TenantInvoiceSummary, PersistenceError> {
    Ok(TenantInvoiceSummary {
        invoice_id: row.try_get("invoice_id")?,
        state: row.try_get("state")?,
        currency: row.try_get::<String, _>("currency")?.trim().to_owned(),
        amount_due_minor: u64::try_from(row.try_get::<i64, _>("amount_due_minor")?)
            .map_err(|_| PersistenceError::InvalidData("negative invoice amount due".into()))?,
        amount_paid_minor: u64::try_from(row.try_get::<i64, _>("amount_paid_minor")?)
            .map_err(|_| PersistenceError::InvalidData("negative invoice amount paid".into()))?,
        due_at: row.try_get("due_at")?,
        hosted_invoice_available: row.try_get("hosted_invoice_available")?,
    })
}

fn nonnegative_u32(value: i32, name: &str) -> Result<u32, PersistenceError> {
    u32::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("negative tenant summary {name}")))
}

fn push_equal_filter<'a>(
    query: &mut QueryBuilder<'a, Postgres>,
    column: &str,
    value: &'a Option<String>,
) {
    if let Some(value) = normalized(value) {
        query
            .push(" AND ")
            .push(column)
            .push(" = ")
            .push_bind(value);
    }
}

fn normalized(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn validate_registry_filter(filter: &TenantRegistryFilter) -> Result<(), PersistenceError> {
    if filter.page_size > MAX_TENANT_SUMMARY_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(
            "tenant registry page size exceeds its bound".into(),
        ));
    }
    for (value, name, maximum) in [
        (&filter.search, "search", 255),
        (&filter.lifecycle, "lifecycle", 64),
        (&filter.billing_state, "billing_state", 64),
        (&filter.plan, "plan", 128),
        (&filter.module, "module", 128),
        (&filter.health, "health", 64),
        (&filter.after_tenant_id, "after_tenant_id", 128),
    ] {
        if let Some(value) = normalized(value) {
            validate_reference(value, name, maximum)?;
        }
    }
    Ok(())
}

fn validate_reference(value: &str, name: &str, maximum: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(PersistenceError::InvalidData(format!(
            "tenant administration {name} is invalid"
        )))
    } else {
        Ok(())
    }
}

fn tenant_administration_error(error: TenantAdministrationError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_filters_and_page_size_are_bounded() {
        assert!(
            validate_registry_filter(&TenantRegistryFilter {
                search: Some("acme".into()),
                page_size: MAX_TENANT_SUMMARY_PAGE_SIZE,
                ..TenantRegistryFilter::default()
            })
            .is_ok()
        );
        assert!(
            validate_registry_filter(&TenantRegistryFilter {
                page_size: MAX_TENANT_SUMMARY_PAGE_SIZE + 1,
                ..TenantRegistryFilter::default()
            })
            .is_err()
        );
        assert!(
            validate_registry_filter(&TenantRegistryFilter {
                module: Some("finance\nforged".into()),
                ..TenantRegistryFilter::default()
            })
            .is_err()
        );
    }

    #[test]
    fn administrative_context_digest_is_stable_and_not_the_bearer() {
        let id = Uuid::new_v4();
        let digest = context_hash(id);
        assert_eq!(digest.len(), 32);
        assert_eq!(digest, context_hash(id));
        assert_ne!(digest, id.as_bytes());
    }

    #[test]
    fn managed_infrastructure_references_are_deterministic_and_opaque() {
        let reference = managed_infrastructure_ref("tenant-acme", "write");
        assert!(reference.starts_with("erpx-managed:registry/"));
        assert!(!reference.contains("tenant-acme"));
        assert_eq!(
            reference,
            managed_infrastructure_ref("tenant-acme", "write")
        );
        assert_ne!(reference, managed_infrastructure_ref("tenant-acme", "read"));
    }
}
