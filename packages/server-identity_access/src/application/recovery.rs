use super::account_auth::PresentedSecrets;
use super::account_ports::{AccountFuture, AuthenticationSecrets, DeviceProof};
use crate::domain::account_auth::{AccountPolicy, RequestAudience};
use crate::domain::recovery::{EmergencyAccessList, RecoveryInput, valid_key};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde_json::Value;
use std::sync::Arc;

/// The authenticated actor of one emergency-access request. It always comes
/// from the presented session, never from the request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryActor {
    pub user_id: String,
    pub session_id: String,
}

/// A resolved Holding/Company scope plus the exact database commands for it.
#[derive(Clone)]
pub struct RecoveryTarget {
    pub company_id: String,
    pub holding_slug: Option<String>,
    pub display_name: String,
    pub commands: Arc<dyn RecoveryCommands>,
}

impl RecoveryTarget {
    pub fn scope_label(&self) -> String {
        match &self.holding_slug {
            Some(_) => crate::domain::recovery::holding_label(&self.display_name, &self.company_id),
            None => crate::domain::recovery::platform_scope().to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActiveRow {
    pub company_id: String,
    pub reason: String,
    pub expires_at: i64,
}

/// Owner port: every statement for one command commits in a single serializable
/// unit with the receipt, audit row and temporary session.
pub trait RecoveryCommands: Send + Sync {
    fn activate<'a>(
        &'a self,
        proof: DeviceProof,
        target: &'a RecoveryTarget,
        input: &'a RecoveryInput,
        key: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Value>;
    fn revoke<'a>(
        &'a self,
        proof: DeviceProof,
        target: &'a RecoveryTarget,
        input: &'a RecoveryInput,
        key: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Value>;
    fn active<'a>(
        &'a self,
        proof: DeviceProof,
        company_id: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Vec<ActiveRow>>;
}

/// Holding/Platform scope registry assembled by the composition crate.
pub trait RecoveryRegistry: Send + Sync {
    fn platform(&self) -> RecoveryTarget;
    fn holding(&self, slug: &str) -> Option<RecoveryTarget>;
    fn holdings(&self) -> Vec<RecoveryTarget>;
}

fn workspace_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "WORKSPACE_REQUIRED",
        "Choose the company and holding for emergency access.",
    )
}

fn command_key_required() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "COMMAND_KEY_REQUIRED",
        "A valid Idempotency-Key is required.",
    )
}

fn status_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECOVERY_STATUS_UNAVAILABLE",
        "Temporary access status is currently unavailable. Try again shortly.",
    )
}

/// Reviewed temporary access in the current Account session.
pub struct EmergencyAccess {
    policy: AccountPolicy,
    secrets: Arc<dyn AuthenticationSecrets>,
    registry: Arc<dyn RecoveryRegistry>,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl EmergencyAccess {
    pub fn new(
        policy: AccountPolicy,
        secrets: Arc<dyn AuthenticationSecrets>,
        registry: Arc<dyn RecoveryRegistry>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            policy,
            secrets,
            registry,
            clock,
        }
    }

    /// Account-only surface: a Holding or product host cannot start break-glass.
    pub fn proof(
        &self,
        host: &str,
        presented: &PresentedSecrets,
    ) -> Result<DeviceProof, BusinessError> {
        let audience = self
            .policy
            .audience(host)
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if audience != RequestAudience::Account {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        Ok(presented.proof(self.secrets.as_ref()))
    }

    fn target(&self, input: &RecoveryInput) -> Result<RecoveryTarget, BusinessError> {
        if let Some(slug) = &input.holding_slug {
            return match self.registry.holding(slug) {
                Some(mut target) => {
                    target.company_id = input.company_id.clone();
                    Ok(target)
                }
                None => Err(workspace_required()),
            };
        }
        if input.company_id != "platform" {
            return Err(workspace_required());
        }
        let mut target = self.registry.platform();
        target.company_id = input.company_id.clone();
        Ok(target)
    }

    pub async fn activate(
        &self,
        proof: DeviceProof,
        raw: &Value,
        key: &str,
    ) -> Result<Value, BusinessError> {
        if !valid_key(key) {
            return Err(command_key_required());
        }
        let input = RecoveryInput::parse(raw)?;
        let target = self.target(&input)?;
        let now = self.clock.now().get();
        target
            .commands
            .activate(proof, &target, &input, key, now)
            .await
    }

    pub async fn revoke(
        &self,
        proof: DeviceProof,
        raw: &Value,
        key: &str,
    ) -> Result<Value, BusinessError> {
        if !valid_key(key) {
            return Err(command_key_required());
        }
        let input = RecoveryInput::parse(raw)?;
        let target = self.target(&input)?;
        let now = self.clock.now().get();
        target
            .commands
            .revoke(proof, &target, &input, key, now)
            .await
    }

    /// Reads Platform and every configured Holding. A failed Holding scope is
    /// reported without hiding the scopes that did resolve; a failed Platform
    /// scope is unavailable.
    pub async fn active(&self, proof: DeviceProof) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let mut access = Vec::new();
        let mut unavailable = Vec::new();
        let platform = self.registry.platform();
        for row in platform
            .commands
            .active(proof.clone(), &platform.company_id, now)
            .await?
        {
            access.push(crate::domain::recovery::EmergencyAccess {
                company_id: row.company_id,
                holding_slug: None,
                scope_label: platform.scope_label(),
                reason: row.reason,
                expires_at: row.expires_at,
            });
        }
        for holding in self.registry.holdings() {
            match holding.commands.active(proof.clone(), "", now).await {
                Ok(rows) => {
                    for row in rows {
                        access.push(crate::domain::recovery::EmergencyAccess {
                            company_id: row.company_id.clone(),
                            holding_slug: holding.holding_slug.clone(),
                            scope_label: crate::domain::recovery::holding_label(
                                &holding.display_name,
                                &row.company_id,
                            ),
                            reason: row.reason,
                            expires_at: row.expires_at,
                        });
                    }
                }
                Err(_) => unavailable.push(holding.scope_label()),
            }
        }
        serde_json::to_value(EmergencyAccessList {
            access,
            unavailable_scopes: unavailable,
        })
        .map_err(|_| status_unavailable())
    }
}
