//! Native People workforce command policy tests.
//!
//! These are owner unit facts: exact idempotency replay, version CAS,
//! independent compensation review, lifecycle gates and the private person
//! detail projection. PostgreSQL transaction behaviour and the composed
//! Organization/Identity/Finance ports are proven by the sibling integration
//! target.
use erpx_server_people_payroll::public::workforce::{
    CompensationMoneyPort, CompensationMoneyRequest, CompensationRow, CompensationTransitionInput,
    EngagementRow, EngagementTransitionInput, NoCompensationMoneyPort, NoStaffPlacementPort,
    PersonAccountLinkPort, PersonAccountLinkRequest, PersonStatusFacts, PersonUpdateInput,
    StaffPlacementPort, StaffPlacementRequest, StoredWorkforceReceipt, WorkforceCommandActor,
    WorkforceCommandFuture, WorkforceCommandRepository, WorkforceCompensationSummary,
    WorkforceLedgerEntry, WorkforcePersonRow, create_compensation, person_detail, save_person,
    set_person_status, transition_compensation, transition_engagement,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "people".into(),
        permissions: vec!["people.manage".into(), "people.compensation.approve".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn profile() -> Value {
    json!({
        "legalName": "Somchai Jaidee",
        "email": "somchai@example.com",
        "phone": "0800000000",
        "countryCode": "TH",
        "branchId": "branch",
        "jobTitle": "Engineer",
        "workerType": "employee",
        "employmentType": "salaried",
        "startDate": "2026-01-01",
        "currency": "THB",
    })
}

fn person_row() -> WorkforcePersonRow {
    WorkforcePersonRow {
        id: "person".into(),
        company_id: "company".into(),
        user_id: Some("someone-else".into()),
        legal_name: "Somchai Jaidee".into(),
        preferred_name: None,
        email: Some("somchai@example.com".into()),
        phone: Some("0800000000".into()),
        work_email: None,
        work_phone: None,
        country_code: "TH".into(),
        status: "active".into(),
        version: 1,
        created_by: "owner".into(),
        engagement_id: Some("engagement".into()),
        engagement_version: Some(1),
        branch_id: Some("branch".into()),
        department_id: None,
        job_title: Some("Engineer".into()),
        worker_type: Some("employee".into()),
        employment_type: Some("salaried".into()),
        start_date: Some("2026-01-01".into()),
        end_date: None,
        currency: Some("THB".into()),
        engagement_status: Some("active".into()),
        manager_person_id: None,
        pay_group_id: None,
        position_id: None,
        level_id: Some("level".into()),
        level_name: Some("Engineer".into()),
        level_number: Some(3),
    }
}

fn engagement_row() -> EngagementRow {
    EngagementRow {
        id: "engagement".into(),
        person_id: "person".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        worker_type: "employee".into(),
        employment_type: "salaried".into(),
        status: "onboarding".into(),
        start_date: "2026-01-01".into(),
        end_date: None,
        manager_person_id: None,
        job_title: "Engineer".into(),
        currency: "THB".into(),
        pay_group_id: None,
        version: 1,
        person_user_id: Some("someone-else".into()),
        person_status: "active".into(),
        level_id: Some("level".into()),
        position_id: None,
    }
}

fn compensation_row() -> CompensationRow {
    CompensationRow {
        id: "compensation".into(),
        engagement_id: "engagement".into(),
        effective_from: "2026-01-01".into(),
        effective_to: None,
        currency: "THB".into(),
        status: "draft".into(),
        version: 1,
        created_by: "owner".into(),
        branch_id: Some("branch".into()),
        department_id: None,
    }
}

#[derive(Default)]
struct FakeRepo {
    receipts: BTreeMap<String, StoredWorkforceReceipt>,
    ledger: Vec<String>,
    person: Option<WorkforcePersonRow>,
    status_facts: Option<PersonStatusFacts>,
    engagement: Option<EngagementRow>,
    compensation: Option<CompensationRow>,
    summary: Option<WorkforceCompensationSummary>,
    email_available: bool,
    evaluation_draft: bool,
    overlap: bool,
    deny_ports: bool,
    create_person_calls: usize,
    update_person_calls: usize,
    status_calls: usize,
    engagement_create_calls: usize,
    engagement_transition_calls: usize,
    compensation_create_calls: usize,
    compensation_transition_calls: usize,
    money_amounts: Vec<(String, i64)>,
}

impl FakeRepo {
    fn with_person() -> Self {
        Self {
            person: Some(person_row()),
            status_facts: Some(PersonStatusFacts {
                status: "active".into(),
                version: 1,
                user_id: Some("someone-else".into()),
                engagement_status: Some("active".into()),
                has_non_ended_engagements: true,
                has_open_access_cutoff: false,
            }),
            engagement: Some(engagement_row()),
            compensation: Some(compensation_row()),
            summary: Some(WorkforceCompensationSummary {
                base_amount_minor: 1_000_000,
                hourly_rate_minor: 0,
                daily_rate_minor: 0,
                standard_daily_hours: 8.0,
                pay_frequency: "monthly".into(),
                effective_from: "2026-01-01".into(),
                effective_to: None,
                currency: "THB".into(),
                status: "approved".into(),
                version: 1,
            }),
            email_available: true,
            ..Self::default()
        }
    }
}

impl WorkforceCommandRepository for FakeRepo {
    type Error = BusinessError;

    fn lock_command<'b>(&'b mut self, _: &'b str) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn request_hash<'b>(
        &'b mut self,
        identity: &'b str,
    ) -> WorkforceCommandFuture<'b, String, Self::Error> {
        Box::pin(async move {
            let mut hash = 0xcbf29ce484222325_u64;
            for byte in identity.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            Ok(format!("{hash:064x}"))
        })
    }
    fn receipt<'b>(
        &'b mut self,
        key: &'b str,
    ) -> WorkforceCommandFuture<'b, Option<StoredWorkforceReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }
    fn ledger<'b>(
        &'b mut self,
        entry: &'b WorkforceLedgerEntry<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ledger.push(entry.operation.to_owned());
            self.receipts.insert(
                entry.key.to_owned(),
                StoredWorkforceReceipt {
                    request_hash: entry.request_hash.to_owned(),
                    result: Some(entry.result.clone()),
                },
            );
            Ok(())
        })
    }
    fn person<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
        _: Option<&'b str>,
        _: Option<&'b str>,
        _: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<WorkforcePersonRow>, Self::Error> {
        Box::pin(async move { Ok(self.person.clone()) })
    }
    fn person_exists<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move { Ok(self.person.is_some()) })
    }
    fn person_status_facts<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
    ) -> WorkforceCommandFuture<'b, Option<PersonStatusFacts>, Self::Error> {
        Box::pin(async move { Ok(self.status_facts.clone()) })
    }
    fn email_available<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
        _: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move { Ok(self.email_available) })
    }
    fn create_person<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::PersonInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.create_person_calls += 1;
            Ok(())
        })
    }
    fn create_person_record<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::PersonRecordInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn update_person<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::PersonUpdate<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.update_person_calls += 1;
            Ok(())
        })
    }
    fn set_person_status<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::PersonStatusWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.status_calls += 1;
            Ok(())
        })
    }
    fn engagement<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
        _: Option<&'b str>,
        _: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<EngagementRow>, Self::Error> {
        Box::pin(async move { Ok(self.engagement.clone()) })
    }
    fn create_engagement<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::EngagementInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.engagement_create_calls += 1;
            Ok(())
        })
    }
    fn transition_engagement<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::EngagementTransitionWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.engagement_transition_calls += 1;
            Ok(())
        })
    }
    fn compensation<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b str,
        _: Option<&'b str>,
        _: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, Option<CompensationRow>, Self::Error> {
        Box::pin(async move { Ok(self.compensation.clone()) })
    }
    fn compensation_evaluation_draft<'b>(
        &'b mut self,
        _: &'b str,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move { Ok(self.evaluation_draft) })
    }
    fn overlapping_compensation<'b>(
        &'b mut self,
        _: &'b str,
        _: &'b str,
        _: &'b str,
        _: Option<&'b str>,
    ) -> WorkforceCommandFuture<'b, bool, Self::Error> {
        Box::pin(async move { Ok(self.overlap) })
    }
    fn create_compensation<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::CompensationInsert<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.compensation_create_calls += 1;
            Ok(())
        })
    }
    fn transition_compensation<'b>(
        &'b mut self,
        _: &'b CompanyId,
        _: &'b erpx_server_people_payroll::public::workforce::CompensationTransitionWrite<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.compensation_transition_calls += 1;
            Ok(())
        })
    }
    fn person_compensation<'b>(
        &'b mut self,
        _: &'b str,
    ) -> WorkforceCommandFuture<'b, Option<WorkforceCompensationSummary>, Self::Error> {
        Box::pin(async move { Ok(self.summary.clone()) })
    }
    fn apply_placement<'b, P: StaffPlacementPort>(
        &'b mut self,
        _: &'b mut P,
        _: &'b StaffPlacementRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            if self.deny_ports {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "PEOPLE_STAFF_PLACEMENT_PORT_UNAVAILABLE",
                    "The Organization staff placement port is unavailable.",
                ));
            }
            Ok(())
        })
    }
    fn verify_account_link<'b, A: PersonAccountLinkPort>(
        &'b mut self,
        _: &'b mut A,
        _: &'b PersonAccountLinkRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            if self.deny_ports {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "PEOPLE_ACCOUNT_LINK_PORT_UNAVAILABLE",
                    "The Identity account link port is unavailable.",
                ));
            }
            Ok(())
        })
    }
    fn revoke_company_access<'b, A: PersonAccountLinkPort>(
        &'b mut self,
        _: &'b mut A,
        _: &'b CompanyId,
        _: &'b str,
        _: &'b str,
        _: i64,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            if self.deny_ports {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "PEOPLE_ACCOUNT_LINK_PORT_UNAVAILABLE",
                    "The Identity account link port is unavailable.",
                ));
            }
            Ok(())
        })
    }
    fn snapshot_compensation_money<'b, M: CompensationMoneyPort>(
        &'b mut self,
        _: &'b mut M,
        request: &'b CompensationMoneyRequest<'b>,
    ) -> WorkforceCommandFuture<'b, (), Self::Error> {
        Box::pin(async move {
            if self.deny_ports {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "PEOPLE_COMPENSATION_MONEY_PORT_UNAVAILABLE",
                    "The Finance money snapshot port is unavailable.",
                ));
            }
            self.money_amounts = request.amounts.to_vec();
            Ok(())
        })
    }
}

/// These stand-ins never reach SQL because the fake repository owns the bridge.
struct NoPlacement;
impl StaffPlacementPort for NoPlacement {
    fn apply<'a>(
        &'a mut self,
        _: &'a mut PgConnection,
        _: &'a StaffPlacementRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}
struct NoLink;
impl PersonAccountLinkPort for NoLink {
    fn verify<'a>(
        &'a mut self,
        _: &'a mut PgConnection,
        _: &'a PersonAccountLinkRequest<'a>,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
    fn revoke_company_access<'a>(
        &'a mut self,
        _: &'a mut PgConnection,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> WorkforceCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn create_person_is_idempotent_and_ledgered_once() {
    let mut repo = FakeRepo::with_person();
    let mut placement = NoPlacement;
    let mut link = NoLink;
    let first = save_person(
        &actor(),
        None,
        &profile(),
        &key("person-create-1"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap();
    let second = save_person(
        &actor(),
        None,
        &profile(),
        &key("person-create-1"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(first.id, "person-person-create-1");
    assert_eq!(first.id, second.id);
    assert_eq!(repo.create_person_calls, 1);
    assert_eq!(repo.ledger, vec!["people.directory.create"]);
}

#[tokio::test]
async fn update_requires_the_current_person_and_engagement_versions() {
    let mut repo = FakeRepo::with_person();
    let mut placement = NoPlacement;
    let mut link = NoLink;
    let mut raw = profile();
    raw["expectedVersion"] = json!(1);
    raw["expectedEngagementVersion"] = json!(2);
    let error = save_person(
        &actor(),
        Some("person"),
        &raw,
        &key("person-update-1"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "ENGAGEMENT_CHANGED");
    raw["expectedEngagementVersion"] = json!(1);
    let result = save_person(
        &actor(),
        Some("person"),
        &raw,
        &key("person-update-2"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.version, 2);
    assert_eq!(repo.update_person_calls, 1);
}

#[tokio::test]
async fn detail_redacts_private_contact_without_compensation_access() {
    let mut repo = FakeRepo::with_person();
    let mut manager = actor();
    manager.permissions = vec!["people.manage".into()];
    let detail = person_detail(&manager, "person", None, &mut repo, NOW)
        .await
        .unwrap();
    assert!(!detail.private_access);
    assert!(detail.phone.is_none());
    assert!(detail.compensation.is_none());
    let mut reader = actor();
    reader.permissions = vec!["people.compensation.read".into()];
    let detail = person_detail(&reader, "person", None, &mut repo, NOW)
        .await
        .unwrap();
    assert!(detail.private_access);
    assert_eq!(detail.phone.as_deref(), Some("0800000000"));
    assert_eq!(
        detail.compensation.as_ref().unwrap().base_amount_minor,
        1_000_000
    );
}

#[tokio::test]
async fn status_keeps_the_retained_lifecycle_gate() {
    let mut repo = FakeRepo::with_person();
    let mut placement = NoPlacement;
    let mut link = NoLink;
    let mut non_beta = actor();
    non_beta.beta = false;
    let error = set_person_status(
        &non_beta,
        "person",
        &json!({"status":"inactive","expectedVersion":1}),
        &key("person-status-1"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LIFECYCLE_REQUIRED");
    let result = set_person_status(
        &actor(),
        "person",
        &json!({"status":"inactive","expectedVersion":1}),
        &key("person-status-2"),
        &mut repo,
        &mut placement,
        &mut link,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "inactive");
    assert_eq!(repo.status_calls, 1);
}

#[tokio::test]
async fn compensation_requires_an_independent_reviewer_and_current_version() {
    let mut repo = FakeRepo::with_person();
    let error = transition_compensation(
        &actor(),
        "compensation",
        &json!({"status":"approved","expectedVersion":1}),
        &key("comp-approve-1"),
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SELF_REVIEW_FORBIDDEN");
    let mut reviewer = actor();
    reviewer.user_id = "reviewer".into();
    let result = transition_compensation(
        &reviewer,
        "compensation",
        &json!({"status":"approved","expectedVersion":1}),
        &key("comp-approve-2"),
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.status, "approved");
    assert_eq!(repo.compensation_transition_calls, 1);
}

#[tokio::test]
async fn create_compensation_reaches_the_money_port_with_every_amount() {
    let mut repo = FakeRepo::with_person();
    let mut money = NoCompensationMoneyPort;
    let mut raw = json!({
        "engagementId": "engagement",
        "effectiveFrom": "2026-01-01",
        "payFrequency": "monthly",
        "currency": "THB",
        "baseAmount": "10000.00",
        "hourlyRate": "50.50",
        "dailyRate": "400.00",
        "components": [
            {"code":"BONUS","label":"Bonus","kind":"earning","amount":"1000.00"}
        ]
    });
    let result = create_compensation(
        &actor(),
        &raw,
        &key("comp-create-1"),
        &mut repo,
        &mut money,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.id, "comp-comp-create-1");
    assert_eq!(repo.compensation_create_calls, 1);
    assert_eq!(
        repo.money_amounts,
        vec![
            ("base".to_owned(), 1_000_000),
            ("hourly_rate".to_owned(), 5_050),
            ("daily_rate".to_owned(), 40_000),
            ("component:BONUS".to_owned(), 100_000),
        ]
    );
    repo.engagement = None;
    raw["engagementId"] = json!("missing");
    assert_eq!(
        create_compensation(
            &actor(),
            &raw,
            &key("comp-create-2"),
            &mut repo,
            &mut money,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "ENGAGEMENT_NOT_FOUND"
    );
    let mut deny = FakeRepo::with_person();
    deny.deny_ports = true;
    assert_eq!(
        create_compensation(
            &actor(),
            &json!({
                "engagementId":"engagement",
                "effectiveFrom":"2026-01-01",
                "payFrequency":"monthly",
                "currency":"THB"
            }),
            &key("comp-create-3"),
            &mut deny,
            &mut NoCompensationMoneyPort,
            NOW,
        )
        .await
        .unwrap_err()
        .code,
        "PEOPLE_COMPENSATION_MONEY_PORT_UNAVAILABLE"
    );
}

#[tokio::test]
async fn activation_and_compensation_ports_fail_closed() {
    let mut repo = FakeRepo::with_person();
    repo.deny_ports = true;
    let mut placement = NoStaffPlacementPort;
    let error = transition_engagement(
        &actor(),
        "engagement",
        &json!({"status":"active","expectedVersion":1}),
        &key("engagement-activate-1"),
        &mut repo,
        &mut placement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PEOPLE_STAFF_PLACEMENT_PORT_UNAVAILABLE");
    assert_eq!(repo.engagement_transition_calls, 0);
}

#[tokio::test]
async fn engagement_transition_plan_rejects_same_status() {
    let mut repo = FakeRepo::with_person();
    let mut row = engagement_row();
    row.status = "active".into();
    repo.engagement = Some(row);
    let mut placement = NoPlacement;
    let error = transition_engagement(
        &actor(),
        "engagement",
        &json!({"status":"active","expectedVersion":1}),
        &key("engagement-activate-2"),
        &mut repo,
        &mut placement,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INVALID_ENGAGEMENT_TRANSITION");
}

#[tokio::test]
async fn command_inputs_are_strict() {
    assert!(
        CompensationTransitionInput::parse(&json!({"status":"draft","expectedVersion":1})).is_err()
    );
    assert!(PersonUpdateInput::parse(&json!({})).is_err());
    assert!(
        EngagementTransitionInput::parse(&json!({"status":"active","expectedVersion":0})).is_err()
    );
}
