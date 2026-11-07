//! Current Holding Account proof lent to Control owners. It carries no Company,
//! role or business permission; preference values cannot mint any such authority.
use super::{
    account_holding_bindings::{ConfiguredHoldingRegistry, CurrentHoldingBinding},
    account_sessions::verify_session_in_transaction,
};
use crate::{
    application::account_ports::DeviceProof,
    domain::account_auth::{AccountPolicy, AccountSession, RequestAudience},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId, UserId};
use sqlx::{Connection, PgConnection};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ACCOUNT_CONTROL_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "SESSION_REVOKED",
        "Sign in again to continue.",
    )
    .into()
}
pub struct HoldingAccountControlScope {
    unit: Unit,
    session: AccountSession,
    user_id: UserId,
    audience: RequestAudience,
    proof: DeviceProof,
    policy: AccountPolicy,
    registry: Arc<ConfiguredHoldingRegistry>,
    binding: CurrentHoldingBinding,
    holding_slug: String,
    holding_name: String,
}
impl HoldingAccountControlScope {
    pub fn session(&self) -> &AccountSession {
        &self.session
    }
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
    pub fn holding_id(&self) -> &HoldingId {
        self.binding.holding_id()
    }
    pub fn holding_slug(&self) -> &str {
        &self.holding_slug
    }
    pub fn holding_name(&self) -> &str {
        &self.holding_name
    }
    pub async fn require_current(&self, c: &mut PgConnection, now: i64) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let current = verify_session_in_transaction(
            c,
            &self.proof,
            &self.audience,
            &self.policy.environment,
            now,
            &self.registry,
        )
        .await?
        .ok_or_else(changed)?;
        if current != self.session {
            return Err(changed());
        }
        self.binding.require_current(c).await?;
        let name:String=sqlx::query_scalar("SELECT display_name FROM holding_bindings WHERE holding_id=$1").bind(self.holding_id().as_str()).fetch_one(&mut *c).await?;
        if name!=self.holding_name{return Err(changed());}
        Ok(())
    }
}
pub async fn verify_holding_account_control(
    c: &mut PgConnection,
    host: &str,
    proof: DeviceProof,
    policy: AccountPolicy,
    registry: Arc<ConfiguredHoldingRegistry>,
    now: i64,
) -> Result<HoldingAccountControlScope, Error> {
    policy.validate()?;
    let audience = policy
        .audience(host)
        .filter(|a| matches!(a, RequestAudience::Holding { .. }))
        .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
    let configured = registry.expected_for_audience(&audience)?;
    let holding_slug = configured.slug.clone();
    let session =
        verify_session_in_transaction(c, &proof, &audience, &policy.environment, now, &registry)
            .await?
            .ok_or_else(changed)?;
    let binding = registry.read_current(c, &audience).await?;
    let holding_name:Option<String>=sqlx::query_scalar("SELECT CASE WHEN octet_length(display_name)<=8192 THEN display_name END FROM holding_bindings WHERE holding_id=$1 FOR SHARE").bind(binding.holding_id().as_str()).fetch_one(&mut *c).await?;
    let holding_name = holding_name.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_BINDING_UNAVAILABLE",
            "The Holding presentation is unavailable.",
        )
    })?;
    let scope = HoldingAccountControlScope {
        unit: unit(c).await?,
        user_id: UserId::new(session.user.id.clone())?,
        session,
        audience,
        proof,
        policy,
        registry,
        binding,
        holding_slug,
        holding_name,
    };
    scope.require_current(c, now).await?;
    Ok(scope)
}
