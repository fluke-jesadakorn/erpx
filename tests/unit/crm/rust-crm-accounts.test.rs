//! Native CRM account create owner unit facts. Domain validation +
//! application delegation against an in-memory fake. No PostgreSQL, no
//! purchase check, no HTTP. The TS runtime stays authoritative.
use erpx_server_crm::public::accounts::{
    AccountCreateInput, AccountCreatePlan, AccountsFuture, AccountCreateRepository, create_account,
    validate_account_create,
};
use erpx_server_crm::public::pipeline::CrmPipelineActor;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn input(kind: &str, name: &str) -> AccountCreateInput {
    serde_json::from_value(json!({
        "action": "create-account", "kind": kind, "name": name
    }))
    .unwrap()
}

struct Fake {
    inserted: Option<(String, String, String, AccountCreatePlan, i64)>,
}

impl Fake {
    fn new() -> Self {
        Self { inserted: None }
    }
}

impl AccountCreateRepository for Fake {
    type Error = BusinessError;

    fn insert_account<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a AccountCreatePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        self.inserted = Some((
            company_id.to_owned(),
            record_id.to_owned(),
            created_by.to_owned(),
            plan.clone(),
            now,
        ));
        Box::pin(async { Ok(()) })
    }
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[test]
fn trims_and_accepts_business_and_individual_kinds() {
    let plan = validate_account_create(&input("business", "  Acme Pty Ltd  ")).unwrap();
    assert_eq!(plan.kind, "business");
    assert_eq!(plan.name, "Acme Pty Ltd");
    let plan = validate_account_create(&input("individual", "Ada Lovelace")).unwrap();
    assert_eq!(plan.kind, "individual");
}

#[test]
fn rejects_control_chars_empty_and_overlong_names() {
    assert!(validate_account_create(&input("business", "A\u{0007}B")).is_err());
    assert!(validate_account_create(&input("business", "   ")).is_err());
    let long = "a".repeat(161);
    assert!(validate_account_create(&input("business", &long)).is_err());
}

#[test]
fn rejects_unknown_kind_and_wrong_action() {
    let err = validate_account_create(&input("organization", "Valid name")).unwrap_err();
    assert_eq!(err.code, "CRM_ACCOUNT_KIND_INVALID");
    assert_eq!(err.kind, FailureKind::InvalidInput);
    let wrong = serde_json::from_value::<AccountCreateInput>(json!({
        "action": "link-finance", "kind": "business", "name": "Valid name"
    }))
    .unwrap();
    let err = validate_account_create(&wrong).unwrap_err();
    assert_eq!(err.code, "CRM_ACCOUNT_ACTION_INVALID");
}

#[tokio::test]
async fn application_delegates_trimmed_plan_to_repository() {
    let actor = CrmPipelineActor {
        user_id: "owner".into(),
        company_id: company(),
        branch_id: None,
        department_id: None,
        permissions: vec!["commerce.read".into(), "commerce.sales".into()],
        modules: vec!["crm".into()],
        expires_at: NOW.get() + 60_000,
    };
    let mut repo = Fake::new();
    let plan = create_account(
        &actor.user_id,
        actor.company_id.as_str(),
        &input("individual", "  Ada Lovelace  "),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(plan.name, "Ada Lovelace");
    let (company_id, record_id, created_by, saved, now) = repo.inserted.unwrap();
    assert_eq!(company_id, "company");
    assert!(record_id.starts_with("account-"));
    assert_eq!(created_by, "owner");
    assert_eq!(saved, plan);
    assert_eq!(now, NOW.get());
}

#[tokio::test]
async fn application_surfaces_validation_errors_without_repository_write() {
    let mut repo = Fake::new();
    let err = create_account(
        "owner",
        company().as_str(),
        &input("business", "   "),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CRM_ACCOUNT_INPUT_INVALID");
    assert!(repo.inserted.is_none());
}
