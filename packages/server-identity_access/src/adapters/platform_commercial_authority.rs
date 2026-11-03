//! Current Platform Commercial permission retained in the native Holding
//! Account Control unit. No Admin host or business Work actor is invented.
//! Composition retains this Control unit through the Holding's final checks.
use super::account_control_scope::{HoldingAccountControlScope, verify_holding_account_control};
use super::account_holding_bindings::ConfiguredHoldingRegistry;
use crate::{application::account_ports::DeviceProof, domain::account_auth::AccountPolicy};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection, Row};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;

/// Selected request identity only; construction grants no rights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformCommercialSelection {
    pub user_id: String,
    pub session_id: String,
    pub assignment_id: String,
    pub role: String,
    pub role_revision: i64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlatformCommercialOperation {
    Read,
    Apply,
}
pub struct CurrentPlatformCommercialAuthority {
    account: HoldingAccountControlScope,
    selected: PlatformCommercialSelection,
    operation: PlatformCommercialOperation,
    source: String,
    can_manage: bool,
}
impl CurrentPlatformCommercialAuthority {
    pub fn selected(&self) -> &PlatformCommercialSelection {
        &self.selected
    }
    pub fn can_manage(&self) -> bool {
        self.can_manage
    }
    pub fn operation(&self) -> PlatformCommercialOperation {
        self.operation
    }
    pub async fn require_current(&self, c: &mut PgConnection, now: i64) -> Result<(), Error> {
        self.account.require_current(c, now).await?;
        let (source, can_manage) = inspect(c, &self.selected, self.operation, now).await?;
        if source != self.source || can_manage != self.can_manage {
            return Err(denied(
                FailureKind::Conflict,
                "PLATFORM_AUTHORITY_CHANGED",
                "Your Platform authority changed. Review again.",
            )
            .into());
        }
        Ok(())
    }
}
fn denied(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
fn authority() -> BusinessError {
    denied(
        FailureKind::Forbidden,
        "PLATFORM_AUTHORITY_REQUIRED",
        "Your selected Platform assignment is unavailable.",
    )
}

/// The provider-entry Holding is exact composition configuration, never a
/// request body. READ COMMITTED plus retained source locks prevents borrowing a
/// pre-lock SERIALIZABLE snapshot for current Control authority.
pub async fn verify_platform_commercial_authority(
    c: &mut PgConnection,
    host: &str,
    proof: DeviceProof,
    policy: AccountPolicy,
    registry: Arc<ConfiguredHoldingRegistry>,
    provider_holding: &HoldingId,
    selected: PlatformCommercialSelection,
    operation: PlatformCommercialOperation,
    now: i64,
) -> Result<CurrentPlatformCommercialAuthority, Error> {
    prepare_sources(c).await?;
    if [
        &selected.user_id,
        &selected.session_id,
        &selected.assignment_id,
        &selected.role,
    ]
    .iter()
    .any(|s| s.is_empty() || s.len() > 128)
        || !(1..=9_007_199_254_740_991).contains(&selected.role_revision)
    {
        return Err(authority().into());
    }
    let account = verify_holding_account_control(c, host, proof, policy, registry, now).await?;
    if account.holding_id() != provider_holding
        || account.user_id().as_str() != selected.user_id
        || account.session().id != selected.session_id
    {
        return Err(authority().into());
    }
    let (source, can_manage) = inspect(c, &selected, operation, now).await?;
    Ok(CurrentPlatformCommercialAuthority {
        account,
        selected,
        operation,
        source,
        can_manage,
    })
}
async fn prepare_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "read committed"
    {
        return Err(PersistenceError::new("PLATFORM_CURRENT_CONTROL_UNIT_REQUIRED").into());
    }
    // Same order as context/family changes; no upgrade after Account reads.
    sqlx::query("LOCK TABLE session IN EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE account_workspace_preferences,admin_provider_contexts,platform_assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,iam_mfa,iam_mfa_sessions,sso_session_bindings IN SHARE MODE").execute(&mut *c).await?;
    Ok(())
}
/// Resolve the normal per-session provider selection, then the existing
/// assignment cookie hint / saved preference / first current assignment. Hints
/// select an owned current assignment and never supply principal or permission.
pub async fn verify_current_platform_commercial_authority(
    c: &mut PgConnection,
    host: &str,
    proof: DeviceProof,
    policy: AccountPolicy,
    registry: Arc<ConfiguredHoldingRegistry>,
    provider_holding: &HoldingId,
    assignment_hint: Option<&str>,
    operation: PlatformCommercialOperation,
    now: i64,
) -> Result<CurrentPlatformCommercialAuthority, Error> {
    prepare_sources(c).await?;
    let account = verify_holding_account_control(c, host, proof, policy, registry, now).await?;
    if account.holding_id() != provider_holding {
        return Err(authority().into());
    }
    let (selected,saved):(Option<String>,Option<String>)=sqlx::query_as("SELECT (SELECT assignment_id FROM admin_provider_contexts WHERE session_id=$1 AND user_id=$2),(SELECT platform_assignment_id FROM account_workspace_preferences WHERE user_id=$2)")
        .bind(&account.session().id).bind(account.user_id().as_str()).fetch_one(&mut *c).await?;
    let requested = selected.as_deref().or(assignment_hint).or(saved.as_deref());
    let rows=sqlx::query("SELECT CASE WHEN octet_length(a.id)<=128 THEN a.id END id,CASE WHEN octet_length(a.role)<=128 THEN a.role END role,r.published_revision::bigint revision FROM platform_assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN('*','platform') AND r.status='active' JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE a.user_id=$1 AND iam_assignment_active(a.id,$2) AND v.scopes_json ? 'platform' ORDER BY a.valid_until,a.id LIMIT 1001")
        .bind(account.user_id().as_str()).bind(now).fetch_all(&mut *c).await?;
    if rows.is_empty() || rows.len() > 1000 {
        return Err(authority().into());
    }
    let mut choices = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        let id = row
            .try_get::<Option<String>, _>("id")?
            .ok_or_else(authority)?;
        if !ids.insert(id.clone()) {
            return Err(authority().into());
        }
        choices.push(PlatformCommercialSelection {
            user_id: account.user_id().as_str().into(),
            session_id: account.session().id.clone(),
            assignment_id: id,
            role: row
                .try_get::<Option<String>, _>("role")?
                .ok_or_else(authority)?,
            role_revision: row.try_get("revision")?,
        });
    }
    let choice = requested.and_then(|id| choices.iter().position(|a| a.assignment_id == id));
    // A saved/cookie preference may be stale; the original session provider
    // context is authoritative and must not silently fall back after revocation.
    if selected.is_some() && choice.is_none() {
        return Err(denied(
            FailureKind::Conflict,
            "PROVIDER_CONTEXT_CHANGED",
            "Your selected Platform context changed. Reload and review before continuing.",
        )
        .into());
    }
    let selected = choices.remove(choice.unwrap_or(0));
    let (source, can_manage) = inspect(c, &selected, operation, now).await?;
    Ok(CurrentPlatformCommercialAuthority {
        account,
        selected,
        operation,
        source,
        can_manage,
    })
}
async fn inspect(
    c: &mut PgConnection,
    a: &PlatformCommercialSelection,
    operation: PlatformCommercialOperation,
    now: i64,
) -> Result<(String, bool), Error> {
    if !(0..=9_007_199_254_740_991).contains(&now) {
        return Err(authority().into());
    }
    let rows=sqlx::query(r#"SELECT a.role,iam_assignment_active(a.id,$4) assignment_active,r.published_revision::bigint role_revision,
      CASE WHEN octet_length(v.permissions_json::text)<=1048576 THEN v.permissions_json END permissions_json,
      CASE WHEN octet_length(v.scopes_json::text)<=65536 THEN v.scopes_json END scopes_json,
      m.status member_status,m.login_policy,COALESCE(i.enabled,false) directory_enabled,COALESCE(i.observed_at,0)::bigint directory_observed_at,COALESCE(c.status='active',false) connection_active,
      selected.user_id selected_user,selected.assignment_id selected_assignment,
      EXISTS(SELECT 1 FROM iam_revoked_sessions revoked WHERE revoked.company_id='platform' AND revoked.user_id=$2 AND revoked.session_id IN($1,(SELECT parent_session_id FROM sso_session_bindings WHERE session_id=$1))) revoked,
      EXISTS(SELECT 1 FROM iam_mfa_sessions f JOIN iam_mfa mfa ON mfa.user_id=f.user_id AND mfa.enabled WHERE f.user_id=$2 AND f.session_id=$1 AND f.verified_at>$4-300000 AND f.verified_at<=$4) fresh_mfa,
      EXISTS(SELECT 1 FROM iam_emergency_sessions e WHERE e.company_id='platform' AND e.user_id=$2 AND e.session_id=$1 AND e.expires_at>$4) emergency,
      encode(sha256(convert_to(jsonb_build_array(to_jsonb(a),to_jsonb(r),to_jsonb(v),to_jsonb(m),to_jsonb(i),to_jsonb(c),to_jsonb(selected),to_jsonb(preference))::text,'UTF8')),'hex') source,
      octet_length(v.permissions_json::text)<=1048576 AND octet_length(v.scopes_json::text)<=65536 bounded
      FROM platform_assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN('*','platform') AND r.status='active'
      JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
      LEFT JOIN iam_members m ON m.company_id='platform' AND m.user_id=a.user_id
      LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.user_id=m.user_id AND i.connection_id=m.connection_id
      LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id
      LEFT JOIN admin_provider_contexts selected ON selected.session_id=$1
      LEFT JOIN account_workspace_preferences preference ON preference.user_id=a.user_id
      WHERE a.id=$3 AND a.user_id=$2"#).bind(&a.session_id).bind(&a.user_id).bind(&a.assignment_id).bind(now).fetch_all(c).await?;
    if rows.len() != 1 {
        return Err(authority().into());
    }
    let row = &rows[0];
    if row.try_get::<bool, _>("revoked")? {
        return Err(denied(
            FailureKind::Unauthenticated,
            "SESSION_REVOKED",
            "Sign in again to continue.",
        )
        .into());
    }
    if !row.try_get::<bool, _>("assignment_active")? || row.try_get::<String, _>("role")? != a.role
    {
        return Err(authority().into());
    }
    if !row.try_get::<bool, _>("bounded")? {
        return Err(denied(
            FailureKind::Unavailable,
            "IAM_AUTHORITY_UNAVAILABLE",
            "Unable to read current authority.",
        )
        .into());
    }
    if row
        .try_get::<Option<String>, _>("selected_user")?
        .is_some_and(|u| u != a.user_id)
        || row
            .try_get::<Option<String>, _>("selected_assignment")?
            .is_some_and(|id| id != a.assignment_id)
    {
        return Err(denied(
            FailureKind::Conflict,
            "PROVIDER_CONTEXT_CHANGED",
            "Your selected Platform context changed. Reload and review before continuing.",
        )
        .into());
    }
    let scopes: Vec<String> =
        serde_json::from_value(row.try_get("scopes_json")?).map_err(|_| authority())?;
    if !scopes.iter().any(|s| s == "platform") {
        return Err(authority().into());
    }
    if row.try_get::<i64, _>("role_revision")? != a.role_revision {
        return Err(denied(
            FailureKind::Conflict,
            "POLICY_CHANGED",
            "Your Platform policy changed. Refresh and review again.",
        )
        .into());
    }
    if row
        .try_get::<Option<String>, _>("member_status")?
        .is_some_and(|s| s != "active")
    {
        return Err(denied(
            FailureKind::Forbidden,
            "IDENTITY_SUSPENDED",
            "This identity no longer has access.",
        )
        .into());
    }
    let login = row.try_get::<Option<String>, _>("login_policy")?;
    let observed = row.try_get::<i64, _>("directory_observed_at")?;
    if login.as_deref() == Some("directory")
        && (!row.try_get::<bool, _>("directory_enabled")?
            || !row.try_get::<bool, _>("connection_active")?
            || observed <= now - 300000
            || observed > now)
    {
        return Err(denied(
            FailureKind::Forbidden,
            "DIRECTORY_ACCESS_UNAVAILABLE",
            "Your directory access needs a successful refresh.",
        )
        .into());
    }
    if login.as_deref() == Some("emergency") && !row.try_get::<bool, _>("emergency")? {
        return Err(denied(
            FailureKind::Forbidden,
            "EMERGENCY_ACCESS_REQUIRED",
            "Verify MFA and activate a temporary emergency session.",
        )
        .into());
    }
    let permissions: Vec<String> =
        serde_json::from_value(row.try_get("permissions_json")?).map_err(|_| authority())?;
    let permissions = crate::domain::effective_authority::effective_authority_permissions(
        &a.role,
        &permissions,
        false,
    );
    let permission = if operation == PlatformCommercialOperation::Apply {
        "platform.commercial.manage"
    } else {
        "platform.commercial.read"
    };
    if !permissions.iter().any(|p| p == permission) {
        return Err(denied(
            FailureKind::Forbidden,
            "PLATFORM_COMMERCIAL_REQUIRED",
            "Your Platform role does not grant this Commercial operation.",
        )
        .into());
    }
    if operation == PlatformCommercialOperation::Apply && !row.try_get::<bool, _>("fresh_mfa")? {
        return Err(denied(
            FailureKind::Forbidden,
            "FRESH_MFA_REQUIRED",
            "Verify an authenticator code before applying package changes.",
        )
        .into());
    }
    Ok((
        row.try_get("source")?,
        permissions
            .iter()
            .any(|p| p == "platform.commercial.manage"),
    ))
}
