//! Portal grant-fence identity, not purchase or actor authority.
//!
//! Composition must take the complete TS lock chain before the first snapshot
//! and prevent releasing/reacquiring locks. PostgreSQL cannot attest either
//! condition afterward. This witness never locks or excludes history writers.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);

pub struct PortalPurchaseFence {
    unit: Unit,
}
impl PortalPurchaseFence {
    /// Bind only after the trusted coordinator has acquired all source locks.
    pub async fn bind(connection: &mut PgConnection) -> Result<Self, Error> {
        Ok(Self {
            unit: inspect(connection).await?,
        })
    }
    pub async fn require_current(&self, connection: &mut PgConnection) -> Result<(), Error> {
        if inspect(connection).await? != self.unit {
            return Err(PersistenceError::new("COMMERCIAL_PORTAL_FENCE_CHANGED").into());
        }
        Ok(())
    }
}
async fn inspect(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCIAL_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible): (bool, bool) = sqlx::query_as("SELECT to_regclass('company_package_grants')='public.company_package_grants'::regclass AND EXISTS(SELECT 1 FROM pg_class p JOIN pg_locks l ON l.relation=p.oid WHERE p.oid='public.company_package_grants'::regclass AND p.relnamespace='public'::regnamespace AND p.relkind IN ('r','p') AND p.relpersistence='p' AND l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted AND l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')), NOT row_security_active('public.company_package_grants'::regclass) AND NOT row_security_active('public.company_package_history_rights'::regclass)").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("COMMERCIAL_PORTAL_GRANT_FENCE_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("COMMERCIAL_PORTAL_RLS_UNSUPPORTED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path'),'public.company_package_grants'::regclass::oid::bigint").fetch_one(c).await?)
}

use super::purchased_capability_sql::{js_number_grants, js_number_history};
use crate::domain::packages::{
    CapabilityPurpose, CommercialCatalogue, CompanyPackageGrant, CompanyPackageHistoryRight,
    PackageDecision, PackageEnforcementMode,
};
use crate::domain::purchase_wire_revision::purchase_wire_revision;
use erpx_server_shared::public::{Clock, CompanyId, FailureKind};
use serde::Serialize;
use sqlx::Row;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalPurchaseShadow {
    event: &'static str,
    phase: &'static str,
    company_id: String,
    capability: String,
    purpose: CapabilityPurpose,
    code: &'static str,
    purchase_revision: String,
}
pub struct PortalPurchasePreDecision {
    pub decision: PackageDecision,
    pub shadow: Option<PortalPurchaseShadow>,
    mode: PackageEnforcementMode,
}
impl PortalPurchasePreDecision {
    pub fn require_allowed(&self) -> Result<(), BusinessError> {
        if self.mode == PackageEnforcementMode::Enforced && !self.decision.allowed {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                self.decision.code.as_code(),
                "This package does not permit the requested work.",
            ));
        }
        Ok(())
    }
}
pub fn write_portal_purchase_shadow(
    writer: &mut impl std::io::Write,
    observation: &PortalPurchaseShadow,
) {
    if serde_json::to_writer(&mut *writer, observation).is_ok() {
        let _ = writer.write_all(b"\n");
    }
}
/// Caller supplies owner-derived scope and a fence bound after the complete
/// pre-snapshot chain. Disabled performs no SQL, witness check, or clock read.
pub async fn inspect_portal_purchase_pre(
    c: &mut PgConnection,
    fence: Option<&PortalPurchaseFence>,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
    mode: PackageEnforcementMode,
    clock: &(dyn Clock + Sync),
) -> Result<Option<PortalPurchasePreDecision>, Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let fence =
        fence.ok_or_else(|| PersistenceError::new("COMMERCIAL_PORTAL_GRANT_FENCE_REQUIRED"))?;
    fence.require_current(&mut *c).await?;
    let row = sqlx::query("SELECT COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=$1),'[]'::jsonb) grants, COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=$1),'[]'::jsonb) history").bind(company.as_str()).fetch_one(&mut *c).await?;
    let grants: serde_json::Value = row.try_get("grants")?;
    let history: serde_json::Value = row.try_get("history")?;
    let parse = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| PersistenceError::new("COMMERCIAL_PURCHASE_FACTS_INVALID"))
    };
    let num = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(|v| v.as_i64())
            .ok_or_else(|| PersistenceError::new("COMMERCIAL_PURCHASE_FACTS_INVALID"))
    };
    let grants: Vec<CompanyPackageGrant> = grants
        .as_array()
        .ok_or_else(|| PersistenceError::new("COMMERCIAL_PURCHASE_FACTS_INVALID"))?
        .iter()
        .map(|g| {
            Ok(CompanyPackageGrant {
                company_id: parse(g, "company_id")?,
                package_id: parse(g, "package_id")?,
                package_version: num(g, "package_version")?,
                state: parse(g, "state")?,
                starts_at: num(g, "starts_at")?,
                ends_at: if g.get("ends_at").is_some_and(|v| v.is_null()) {
                    None
                } else {
                    Some(num(g, "ends_at")?)
                },
                version: num(g, "version")?,
                reason: parse(g, "reason")?,
                evidence: parse(g, "evidence")?,
            })
        })
        .collect::<Result<_, PersistenceError>>()?;
    let history: Vec<CompanyPackageHistoryRight> = history
        .as_array()
        .ok_or_else(|| PersistenceError::new("COMMERCIAL_PURCHASE_FACTS_INVALID"))?
        .iter()
        .map(|h| {
            Ok(CompanyPackageHistoryRight {
                company_id: parse(h, "company_id")?,
                package_id: parse(h, "package_id")?,
                package_version: num(h, "package_version")?,
                source_grant_version: num(h, "source_grant_version")?,
                source_event_id: parse(h, "source_event_id")?,
                retained_at: num(h, "retained_at")?,
            })
        })
        .collect::<Result<_, PersistenceError>>()?;
    let now = clock.now();
    fence.require_current(&mut *c).await?;
    let mut decision_grants = grants.clone();
    let mut decision_history = history.clone();
    js_number_grants(&mut decision_grants);
    js_number_history(&mut decision_history);
    let decision = catalogue.purchased_capability(
        &decision_grants,
        company.as_str(),
        capability,
        purpose,
        (now.get() as f64) as i64,
        &decision_history,
    );
    let shadow = if mode == PackageEnforcementMode::Shadow {
        purchase_wire_revision(company.as_str(), &grants, &history, now)
            .ok()
            .map(|purchase_revision| PortalPurchaseShadow {
                event: "commercial.purchase.shadow",
                phase: "read",
                company_id: company.as_str().into(),
                capability: capability.into(),
                purpose,
                code: decision.code.as_code(),
                purchase_revision,
            })
    } else {
        None
    };
    Ok(Some(PortalPurchasePreDecision {
        decision,
        shadow,
        mode,
    }))
}

use super::purchase_command_boundary::purchase_command_failure;
use super::purchased_capability_sql::{
    PurchaseShadowCheckedObservation, decision_and_observation, push_capability_predicate,
    push_shadow_facts,
};
use sqlx::{Postgres, QueryBuilder};

/// Post-effects purchase guard and transaction-local cleanup only. The caller
/// must run source/release checks in the TS order and emit returned shadow
/// telemetry best-effort. This does not attest those upstream checks.
pub async fn finish_portal_purchase(
    c: &mut PgConnection,
    fence: Option<&PortalPurchaseFence>,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
    mode: PackageEnforcementMode,
    key: &str,
) -> Result<Option<PurchaseShadowCheckedObservation>, Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let fence =
        fence.ok_or_else(|| PersistenceError::new("COMMERCIAL_PORTAL_GRANT_FENCE_REQUIRED"))?;
    fence.require_current(&mut *c).await?;
    let mut q = QueryBuilder::<Postgres>::new("WITH purchase_scope AS MATERIALIZED (SELECT ");
    q.push_bind(company.as_str().to_owned()).push(
        "::text company_id, floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) ",
    );
    let observation = if mode == PackageEnforcementMode::Enforced {
        q.push("INSERT INTO command_assertions(command_id,step,ok) SELECT ")
            .push_bind(key.to_owned())
            .push("::text,'commercial-purchase-after',CAST(CASE WHEN ");
        push_capability_predicate(&mut q, catalogue, capability, purpose);
        q.push(" THEN 1 ELSE 0 END AS bigint) FROM purchase_scope s");
        q.build().execute(&mut *c).await.map_err(|error| {
            match purchase_command_failure(&error) {
                Some(business) => Error::Business(business),
                None => error.into(),
            }
        })?;
        None
    } else {
        q.push("SELECT s.now_ms, (");
        push_capability_predicate(&mut q, catalogue, capability, purpose);
        q.push(") allowed");
        push_shadow_facts(&mut q);
        q.push(" FROM purchase_scope s");
        let row = q.build().fetch_one(&mut *c).await?;
        let now: i64 = row.try_get("now_ms")?;
        row.try_get::<serde_json::Value, _>("grants")
            .ok()
            .zip(row.try_get::<serde_json::Value, _>("history").ok())
            .and_then(|(g, h)| {
                decision_and_observation(
                    company.as_str(),
                    capability,
                    purpose,
                    now,
                    catalogue,
                    g,
                    h,
                )
                .map(|(_, o)| o)
            })
    };
    sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
        .bind(key)
        .execute(&mut *c)
        .await?;
    fence.require_current(c).await?;
    Ok(observation)
}

/// TS Portal purchase boundary before effects. Caller already performed release
/// and pre-decision, and holds the complete pre-snapshot owner lock chain.
/// This does not establish Company, actor, release, or session authority.
pub async fn begin_portal_purchase(
    c: &mut PgConnection,
    fence: Option<&PortalPurchaseFence>,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
    mode: PackageEnforcementMode,
    key: &str,
) -> Result<(), Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(());
    }
    let fence =
        fence.ok_or_else(|| PersistenceError::new("COMMERCIAL_PORTAL_GRANT_FENCE_REQUIRED"))?;
    fence.require_current(&mut *c).await?;
    super::purchase_command_boundary::lock_company_purchase_read_sources(&mut *c, company).await?;
    if mode == PackageEnforcementMode::Enforced {
        let mut q = QueryBuilder::<Postgres>::new("WITH purchase_scope AS MATERIALIZED (SELECT ");
        q.push_bind(company.as_str().to_owned()).push("::text company_id, floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) INSERT INTO command_assertions(command_id,step,ok) SELECT ").push_bind(key.to_owned()).push("::text,'commercial-purchase-before',CAST(CASE WHEN ");
        push_capability_predicate(&mut q, catalogue, capability, purpose);
        q.push(" THEN 1 ELSE 0 END AS bigint) FROM purchase_scope s");
        q.build().execute(&mut *c).await.map_err(|error| {
            match purchase_command_failure(&error) {
                Some(business) => Error::Business(business),
                None => error.into(),
            }
        })?;
    }
    fence.require_current(c).await?;
    Ok(())
}
