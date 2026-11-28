//! Customer Authority's binding-version effect for a recorded Portal support request.
//! The caller owns the serializable Holding unit, verified Control session,
//! purchase authority and Files binding; this port neither supplies nor commits them.
use crate::domain::portal_session::PortalCustomerProof;
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_PORTAL_BINDING_UNAVAILABLE",
        "The original customer binding or document changed before commit.",
    )
    .into()
}

/// Captured from the real binding, order and identity before the action insert.
#[derive(Debug)]
pub struct PortalEvidenceWitness {
    binding_id: String,
    company_id: String,
    order_id: String,
    customer_id: String,
    user_id: String,
    email: String,
    version: i64,
    order_version: i64,
    order_state: String,
    command_now: i64,
    transaction_id: String,
}

/// Read and lock the actual source before preparing Files evidence.
pub async fn prepare_portal_evidence_change(
    c: &mut PgConnection,
    proof: &PortalCustomerProof,
    binding_id: &str,
    expected_version: i64,
    now: i64,
) -> Result<PortalEvidenceWitness, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_SERIALIZABLE_REQUIRED").into());
    }
    let row: Option<(String,String,String,i64,String)> = sqlx::query_as("SELECT b.company_id,b.order_id,b.customer_id,o.version,o.state FROM customer_access_bindings b JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=$2 AND i.email=$3 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id WHERE b.id=$1 AND b.email=$3 AND b.status='approved' AND b.expires_at>$4 AND b.version=$5 AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') FOR UPDATE OF b,o")
        .bind(binding_id).bind(&proof.user_id).bind(proof.normalized_email()).bind(now).bind(expected_version).fetch_optional(&mut *c).await?;
    let (company_id, order_id, customer_id, order_version, order_state) =
        row.ok_or_else(changed)?;
    let transaction_id: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
        .fetch_one(&mut *c)
        .await?;
    Ok(PortalEvidenceWitness {
        binding_id: binding_id.into(),
        company_id,
        order_id,
        customer_id,
        user_id: proof.user_id.clone(),
        email: proof.normalized_email(),
        version: expected_version,
        order_version,
        order_state,
        command_now: now,
        transaction_id,
    })
}

/// Apply the same version and event as `prepareCustomerPortalEvidenceChange`.
/// `proof` must originate from the current verified Control session and
/// enrollment; this port additionally checks its identity against the actual
/// Holding binding, source order and already-recorded support action.
pub async fn record_portal_evidence_change(
    c: &mut PgConnection,
    witness: &PortalEvidenceWitness,
    action_id: &str,
    key: &str,
    now: i64,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_SERIALIZABLE_REQUIRED").into());
    }
    let transaction_id: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
        .fetch_one(&mut *c)
        .await?;
    if transaction_id != witness.transaction_id {
        return Err(changed());
    }
    let binding_id = witness.binding_id.as_str();
    let expected_version = witness.version;
    if expected_version < 1
        || now < 0
        || key.len() < 8
        || key.len() > 180
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b":_.-".contains(&b))
    {
        return Err(changed());
    }
    let row: Option<(String,)> = sqlx::query_as("SELECT b.company_id FROM customer_access_bindings b JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=$2 AND i.email=$3 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id JOIN customer_portal_actions a ON a.id=$4 AND a.binding_id=b.id AND a.company_id=b.company_id AND a.order_id=b.order_id AND a.customer_id=b.customer_id AND a.user_id=i.user_id AND a.action='request_support' AND a.request_key=$5 AND a.source_version=o.version WHERE b.id=$1 AND b.email=$3 AND b.status='approved' AND b.expires_at>$6 AND b.version=$7 AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') FOR UPDATE OF b,o")
        .bind(binding_id).bind(&witness.user_id).bind(&witness.email).bind(action_id).bind(key).bind(now).bind(expected_version).fetch_optional(&mut *c).await?;
    let (company,) = row.ok_or_else(changed)?;
    if company != witness.company_id {
        return Err(changed());
    }
    let exact: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id WHERE b.id=$1 AND b.order_id=$2 AND b.customer_id=$3 AND o.version=$4 AND o.state=$5)")
        .bind(binding_id).bind(&witness.order_id).bind(&witness.customer_id).bind(witness.order_version).bind(&witness.order_state).fetch_one(&mut *c).await?;
    if !exact {
        return Err(changed());
    }
    let version = expected_version.checked_add(1).ok_or_else(changed)?;
    let affected = sqlx::query("UPDATE customer_access_bindings SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
        .bind(witness.command_now).bind(binding_id).bind(&company).bind(expected_version).execute(&mut *c).await?.rows_affected();
    if affected != 1 {
        return Err(changed());
    }
    sqlx::query("INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) VALUES($1,$2,$3,'portal-evidence-bind',$4,'Customer attached clean evidence to the support request',$5)")
        .bind(format!("customer-access-event-{key}")).bind(binding_id).bind(version).bind(&witness.user_id).bind(witness.command_now).execute(&mut *c).await?;
    Ok(())
}
