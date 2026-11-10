//! Native Legal retention commands: policy lifecycle, assignment, destruction
//! request, independent approval, retry and cancellation.
//!
//! The owner unit stays in the adapter: replay is checked first, Company scope
//! is locked for policy changes, the current item/policy/destruction version is
//! re-checked, holds block destruction, and receipt/audit/outbox/version/event
//! are written in the same serializable transaction. Business rules live here.
use crate::domain::auxiliary::{
    LegalAuxiliaryActor, LegalRetentionAction, destruction_transition,
    positive_version, require_independent_review, retention_assignable, retention_requestable,
    validate_reason,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, IdentifierSource,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

pub type RetentionError = PgTransactionError<BusinessError>;
pub type RetentionFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RetentionError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPolicyRow {
    pub id: String,
    pub state: String,
    pub version: i64,
    pub created_by: String,
    pub days: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionAssignmentFacts {
    pub policy_id: String,
    pub policy_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionDestructionRow {
    pub id: String,
    pub state: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetentionWire {
    action: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    expected_version: Option<i64>,
    #[serde(default)]
    policy_id: Option<String>,
    #[serde(default)]
    policy_version: Option<i64>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    confirmation: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    days: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct RetentionCommand {
    pub action: LegalRetentionAction,
    pub id: Option<String>,
    pub expected_version: Option<i64>,
    pub policy_id: Option<String>,
    pub policy_version: Option<i64>,
    pub request_id: Option<String>,
    pub confirmation: Option<String>,
    pub reason: String,
    pub name: Option<String>,
    pub days: Option<i64>,
}

/// One serializable Legal retention unit. Every method borrows the caller's
/// connection; none opens an independent transaction or performs provider work.
pub trait LegalRetentionPorts {
    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        operation: &'a str,
        aggregate: &'a str,
        raw: &'a Value,
    ) -> RetentionFuture<'a, Option<Value>>;
    fn item<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
    ) -> RetentionFuture<'a, Option<(String, String, i64)>>;
    fn company_lock<'a>(&'a mut self, company: &'a str) -> RetentionFuture<'a, ()>;
    fn create_policy<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
        name: &'a str,
        days: i64,
        basis: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, ()>;
    fn policy<'a>(
        &'a mut self,
        company: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<RetentionPolicyRow>>;
    fn decide_policy<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        id: &'a str,
        version: i64,
        state: &'a str,
    ) -> RetentionFuture<'a, ()>;
    fn approved_policy<'a>(
        &'a mut self,
        company: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<(String, i64, i64)>>;
    fn assign_retention<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item: &'a str,
        policy_id: &'a str,
        policy_version: i64,
        days: i64,
        now: i64,
    ) -> RetentionFuture<'a, ()>;
    fn eligible_assignment<'a>(
        &'a mut self,
        item: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, Option<RetentionAssignmentFacts>>;
    fn request_destruction<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        item: &'a str,
        policy_id: &'a str,
        policy_version: i64,
        manifest: &'a Value,
        reason: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, ()>;
    fn destruction<'a>(
        &'a mut self,
        item: &'a str,
        id: Option<&'a str>,
    ) -> RetentionFuture<'a, Option<RetentionDestructionRow>>;
    fn change_destruction<'a>(
        &'a mut self,
        id: &'a str,
        state: &'a str,
        actor: &'a LegalAuxiliaryActor,
        now: i64,
    ) -> RetentionFuture<'a, ()>;
    fn active_hold<'a>(&'a mut self, item: &'a str) -> RetentionFuture<'a, bool>;
    fn manifest<'a>(
        &'a mut self,
        item: &'a str,
        company: &'a str,
        now: i64,
    ) -> RetentionFuture<'a, Value>;
    /// Writes receipt, audit, outbox, the item version advance and the event in
    /// the caller's transaction. `item` is `None` for Company policy changes.
    #[allow(clippy::too_many_arguments)]
    fn commit<'a>(
        &'a mut self,
        actor: &'a LegalAuxiliaryActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        operation: &'a str,
        aggregate: &'a str,
        version: i64,
        expected_version: Option<i64>,
        item: Option<&'a str>,
        event_id: &'a str,
        note: &'a str,
        result: Value,
        now: i64,
    ) -> RetentionFuture<'a, Value>;
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

pub fn parse_retention_command(raw: &Value) -> Result<RetentionCommand, BusinessError> {
    let wire: RetentionWire = serde_json::from_value(raw.clone())
        .map_err(|_| invalid("LEGAL_REQUEST_INVALID", "Use a current retention action."))?;
    let action = LegalRetentionAction::parse(&wire.action)
        .ok_or_else(|| invalid("LEGAL_REQUEST_INVALID", "Use a current retention action."))?;
    let reason = validate_reason(wire.reason.as_deref().unwrap_or_default())?;
    if !action.policy_lifecycle() {
        if wire.id.as_deref().map(str::trim).unwrap_or_default().is_empty() {
            return Err(invalid(
                "LEGAL_REQUEST_INVALID",
                "Choose the current Legal record.",
            ));
        }
    }
    if let Some(name) = &wire.name {
        let length = name.trim().chars().count();
        if !(3..=200).contains(&length) {
            return Err(invalid(
                "LEGAL_REQUEST_INVALID",
                "Name the policy in 3 to 200 characters.",
            ));
        }
    }
    if let Some(days) = wire.days {
        if !(1..=crate::domain::auxiliary::RETENTION_DAYS_MAX).contains(&days) {
            return Err(invalid(
                "LEGAL_REQUEST_INVALID",
                "Choose a retention period from 1 to 36,500 days.",
            ));
        }
    }
    Ok(RetentionCommand {
        action,
        id: wire.id,
        expected_version: wire.expected_version,
        policy_id: wire.policy_id,
        policy_version: wire.policy_version,
        request_id: wire.request_id,
        confirmation: wire.confirmation,
        reason,
        name: wire.name,
        days: wire.days,
    })
}

/// Execute one retention command. `request_hash` is the exact body hash used by
/// the retained receipt, so a lost response replays the original result.
#[allow(clippy::too_many_arguments)]
pub async fn execute_retention_command<P: LegalRetentionPorts>(
    ports: &mut P,
    actor: &LegalAuxiliaryActor,
    key: &CommandKey,
    request_hash: &str,
    raw: &Value,
    identifiers: &mut impl IdentifierSource,
    now: i64,
) -> Result<Value, RetentionError> {
    let command = parse_retention_command(raw)?;
    let operation = command.action.operation();
    if command.action.policy_lifecycle() {
        if !actor.can(crate::domain::auxiliary::RETENTION_POLICY_PERMISSION) {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role cannot manage retention policies.",
            )
            .into());
        }
        if !actor.company_scoped() {
            return Err(invalid(
                "LEGAL_COMPANY_SCOPE",
                "Manage retention policies at Company scope.",
            )
            .into());
        }
        let aggregate = actor.company_id.as_str().to_owned();
        if let Some(prior) = ports.replay(key, request_hash, operation, &aggregate, raw).await? {
            return Ok(prior);
        }
        ports.company_lock(&aggregate).await?;
        let result = if command.action == LegalRetentionAction::Policy {
            let name = command.name.as_deref().map(str::trim).unwrap_or_default();
            if name.chars().count() < 3 {
                return Err(invalid(
                    "LEGAL_REQUEST_INVALID",
                    "Name the policy in 3 to 200 characters.",
                )
                .into());
            }
            let days = command.days.ok_or_else(|| {
                invalid(
                    "LEGAL_REQUEST_INVALID",
                    "Choose a retention period from 1 to 36,500 days.",
                )
            })?;
            let policy_id = format!("retention-{}", key.as_str());
            ports
                .create_policy(actor, &policy_id, name, days, &command.reason, now)
                .await?;
            json!({"id": aggregate, "version": 1, "policyId": policy_id})
        } else {
            let policy = ports.policy(&aggregate, command.policy_id.as_deref()).await?;
            let Some(policy) = policy else {
                return Err(invalid(
                    "LEGAL_REQUEST_INVALID",
                    "Select a current Company retention policy.",
                )
                .into());
            };
            let version = positive_version(command.policy_version)?;
            if policy.version != version {
                return Err(conflict(
                    "LEGAL_VERSION_CONFLICT",
                    "Policy changed. Refresh.",
                )
                .into());
            }
            if command.action == LegalRetentionAction::ApprovePolicy {
                require_independent_review(
                    policy.created_by == actor.user_id.as_str(),
                    policy.state == "draft",
                    "An independent manager must approve the policy.",
                )?;
            }
            ports
                .decide_policy(
                    actor,
                    &policy.id,
                    policy.version,
                    if command.action == LegalRetentionAction::ApprovePolicy {
                        "approved"
                    } else {
                        "retired"
                    },
                )
                .await?;
            json!({"id": aggregate, "version": 1, "policyId": policy.id})
        };
        let event_id = identifiers.next();
        return ports
            .commit(
                actor,
                key,
                request_hash,
                operation,
                &aggregate,
                1,
                None,
                None,
                event_id.as_str(),
                &command.reason,
                result,
                now,
            )
            .await;
    }

    if !actor.can(crate::domain::auxiliary::RETENTION_RECORD_PERMISSION) {
        return Err(forbidden(
            "PERMISSION_REQUIRED",
            "Your selected role cannot authorize Legal destruction.",
        )
        .into());
    }
    let item_id = command.id.as_deref().map(str::trim).unwrap_or_default();
    let Some((state, _, item_version)) = ports.item(actor, item_id).await? else {
        return Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.").into());
    };
    if let Some(prior) = ports.replay(key, request_hash, operation, item_id, raw).await? {
        return Ok(prior);
    }
    let expected = positive_version(command.expected_version)?;
    if expected != item_version {
        return Err(conflict(
            "LEGAL_VERSION_CONFLICT",
            "The Legal record changed. Refresh before saving.",
        )
        .into());
    }
    let next_version = item_version + 1;
    match command.action {
        LegalRetentionAction::Assign => {
            retention_assignable(&state)?;
            let Some((policy_id, policy_version, days)) = ports
                .approved_policy(actor.company_id.as_str(), command.policy_id.as_deref())
                .await?
            else {
                return Err(invalid(
                    "LEGAL_RETENTION_POLICY_REQUIRED",
                    "Select an approved Company policy.",
                )
                .into());
            };
            ports
                .assign_retention(actor, item_id, &policy_id, policy_version, days, now)
                .await?;
        }
        LegalRetentionAction::Request => {
            let assignment = ports.eligible_assignment(item_id, now).await?;
            retention_requestable(&state, assignment.is_some())?;
            let assignment = assignment.expect("checked above");
            if ports.active_hold(item_id).await? {
                return Err(conflict(
                    "LEGAL_RETENTION_HOLD",
                    "An active Legal hold blocks destruction.",
                )
                .into());
            }
            let manifest = ports
                .manifest(item_id, actor.company_id.as_str(), now)
                .await?;
            ports
                .request_destruction(
                    actor,
                    item_id,
                    &assignment.policy_id,
                    assignment.policy_version,
                    &manifest,
                    &command.reason,
                    now,
                )
                .await?;
        }
        action => {
            let Some(row) = ports.destruction(item_id, command.request_id.as_deref()).await? else {
                return Err(invalid(
                    "LEGAL_RETENTION_STATE",
                    "Destruction request unavailable.",
                )
                .into());
            };
            let state = destruction_transition(action, &row.state)?;
            if action == LegalRetentionAction::Approve {
                if command.confirmation.as_deref() != Some(row.id.as_str()) {
                    return Err(invalid(
                        "LEGAL_RETENTION_CONFIRMATION",
                        "Confirm the exact destruction request ID.",
                    )
                    .into());
                }
                require_independent_review(
                    row.requested_by == actor.user_id.as_str(),
                    row.state == "requested",
                    "An independent manager must authorize destruction.",
                )?;
            }
            if matches!(
                action,
                LegalRetentionAction::Approve | LegalRetentionAction::Retry
            ) && ports.active_hold(item_id).await?
            {
                return Err(conflict(
                    "LEGAL_RETENTION_HOLD",
                    "An active Legal hold blocks destruction.",
                )
                .into());
            }
            ports
                .change_destruction(&row.id, state, actor, now)
                .await?;
        }
    }
    let result = json!({"id": item_id, "version": next_version});
    let event_id = identifiers.next();
    ports
        .commit(
            actor,
            key,
            request_hash,
            operation,
            item_id,
            next_version,
            Some(expected),
            Some(item_id),
            event_id.as_str(),
            &command.reason,
            result,
            now,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::{AssignmentId, CompanyId, HoldingId, RecordId, UserId};
    use std::collections::VecDeque;

    fn actor(permissions: &[&str]) -> LegalAuxiliaryActor {
        LegalAuxiliaryActor {
            user_id: UserId::new("user-one").unwrap(),
            assignment_id: AssignmentId::new("assignment-one").unwrap(),
            holding_id: HoldingId::new("holding-one").unwrap(),
            company_id: CompanyId::new("company-one").unwrap(),
            branch_id: None,
            department_id: None,
            role: "legal_manager".into(),
            permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    struct Ids(VecDeque<&'static str>);
    impl IdentifierSource for Ids {
        fn next(&mut self) -> RecordId {
            RecordId::new(self.0.pop_front().unwrap_or("event-fallback")).unwrap()
        }
    }

    #[derive(Default)]
    struct Fake {
        replay: Option<Value>,
        item: Option<(String, String, i64)>,
        policy: Option<RetentionPolicyRow>,
        approved: Option<(String, i64, i64)>,
        assignment: Option<RetentionAssignmentFacts>,
        destruction: Option<RetentionDestructionRow>,
        hold: bool,
        calls: Vec<String>,
        committed: Option<Value>,
    }

    impl LegalRetentionPorts for Fake {
        fn replay<'a>(&'a mut self, _: &'a CommandKey, _: &'a str, _: &'a str, _: &'a str, _: &'a Value) -> RetentionFuture<'a, Option<Value>> {
            self.calls.push("replay".into());
            let prior = self.replay.clone();
            Box::pin(async move { Ok(prior) })
        }
        fn item<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, _: &'a str) -> RetentionFuture<'a, Option<(String, String, i64)>> {
            self.calls.push("item".into());
            let item = self.item.clone();
            Box::pin(async move { Ok(item) })
        }
        fn company_lock<'a>(&'a mut self, _: &'a str) -> RetentionFuture<'a, ()> {
            self.calls.push("lock".into());
            Box::pin(async { Ok(()) })
        }
        fn create_policy<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, id: &'a str, _: &'a str, _: i64, _: &'a str, _: i64) -> RetentionFuture<'a, ()> {
            self.calls.push(format!("create_policy:{id}"));
            Box::pin(async { Ok(()) })
        }
        fn policy<'a>(&'a mut self, _: &'a str, _: Option<&'a str>) -> RetentionFuture<'a, Option<RetentionPolicyRow>> {
            self.calls.push("policy".into());
            let policy = self.policy.clone();
            Box::pin(async move { Ok(policy) })
        }
        fn decide_policy<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, _: &'a str, _: i64, state: &'a str) -> RetentionFuture<'a, ()> {
            self.calls.push(format!("decide:{state}"));
            Box::pin(async { Ok(()) })
        }
        fn approved_policy<'a>(&'a mut self, _: &'a str, _: Option<&'a str>) -> RetentionFuture<'a, Option<(String, i64, i64)>> {
            self.calls.push("approved".into());
            let approved = self.approved.clone();
            Box::pin(async move { Ok(approved) })
        }
        fn assign_retention<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, _: &'a str, _: &'a str, _: i64, days: i64, _: i64) -> RetentionFuture<'a, ()> {
            self.calls.push(format!("assign:{days}"));
            Box::pin(async { Ok(()) })
        }
        fn eligible_assignment<'a>(&'a mut self, _: &'a str, _: i64) -> RetentionFuture<'a, Option<RetentionAssignmentFacts>> {
            self.calls.push("eligible".into());
            let assignment = self.assignment.clone();
            Box::pin(async move { Ok(assignment) })
        }
        fn request_destruction<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, _: &'a str, policy: &'a str, version: i64, _: &'a Value, _: &'a str, _: i64) -> RetentionFuture<'a, ()> {
            self.calls.push(format!("request:{policy}:{version}"));
            Box::pin(async { Ok(()) })
        }
        fn destruction<'a>(&'a mut self, _: &'a str, _: Option<&'a str>) -> RetentionFuture<'a, Option<RetentionDestructionRow>> {
            self.calls.push("destruction".into());
            let row = self.destruction.clone();
            Box::pin(async move { Ok(row) })
        }
        fn change_destruction<'a>(&'a mut self, _: &'a str, state: &'a str, _: &'a LegalAuxiliaryActor, _: i64) -> RetentionFuture<'a, ()> {
            self.calls.push(format!("change:{state}"));
            Box::pin(async { Ok(()) })
        }
        fn active_hold<'a>(&'a mut self, _: &'a str) -> RetentionFuture<'a, bool> {
            self.calls.push("hold".into());
            let hold = self.hold;
            Box::pin(async move { Ok(hold) })
        }
        fn manifest<'a>(&'a mut self, _: &'a str, _: &'a str, _: i64) -> RetentionFuture<'a, Value> {
            self.calls.push("manifest".into());
            Box::pin(async { Ok(json!({"documents": []})) })
        }
        fn commit<'a>(&'a mut self, _: &'a LegalAuxiliaryActor, _: &'a CommandKey, _: &'a str, _: &'a str, _: &'a str, _: i64, _: Option<i64>, _: Option<&'a str>, _: &'a str, _: &'a str, result: Value, _: i64) -> RetentionFuture<'a, Value> {
            self.calls.push("commit".into());
            self.committed = Some(result.clone());
            Box::pin(async move { Ok(result) })
        }
    }

    fn key() -> CommandKey {
        CommandKey::new("command-key-one").unwrap()
    }

    async fn run(fake: &mut Fake, raw: Value, now: i64) -> Result<Value, RetentionError> {
        let actor = actor(&["legal.manage", "legal.destroy"]);
        execute_retention_command(
            fake,
            &actor,
            &key(),
            "a".repeat(64).as_str(),
            &raw,
            &mut Ids(VecDeque::from(["event-one"])),
            now,
        )
        .await
    }

    #[tokio::test]
    async fn policy_lifecycle_requires_manage_company_scope_and_independence() {
        let raw = json!({"action":"retention.policy","name":"Seven year retention","days":2555,"reason":"Required by policy"});
        let mut fake = Fake::default();
        let result = run(&mut fake, raw, 10).await.unwrap();
        assert_eq!(result["policyId"], "retention-command-key-one");
        assert_eq!(
            fake.calls,
            vec!["replay", "lock", "create_policy:retention-command-key-one", "commit"]
        );

        // Branch scope cannot manage a Company policy.
        let mut branch_actor = actor(&["legal.manage"]);
        branch_actor.branch_id = Some(RecordId::new("branch-one").unwrap());
        let mut fake = Fake::default();
        let error = execute_retention_command(
            &mut fake,
            &branch_actor,
            &key(),
            "a".repeat(64).as_str(),
            &json!({"action":"retention.policy","name":"Seven year retention","days":2555,"reason":"Required by policy"}),
            &mut Ids(VecDeque::new()),
            10,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "LEGAL_COMPANY_SCOPE");
        assert!(fake.calls.is_empty());
    }

    #[tokio::test]
    async fn policy_approval_requires_a_different_manager_and_current_version() {
        let raw = json!({"action":"retention.approve-policy","policyId":"policy-one","policyVersion":2,"reason":"Independent review"});
        let mut fake = Fake {
            policy: Some(RetentionPolicyRow {
                id: "policy-one".into(),
                state: "draft".into(),
                version: 2,
                created_by: "user-two".into(),
                days: 2555,
            }),
            ..Fake::default()
        };
        run(&mut fake, raw, 10).await.unwrap();
        assert!(fake.calls.contains(&"decide:approved".to_owned()));

        let mut fake = Fake {
            policy: Some(RetentionPolicyRow {
                id: "policy-one".into(),
                state: "draft".into(),
                version: 2,
                created_by: "user-one".into(),
                days: 2555,
            }),
            ..Fake::default()
        };
        let error = run(&mut fake, json!({"action":"retention.approve-policy","policyId":"policy-one","policyVersion":2,"reason":"Independent review"}), 10)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEGAL_INDEPENDENT_REVIEW");

        let mut fake = Fake {
            policy: Some(RetentionPolicyRow {
                id: "policy-one".into(),
                state: "draft".into(),
                version: 3,
                created_by: "user-two".into(),
                days: 2555,
            }),
            ..Fake::default()
        };
        let error = run(&mut fake, json!({"action":"retention.approve-policy","policyId":"policy-one","policyVersion":2,"reason":"Independent review"}), 10)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEGAL_VERSION_CONFLICT");
    }

    #[tokio::test]
    async fn replay_returns_the_original_result_before_any_business_check() {
        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            replay: Some(json!({"id":"item-one","version":5})),
            hold: true,
            ..Fake::default()
        };
        let result = run(&mut fake, json!({"action":"retention.request","id":"item-one","expectedVersion":4,"reason":"Elapsed"}), 10)
            .await
            .unwrap();
        assert_eq!(result["version"], 5);
        assert_eq!(fake.calls, vec!["item", "replay"]);
    }

    #[tokio::test]
    async fn request_requires_an_elapsed_policy_and_no_active_hold() {
        let raw = json!({"action":"retention.request","id":"item-one","expectedVersion":4,"reason":"Elapsed"});
        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            assignment: Some(RetentionAssignmentFacts {
                policy_id: "policy-one".into(),
                policy_version: 3,
            }),
            ..Fake::default()
        };
        run(&mut fake, raw.clone(), 10).await.unwrap();
        assert!(fake.calls.contains(&"request:policy-one:3".to_owned()));
        assert!(fake.calls.contains(&"manifest".to_owned()));

        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            assignment: None,
            ..Fake::default()
        };
        let error = run(&mut fake, raw.clone(), 10).await.unwrap_err();
        assert_eq!(business(error).code, "LEGAL_RETENTION_NOT_ELAPSED");

        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            assignment: Some(RetentionAssignmentFacts {
                policy_id: "policy-one".into(),
                policy_version: 3,
            }),
            hold: true,
            ..Fake::default()
        };
        let error = run(&mut fake, raw, 10).await.unwrap_err();
        assert_eq!(business(error).code, "LEGAL_RETENTION_HOLD");
        assert!(!fake.calls.contains(&"request:policy-one:3".to_owned()));
    }

    #[tokio::test]
    async fn approval_requires_the_exact_confirmation_and_an_independent_manager() {
        let raw = json!({"action":"retention.approve","id":"item-one","expectedVersion":4,"requestId":"destroy-one","confirmation":"destroy-one","reason":"Authorize"});
        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            destruction: Some(RetentionDestructionRow {
                id: "destroy-one".into(),
                state: "requested".into(),
                requested_by: "user-two".into(),
                approved_by: None,
            }),
            ..Fake::default()
        };
        run(&mut fake, raw.clone(), 10).await.unwrap();
        assert!(fake.calls.contains(&"change:approved".to_owned()));

        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            destruction: Some(RetentionDestructionRow {
                id: "destroy-one".into(),
                state: "requested".into(),
                requested_by: "user-one".into(),
                approved_by: None,
            }),
            ..Fake::default()
        };
        let error = run(&mut fake, raw, 10).await.unwrap_err();
        assert_eq!(business(error).code, "LEGAL_INDEPENDENT_REVIEW");

        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 4)),
            destruction: Some(RetentionDestructionRow {
                id: "destroy-one".into(),
                state: "requested".into(),
                requested_by: "user-two".into(),
                approved_by: None,
            }),
            ..Fake::default()
        };
        let error = run(&mut fake, json!({"action":"retention.approve","id":"item-one","expectedVersion":4,"requestId":"destroy-one","confirmation":"other","reason":"Authorize"}), 10)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEGAL_RETENTION_CONFIRMATION");
    }

    #[tokio::test]
    async fn stale_versions_and_missing_destroy_permission_fail_closed() {
        let mut fake = Fake {
            item: Some(("archived".into(), "item-one".into(), 5)),
            ..Fake::default()
        };
        let error = run(&mut fake, json!({"action":"retention.cancel","id":"item-one","expectedVersion":4,"reason":"Cancel"}), 10)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEGAL_VERSION_CONFLICT");

        let reader = actor(&["legal.read"]);
        let mut fake = Fake::default();
        let error = execute_retention_command(
            &mut fake,
            &reader,
            &key(),
            "a".repeat(64).as_str(),
            &json!({"action":"retention.assign","id":"item-one","expectedVersion":4,"policyId":"policy-one","reason":"Assign"}),
            &mut Ids(VecDeque::new()),
            10,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PERMISSION_REQUIRED");
        assert!(fake.calls.is_empty());
        let _ = reader;
    }

    fn business(error: RetentionError) -> BusinessError {
        match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => panic!("unexpected persistence error"),
        }
    }

    #[test]
    fn parsing_bounds_name_days_and_reason() {
        assert!(parse_retention_command(&json!({"action":"retention.policy","name":"Na","days":1,"reason":"Valid reason"})).is_err());
        assert!(parse_retention_command(&json!({"action":"retention.policy","name":"Valid","days":0,"reason":"Valid reason"})).is_err());
        assert!(parse_retention_command(&json!({"action":"retention.policy","name":"Valid","days":1,"reason":"no"})).is_err());
        assert!(parse_retention_command(&json!({"action":"retention.other","reason":"Valid reason"})).is_err());
        assert!(parse_retention_command(&json!({"action":"retention.assign","reason":"Valid reason"})).is_err());
    }
}
