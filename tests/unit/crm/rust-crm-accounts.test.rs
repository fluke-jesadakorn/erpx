//! Native CRM account create owner unit facts. Domain validation +
//! application delegation against an in-memory fake. No PostgreSQL, no
//! purchase check, no HTTP. The TS runtime stays authoritative.
use erpx_crm_server::public::accounts::{
    AccountCreateInput, AccountCreatePlan, AccountCreateRepository, AccountLinkFinanceActor,
    AccountLinkFinanceInput, AccountLinkFinancePlan, AccountLinkFinanceRepository, AccountsFuture,
    ContactCreateInput, ContactCreatePlan, GuestLinkInput, GuestLinkPlan, GuestSharingInput,
    GuestSharingPlan, IdentityFamilyRepository, change_guest_sharing, create_account,
    create_contact, link_account_finance, link_guest, validate_account_create,
    validate_account_link_finance, validate_contact_create, validate_guest_link,
    validate_guest_sharing,
};
use erpx_crm_server::public::pipeline::CrmPipelineActor;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
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
fn matches_javascript_utf16_length_and_rejects_empty_and_overlong_names() {
    assert!(validate_account_create(&input("business", "A\u{0007}B")).is_ok());
    assert!(validate_account_create(&input("business", &"ก".repeat(160))).is_ok());
    assert!(validate_account_create(&input("business", &"😀".repeat(80))).is_ok());
    assert!(validate_account_create(&input("business", &"😀".repeat(81))).is_err());
    assert_eq!(
        validate_account_create(&input("business", "\u{feff}Acme\u{feff}"))
            .unwrap()
            .name,
        "Acme"
    );
    assert_eq!(
        validate_account_create(&input("business", "\u{0085}Acme\u{0085}"))
            .unwrap()
            .name,
        "\u{0085}Acme\u{0085}"
    );
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
        "command-1",
        &input("individual", "  Ada Lovelace  "),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(plan.name, "Ada Lovelace");
    let (company_id, record_id, created_by, saved, now) = repo.inserted.unwrap();
    assert_eq!(company_id, "company");
    assert_eq!(record_id, "account-command-1");
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
        "command-2",
        &input("business", "   "),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CRM_ACCOUNT_INPUT_INVALID");
    assert!(repo.inserted.is_none());
}

fn link_input() -> AccountLinkFinanceInput {
    serde_json::from_value(json!({
        "action": "link-finance", "accountId": "account-1",
        "customerId": "customer-1", "expectedVersion": 1,
        "evidence": "  Independent customer review  "
    }))
    .unwrap()
}

fn reviewer() -> AccountLinkFinanceActor {
    AccountLinkFinanceActor {
        user_id: "reviewer".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        can_review: true,
    }
}

struct LinkFake {
    linked: Option<(AccountLinkFinanceActor, AccountLinkFinancePlan, i64)>,
}

impl AccountLinkFinanceRepository for LinkFake {
    type Error = BusinessError;

    fn link_finance<'a>(
        &'a mut self,
        actor: &'a AccountLinkFinanceActor,
        plan: &'a AccountLinkFinancePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        self.linked = Some((actor.clone(), plan.clone(), now));
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn link_finance_trims_evidence_and_requires_independent_reviewer() {
    let plan = validate_account_link_finance(&reviewer(), &link_input()).unwrap();
    assert_eq!(plan.evidence, "Independent customer review");
    let mut no_review = reviewer();
    no_review.can_review = false;
    assert_eq!(
        validate_account_link_finance(&no_review, &link_input())
            .unwrap_err()
            .code,
        "CRM_ACCOUNT_REVIEW_REQUIRED"
    );
    let long = "😀".repeat(2_001);
    for evidence in ["  ", long.as_str()] {
        let mut input = link_input();
        input.evidence = evidence.into();
        assert_eq!(
            validate_account_link_finance(&reviewer(), &input)
                .unwrap_err()
                .code,
            "CRM_ACCOUNT_INPUT_INVALID"
        );
    }
    let mut wrong = link_input();
    wrong.action = "create-account".into();
    assert_eq!(
        validate_account_link_finance(&reviewer(), &wrong)
            .unwrap_err()
            .code,
        "CRM_ACCOUNT_ACTION_INVALID"
    );
    let mut version = link_input();
    version.expected_version = 0;
    assert_eq!(
        validate_account_link_finance(&reviewer(), &version)
            .unwrap_err()
            .code,
        "CRM_ACCOUNT_INPUT_INVALID"
    );
}

#[tokio::test]
async fn link_finance_application_delegates_the_retained_plan() {
    let mut repo = LinkFake { linked: None };
    let plan = link_account_finance(&reviewer(), &link_input(), &mut repo, NOW.get())
        .await
        .unwrap();
    assert_eq!(plan.account_id, "account-1");
    let (actor, saved, now) = repo.linked.unwrap();
    assert_eq!(actor.user_id, "reviewer");
    assert_eq!(saved, plan);
    assert_eq!(now, NOW.get());
}

fn native(value: serde_json::Value) -> serde_json::Value {
    let action = value.get("action").and_then(serde_json::Value::as_str).unwrap_or("");
    match action {
        "create-contact" => serde_json::from_value::<ContactCreateInput>(value)
            .ok()
            .and_then(|input| validate_contact_create(&input).ok())
            .map(|plan| json!(["create-contact", plan.account_id, plan.name, plan.detail]))
            .unwrap_or(serde_json::Value::Null),
        "link-guest" => serde_json::from_value::<GuestLinkInput>(value)
            .ok()
            .and_then(|input| validate_guest_link(&input).ok())
            .map(|plan| {
                json!(["link-guest", plan.contact_id, plan.guest_id, plan.property_id, plan.evidence, plan.sharing_evidence])
            })
            .unwrap_or(serde_json::Value::Null),
        "guest-sharing" => serde_json::from_value::<GuestSharingInput>(value)
            .ok()
            .and_then(|input| validate_guest_sharing(&input).ok())
            .map(|plan| {
                json!(["guest-sharing", plan.link_id, plan.expected_version, plan.state, plan.evidence])
            })
            .unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    }
}

#[test]
fn identity_family_matches_ts_zod_for_all_owner_complete_actions() {
    let cases = json!([
        {"action":"create-contact","accountId":"acc-1","name":"  Front Desk  ","detail":"\u{feff}  Reception\u{00a0}  "},
        {"action":"create-contact","accountId":"acc-1","name":"a","detail":"okay"},
        {"action":"create-contact","accountId":"","name":"Front Desk","detail":"okay"},
        {"action":"link-guest","contactId":"c","guestId":"g","propertyId":"p","evidence":"  linked  "},
        {"action":"link-guest","contactId":"c","guestId":"g","propertyId":"p","evidence":"linked","sharingEvidence":"  granted😀  "},
        {"action":"link-guest","contactId":"c","guestId":"g","propertyId":"p","evidence":"linked","sharingEvidence":"x"},
        {"action":"guest-sharing","linkId":"l-1","expectedVersion":4,"state":"revoked","evidence":"  audited  "},
        {"action":"guest-sharing","linkId":"l-1","expectedVersion":9007199254740992_i64,"state":"granted","evidence":"audited"},
        {"action":"guest-sharing","linkId":"l-1","expectedVersion":1,"state":"pending","evidence":"audited"},
        {"action":"delete-everything","linkId":"l-1"}
    ]);
    let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);process.stdout.write(JSON.stringify(cases.map(input=>{const r=m.crmIdentityCommand.safeParse(input);if(!r.success)return null;const d=r.data;switch(d.action){case'create-contact':return['create-contact',d.accountId,d.name,d.detail];case'link-guest':return['link-guest',d.contactId,d.guestId,d.propertyId,d.evidence,d.sharingEvidence===undefined?null:d.sharingEvidence];case'guest-sharing':return['guest-sharing',d.linkId,d.expectedVersion,d.state,d.evidence];default:return null;}})));"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let module = root.join("modules/crm/server/src/domain/identities.ts");
    let output = std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
        .current_dir(root)
        .args(["--conditions=react-server", "-e", script])
        .arg(module)
        .arg(cases.to_string())
        .output()
        .unwrap();
    assert!(output.status.success(), "actual CRM TS identity schema unavailable");
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let actual: Vec<serde_json::Value> = cases.as_array().unwrap().iter().map(|value| native(value.clone())).collect();
    assert_eq!(actual, expected);
}

#[derive(Default)]
struct IdentityFake {
    contact: Option<(String, String, String, ContactCreatePlan, i64)>,
    guest: Option<(String, String, String, String, GuestLinkPlan, i64)>,
    sharing: Option<(String, String, String, GuestSharingPlan, i64)>,
}

impl IdentityFamilyRepository for IdentityFake {
    type Error = BusinessError;

    fn insert_contact<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a ContactCreatePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        self.contact = Some((company_id.into(), record_id.into(), created_by.into(), plan.clone(), now));
        Box::pin(async { Ok(()) })
    }

    fn link_guest<'a>(
        &'a mut self,
        company_id: &'a str,
        link_id: &'a str,
        sharing_event_id: &'a str,
        created_by: &'a str,
        plan: &'a GuestLinkPlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        self.guest = Some((company_id.into(), link_id.into(), sharing_event_id.into(), created_by.into(), plan.clone(), now));
        Box::pin(async { Ok(()) })
    }

    fn change_guest_sharing<'a>(
        &'a mut self,
        company_id: &'a str,
        sharing_event_id: &'a str,
        created_by: &'a str,
        plan: &'a GuestSharingPlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        self.sharing = Some((company_id.into(), sharing_event_id.into(), created_by.into(), plan.clone(), now));
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn identity_family_application_delegates_all_actions() {
    let mut repo = IdentityFake::default();
    let contact = create_contact(
        "owner",
        "company",
        "key-contact",
        &serde_json::from_value(json!({
            "action":"create-contact","accountId":"acc-1","name":"  Front Desk  ","detail":"okay"
        }))
        .unwrap(),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(contact.name, "Front Desk");
    assert_eq!(repo.contact.as_ref().unwrap().1, "contact-key-contact");

    let guest = link_guest(
        "owner",
        "company",
        "key-guest",
        &serde_json::from_value(json!({
            "action":"link-guest","contactId":"c","guestId":"g","propertyId":"p","evidence":"linked","sharingEvidence":"granted"
        }))
        .unwrap(),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(guest.sharing_evidence.as_deref(), Some("granted"));
    assert_eq!(repo.guest.as_ref().unwrap().1, "guest-link-key-guest");
    assert_eq!(repo.guest.as_ref().unwrap().2, "sharing-key-guest");

    let sharing = change_guest_sharing(
        "owner",
        "company",
        "key-sharing",
        &serde_json::from_value(json!({
            "action":"guest-sharing","linkId":"l-1","expectedVersion":1,"state":"revoked","evidence":"audited"
        }))
        .unwrap(),
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(sharing.version, 2);
    assert_eq!(repo.sharing.as_ref().unwrap().1, "sharing-key-sharing");
}
