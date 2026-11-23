//! Native Field projection sources for the final-root owner joins.
//!
//! These fixed, owner-authored statements replace the five SQL fragments that
//! `final-root-owner-projection-sources.ts` exported to Customer 360 and
//! customer-authority consumers. A caller never supplies SQL or a relation
//! name: it selects a closed `FinalRootProjectionSource` and composes the
//! returned statement inside its own transaction. A `Current...` witness binds
//! the fragments to one serializable, fenced snapshot so a late effect cannot
//! silently change the source of a composed read.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const AGREEMENT_EVENTS_SQL: &str = r#"SELECT "id","agreement_id","version","action","actor_id","reason","status","created_at" FROM field_service_agreement_events"#;
const AGREEMENTS_SQL: &str = r#"SELECT "id","company_id","branch_id","department_id","customer_id","asset_id","title","start_date","end_date","currency","visit_rate_minor","status","created_by","reviewed_by","reviewed_at","version","created_at","updated_at","warranty_discount_bps" FROM field_service_agreements"#;
const BILLING_RELEASES_SQL: &str = r#"SELECT "id","order_id","agreement_id","customer_id","currency","amount_minor","source_order_version","status","released_by","reason","created_at","warranty_applied","warranty_discount_bps","warranty_end_snapshot","pricing_date" FROM field_service_billing_releases"#;
const ORDER_EVENTS_SQL: &str = r#"SELECT "id","order_id","version","action","actor_id","reason","status","technician_id","scheduled_at","details_json","created_at" FROM field_service_order_events"#;
const ORDERS_SQL: &str = r#"SELECT "id","company_id","branch_id","department_id","ticket_id","ticket_version","agreement_id","agreement_version","asset_id","issue","site","technician_id","scheduled_at","status","exception_reason","visit_started_at","visit_completed_at","labor_minutes","completion_evidence","customer_confirmation","version","created_by","created_at","updated_at" FROM field_service_orders"#;

/// The closed set of final-root projection sources Field serves to other owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalRootProjectionSource {
    AgreementEvents,
    Agreements,
    BillingReleases,
    OrderEvents,
    Orders,
}

impl FinalRootProjectionSource {
    pub const ALL: [Self; 5] = [
        Self::AgreementEvents,
        Self::Agreements,
        Self::BillingReleases,
        Self::OrderEvents,
        Self::Orders,
    ];

    /// Fixed SQL only; the returned statement is never derived from input.
    pub fn sql(self) -> &'static str {
        match self {
            Self::AgreementEvents => AGREEMENT_EVENTS_SQL,
            Self::Agreements => AGREEMENTS_SQL,
            Self::BillingReleases => BILLING_RELEASES_SQL,
            Self::OrderEvents => ORDER_EVENTS_SQL,
            Self::Orders => ORDERS_SQL,
        }
    }

    /// Fully qualified relation this fragment reads, used for the fence witness.
    pub fn relation(self) -> &'static str {
        match self {
            Self::AgreementEvents => "public.field_service_agreement_events",
            Self::Agreements => "public.field_service_agreements",
            Self::BillingReleases => "public.field_service_billing_releases",
            Self::OrderEvents => "public.field_service_order_events",
            Self::Orders => "public.field_service_orders",
        }
    }
}

type SourceUnit = ((String, i32, String, String, String), String, Vec<i64>);

/// Utility fences precede the first protected snapshot. The coordinator acquires
/// every other owner's sources in its own reviewed order; these locks are not a
/// source or access grant and a conflict retries the whole unit.
pub async fn lock_final_root_projection_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FIELD_ROOT_PROJECTION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.field_service_agreement_events,public.field_service_agreements,public.field_service_billing_releases,public.field_service_order_events,public.field_service_orders IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<SourceUnit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FIELD_ROOT_PROJECTION_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible, path): (bool, bool, String) = sqlx::query_as("SELECT (SELECT count(*)=5 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class c WHERE c.oid IN ('public.field_service_agreement_events'::regclass,'public.field_service_agreements'::regclass,'public.field_service_billing_releases'::regclass,'public.field_service_order_events'::regclass,'public.field_service_orders'::regclass)), NOT (row_security_active('public.field_service_agreement_events'::regclass) OR row_security_active('public.field_service_agreements'::regclass) OR row_security_active('public.field_service_billing_releases'::regclass) OR row_security_active('public.field_service_order_events'::regclass) OR row_security_active('public.field_service_orders'::regclass)), current_setting('search_path')")
        .fetch_one(&mut *c)
        .await?;
    if !held {
        return Err(PersistenceError::new("FIELD_ROOT_PROJECTION_LOCKS_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("FIELD_ROOT_PROJECTION_RLS_UNSUPPORTED").into());
    }
    let oids: Vec<i64> = sqlx::query_scalar("SELECT unnest(ARRAY['public.field_service_agreement_events'::regclass::oid,'public.field_service_agreements'::regclass::oid,'public.field_service_billing_releases'::regclass::oid,'public.field_service_order_events'::regclass::oid,'public.field_service_orders'::regclass::oid])::bigint")
        .fetch_all(&mut *c)
        .await?;
    let transaction =
        sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text")
            .fetch_one(c)
            .await?;
    Ok((transaction, path, oids))
}

/// A witness that the fixed source statements were inspected under one fenced
/// snapshot. The consumer retains its scoped result and revalidates after all
/// late effects before using any composed row.
pub struct CurrentFinalRootProjectionSources {
    unit: SourceUnit,
}

impl CurrentFinalRootProjectionSources {
    /// Returns only one of the five fixed statements; no caller SQL is accepted.
    pub fn sql(&self, source: FinalRootProjectionSource) -> &'static str {
        source.sql()
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_ROOT_PROJECTION_SOURCE_CHANGED",
                "The Field projection source changed.",
            )
            .into());
        }
        Ok(())
    }
}

pub async fn inspect_final_root_projection_sources(
    c: &mut PgConnection,
) -> Result<CurrentFinalRootProjectionSources, Error> {
    Ok(CurrentFinalRootProjectionSources {
        unit: unit(c).await?,
    })
}
