//! Native CRM activity create / complete / cancel owner unit facts.
//! Domain validation + application state gates against an in-memory fake.
//! No PostgreSQL, no purchase check, no HTTP. `reassign` is rejected here
//! by design (TS-owned cross-owner eligibility join).
use erpx_server_crm::public::activities_native::{
    ActivitiesFuture, ActivityActor, ActivityChangeInput, ActivityCreateInput, ActivityFacts,
    ActivityRepository, LeadFacts, change_activity, create_activity, validate_change,
    validate_create,
};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> ActivityActor {
    ActivityActor {
        user_id: "owner".into(),
        company_id: "company".into(),
        can_sales: true,
        can_manage: false,
        has_read: true,
        same_person_ids: vec![],
    }
}

fn create() -> ActivityCreateInput {
    serde_json::from_value(json!({
        "leadId": "lead-1", "expectedVersion": 1, "activityType": "call",
        "subject": "Call the customer", "details": "Discuss scope",
        "ownerId": "owner", "dueDate": "2026-11-20"
    }))
    .unwrap()
}

struct Fake {
    lead: Option<LeadFacts>,
    activity: Option<ActivityFacts>,
    inserted: Option<String>,
    updated: Option<(String, String)>,
}

impl Fake {
    fn new() -> Self {
        Self {
            lead: Some(LeadFacts {
                status: "open".into(),
                stage: None,
                version: 1,
            }),
            activity: Some(ActivityFacts {
                owner_id: "owner".into(),
                status: "open".into(),
                version: 1,
            }),
            inserted: None,
            updated: None,
        }
    }
}

impl ActivityRepository for Fake {
    type Error = BusinessError;

    fn lead<'a>(
        &'a mut self,
        _company_id: &'a str,
        _lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error> {
        let lead = self.lead.clone();
        Box::pin(async move { Ok(lead) })
    }

    fn activity<'a>(
        &'a mut self,
        _company_id: &'a str,
        _activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error> {
        let activity = self.activity.clone();
        Box::pin(async move { Ok(activity) })
    }

    fn insert_activity<'a>(
        &'a mut self,
        _company_id: &'a str,
        record_id: &'a str,
        _created_by: &'a str,
        _plan: &'a erpx_server_crm::public::activities_native::ActivityCreatePlan,
        _now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        self.inserted = Some(record_id.to_owned());
        Box::pin(async { Ok(()) })
    }

    fn update_activity<'a>(
        &'a mut self,
        record_id: &'a str,
        plan: &'a erpx_server_crm::public::activities_native::ActivityChangePlan,
        _now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        self.updated = Some((
            record_id.to_owned(),
            plan.status.clone(),
        ));
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn create_plan_trims_text() {
    let mut input = create();
    input.subject = "  Call the customer  ".into();
    let plan = validate_create(&actor(), &input).unwrap();
    assert_eq!(plan.subject, "Call the customer");
    assert_eq!(plan.activity_type, "call");
}

#[test]
fn create_rejects_bad_type_date_and_version() {
    let mut input = create();
    input.activity_type = "visit".into();
    assert_eq!(
        validate_create(&actor(), &input).unwrap_err().code,
        "CRM_ACTIVITY_TYPE_INVALID"
    );
    let mut input = create();
    input.due_date = "2026-02-29".into();
    assert!(validate_create(&actor(), &input).is_err());
    let mut input = create();
    input.expected_version = 0;
    assert!(validate_create(&actor(), &input).is_err());
}

#[test]
fn create_requires_sales_and_read() {
    let mut a = actor();
    a.can_sales = false;
    assert_eq!(
        validate_create(&a, &create()).unwrap_err().kind,
        FailureKind::Forbidden
    );
    let mut a = actor();
    a.has_read = false;
    assert_eq!(
        validate_create(&a, &create()).unwrap_err().code,
        "CRM_PERMISSION_REQUIRED"
    );
}

#[test]
fn complete_is_owner_only_cancel_allows_manager() {
    let complete = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
    };
    assert!(validate_change(&actor(), &complete, "owner", "open").is_ok());
    assert_eq!(
        validate_change(&actor(), &complete, "other", "open").unwrap_err().code,
        "CRM_ACTIVITY_OWNER_REQUIRED"
    );
    let mut manager = actor();
    manager.can_manage = true;
    manager.user_id = "manager".into();
    let cancel = ActivityChangeInput {
        action: "cancel".into(),
        expected_version: 1,
        reason: "No longer needed".into(),
    };
    assert_eq!(
        validate_change(&manager, &cancel, "other", "open")
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        validate_change(&actor(), &cancel, "other", "open").unwrap_err().code,
        "CRM_ACTIVITY_OWNER_REQUIRED"
    );
}

#[test]
fn reassign_and_closed_are_rejected() {
    let reassign = ActivityChangeInput {
        action: "reassign".into(),
        expected_version: 1,
        reason: "Needed".into(),
    };
    assert_eq!(
        validate_change(&actor(), &reassign, "owner", "open").unwrap_err().code,
        "CRM_ACTIVITY_REASSIGN_TS_OWNED"
    );
    let done = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
    };
    assert_eq!(
        validate_change(&actor(), &done, "owner", "completed").unwrap_err().code,
        "CRM_ACTIVITY_STATE_CHANGED"
    );
}

#[tokio::test]
async fn create_rejects_closed_or_version_mismatched_lead() {
    let mut repo = Fake::new();
    repo.lead = Some(LeadFacts {
        status: "disqualified".into(),
        stage: None,
        version: 1,
    });
    assert_eq!(
        create_activity(&actor(), &create(), &mut repo, NOW.get())
            .await
            .unwrap_err()
            .code,
        "CRM_ACTIVITY_SOURCE_CLOSED"
    );
    let mut repo = Fake::new();
    repo.lead = Some(LeadFacts {
        status: "open".into(),
        stage: None,
        version: 2,
    });
    assert!(create_activity(&actor(), &create(), &mut repo, NOW.get())
        .await
        .is_err());
    assert!(repo.inserted.is_none());
}

#[tokio::test]
async fn create_and_complete_round_trip_through_fake() {
    let mut repo = Fake::new();
    let plan = create_activity(&actor(), &create(), &mut repo, NOW.get())
        .await
        .unwrap();
    assert_eq!(plan.activity_type, "call");
    assert!(repo.inserted.as_deref().unwrap().starts_with("crm-activity-"));
    let complete = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
    };
    let updated = change_activity(&actor(), "crm-activity-key-1", &complete, &mut repo, NOW.get())
        .await
        .unwrap();
    assert_eq!(updated.status, "completed");
    assert_eq!(
        repo.updated,
        Some(("crm-activity-key-1".into(), "completed".into()))
    );
}
