//! Native Sales customer follow-up command rules and orchestration.
use erpx_server_sales::public::customer_followup_commands::{
    ChangeFollowup, CreateFollowup, FollowupAction, FollowupActor, FollowupCommandRepository,
    FollowupFuture, FollowupOrder, FollowupPurpose, FollowupReceiptSnapshot, FollowupSource,
    change_customer_followup, create_customer_followup, valid_due_date,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "customer-followup-0001";

fn actor(user: &str, permissions: &[&str]) -> FollowupActor {
    FollowupActor {
        user_id: user.into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        assignment_id: format!("assignment-{user}"),
        role: "sales_manager".into(),
        session_id: Some("session".into()),
        assignment_version: Some(1),
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn owner() -> FollowupActor {
    actor("owner", &["commerce.read", "commerce.sales"])
}

fn manager() -> FollowupActor {
    actor("manager", &["commerce.read", "commerce.sales", "commerce.approve"])
}

fn order() -> FollowupOrder {
    FollowupOrder {
        id: "order".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        kind: "quote".into(),
        witness: "[\"order\"]".into(),
    }
}

fn create_input() -> CreateFollowup {
    CreateFollowup {
        order_id: "order".into(),
        expected_version: 1,
        owner_id: "owner".into(),
        due_date: "2026-09-22".into(),
        next_action: "Call the customer about installation requirements".into(),
    }
}

fn change_input(action: FollowupAction) -> ChangeFollowup {
    ChangeFollowup {
        action,
        expected_version: 1,
        owner_id: (action == FollowupAction::Reassign).then(|| "manager".to_owned()),
        due_date: (action == FollowupAction::Reassign).then(|| "2026-09-23".to_owned()),
        reason: "Recorded contact outcome for this customer".into(),
    }
}

fn source(status: &str, version: i64, owner_id: &str) -> FollowupSource {
    FollowupSource {
        id: format!("followup-{KEY}"),
        company_id: "company".into(),
        order_id: "order".into(),
        order_kind: "quote".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        owner_id: Some(owner_id.into()),
        due_date: Some("2026-09-22".into()),
        status: Some(status.into()),
        version: Some(version),
        witness: "{\"followup\":[\"x\"],\"order\":[\"order\"]}".into(),
    }
}

#[derive(Default)]
struct Fake {
    receipt: Option<FollowupReceiptSnapshot>,
    authority: bool,
    eligible: bool,
    capability: Vec<(String, FollowupPurpose)>,
    commits: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            authority: true,
            eligible: true,
            ..Default::default()
        }
    }
}

impl FollowupCommandRepository for Fake {
    type Error = BusinessError;

    fn lock_command<'a>(&'a mut self, _: &'a str) -> FollowupFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn digest<'a>(&'a mut self, value: &'a str) -> FollowupFuture<'a, String, Self::Error> {
        Box::pin(async move {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            Ok(format!("{:064x}", hasher.finish()))
        })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupReceiptSnapshot>, Self::Error> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }
    fn order<'a>(
        &'a mut self,
        _: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupOrder>, Self::Error> {
        Box::pin(async { Ok(Some(order())) })
    }
    fn followup<'a>(
        &'a mut self,
        _: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupSource>, Self::Error> {
        Box::pin(async { Ok(Some(source("open", 1, "owner"))) })
    }
    fn owner_eligible<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> FollowupFuture<'a, bool, Self::Error> {
        let eligible = self.eligible;
        Box::pin(async move { Ok(eligible) })
    }
    fn capability<'a>(
        &'a mut self,
        _: &'a str,
        capability: &'a str,
        purpose: FollowupPurpose,
        _: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        self.capability.push((capability.to_owned(), purpose));
        Box::pin(async { Ok(()) })
    }
    fn current_authority<'a>(
        &'a mut self,
        _: &'a FollowupActor,
        _: bool,
        _: i64,
    ) -> FollowupFuture<'a, bool, Self::Error> {
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }
    fn commit_create<'a>(
        &'a mut self,
        plan: &'a erpx_server_sales::public::customer_followup_commands::CreateFollowupPlan<'a>,
        _: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        let authority = self.authority;
        if !authority {
            return Box::pin(async {
                Err(erpx_server_sales::public::customer_followup_commands::access_changed())
            });
        }
        self.commits
            .lock()
            .unwrap()
            .push(format!("create:{}:{}", plan.id, plan.input.owner_id));
        Box::pin(async { Ok(()) })
    }
    fn commit_change<'a>(
        &'a mut self,
        plan: &'a erpx_server_sales::public::customer_followup_commands::ChangeFollowupPlan<'a>,
        _: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        self.commits.lock().unwrap().push(format!(
            "{}:{}:{}:{}",
            plan.input.action.as_str(),
            plan.id,
            plan.owner,
            plan.receipt.version
        ));
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn calendar_due_dates_and_bounded_text_are_enforced() {
    for valid in ["2026-09-22", "2024-02-29", "2026-12-31"] {
        assert!(valid_due_date(valid), "{valid}");
    }
    for invalid in ["2026-9-22", "2026-02-29", "2025-13-01", "2025-04-31", "2026-00-10"] {
        assert!(!valid_due_date(invalid), "{invalid}");
    }
    assert!(create_input().validate().is_ok());
    assert!(CreateFollowup { expected_version: 2, ..create_input() }.validate().is_err());
    assert!(CreateFollowup { due_date: "2026-02-30".into(), ..create_input() }.validate().is_err());
    assert!(CreateFollowup { next_action: "x".into(), ..create_input() }.validate().is_err());
    assert!(ChangeFollowup { action: FollowupAction::Reassign, owner_id: None, ..change_input(FollowupAction::Reassign) }.validate().is_err());
    assert!(ChangeFollowup { action: FollowupAction::Complete, owner_id: Some("owner".into()), ..change_input(FollowupAction::Complete) }.validate().is_err());
}

#[tokio::test]
async fn create_replays_and_settles_new_work() {
    let mut repo = Fake::ready();
    let receipt = create_customer_followup(&owner(), create_input(), KEY, &mut repo, NOW)
        .await
        .unwrap();
    assert_eq!(receipt.version, 1);
    assert_eq!(receipt.status, "open");
    assert_eq!(repo.capability, [("sales.quotes".to_owned(), FollowupPurpose::NewWork)]);
    assert_eq!(repo.commits.lock().unwrap().len(), 1);

    let mut replay = Fake::ready();
    replay.receipt = Some(FollowupReceiptSnapshot {
        request_hash: format!(
            "{:064x}",
            {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                create_input().identity(&owner(), &format!("followup-{KEY}"), KEY).hash(&mut hasher);
                hasher.finish()
            }
        ),
        result_json: receipt.wire().to_string(),
        operation: "customer.followup.create".into(),
        actor_id: "owner".into(),
        record_id: format!("followup-{KEY}"),
    });
    assert_eq!(
        create_customer_followup(&owner(), create_input(), KEY, &mut replay, NOW)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(replay.capability, [("sales.quotes".to_owned(), FollowupPurpose::History)]);
    assert!(replay.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn only_the_owner_completes_and_only_a_manager_reassigns_or_foreign_cancels() {
    let mut repo = Fake::ready();
    let error = change_customer_followup(
        &manager(),
        &format!("followup-{KEY}"),
        change_input(FollowupAction::Complete),
        KEY,
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_FOLLOWUP_OWNER_REQUIRED");
    assert!(repo.commits.lock().unwrap().is_empty());

    let mut repo = Fake::ready();
    assert_eq!(
        change_customer_followup(
            &owner(),
            &format!("followup-{KEY}"),
            change_input(FollowupAction::Complete),
            KEY,
            &mut repo,
            NOW,
        )
        .await
        .unwrap()
        .status,
        "completed"
    );

    let mut repo = Fake::ready();
    let error = change_customer_followup(
        &owner(),
        &format!("followup-{KEY}"),
        change_input(FollowupAction::Reassign),
        KEY,
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_FOLLOWUP_OWNER_REQUIRED");

    let mut repo = Fake::ready();
    assert_eq!(
        change_customer_followup(
            &manager(),
            &format!("followup-{KEY}"),
            change_input(FollowupAction::Cancel),
            KEY,
            &mut repo,
            NOW,
        )
        .await
        .unwrap()
        .status,
        "cancelled"
    );

    let mut repo = Fake::ready();
    let error = change_customer_followup(
        &actor("other", &["commerce.read", "commerce.sales"]),
        &format!("followup-{KEY}"),
        change_input(FollowupAction::Cancel),
        KEY,
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_FOLLOWUP_OWNER_REQUIRED");

    let mut repo = Fake::ready();
    let receipt = change_customer_followup(
        &owner(),
        &format!("followup-{KEY}"),
        change_input(FollowupAction::Cancel),
        KEY,
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(receipt.status, "cancelled");
}

#[tokio::test]
async fn version_and_owner_eligibility_and_authority_fail_closed() {
    for (mutate, code) in [
        (
            Box::new(|repo: &mut Fake| {
                repo.eligible = false;
            }) as Box<dyn Fn(&mut Fake)>,
            "CONFLICT",
        ),
        (
            Box::new(|repo: &mut Fake| {
                repo.authority = false;
            }),
            "FOLLOWUP_ACCESS_CHANGED",
        ),
    ] {
        let mut repo = Fake::ready();
        mutate(&mut repo);
        let mut input = create_input();
        input.owner_id = "owner".into();
        let error = create_customer_followup(&owner(), input, KEY, &mut repo, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
        assert!(repo.commits.lock().unwrap().is_empty());
    }

    let mut repo = Fake::ready();
    let mut input = change_input(FollowupAction::Complete);
    input.expected_version = 9;
    // The fake's followup is version 1, so the use case refuses before commit.
    let error = change_customer_followup(
        &owner(),
        &format!("followup-{KEY}"),
        input,
        KEY,
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "CONFLICT");
}
