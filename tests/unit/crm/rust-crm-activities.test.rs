//! Native CRM activity create / complete / cancel owner unit facts.
//! Domain validation + application state gates against an in-memory fake.
//! No PostgreSQL, no purchase check, no HTTP. `reassign` is rejected here
//! by design (TS-owned cross-owner eligibility join).
use erpx_server_crm::public::activities_native::{
    ActivitiesFuture, ActivityActor, ActivityChangeInput, ActivityCreateInput, ActivityFacts,
    ActivityRepository, LeadFacts, change_activity, create_activity, validate_change,
    validate_create, prepare_activity_reassign,
};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> ActivityActor {
    ActivityActor {
        user_id: "owner".into(),
        company_id: "company".into(), branch_id: None, department_id: None,
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

#[test]
fn activity_create_date_matches_iso_year_zero_and_leap_boundaries() {
    for date in ["0000-01-01", "0000-02-29", "2000-02-29", "9999-12-31"] {
        let mut input = create();
        input.due_date = date.into();
        assert!(validate_create(&actor(), &input).is_ok(), "{date}");
    }
    for date in ["0000-02-30", "1900-02-29"] {
        let mut input = create();
        input.due_date = date.into();
        assert!(validate_create(&actor(), &input).is_err(), "{date}");
    }
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
                due_date: "2026-11-20".into(),
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
        _actor: &'a ActivityActor,
        _lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error> {
        let lead = self.lead.clone();
        Box::pin(async move { Ok(lead) })
    }

    fn activity<'a>(
        &'a mut self,
        _actor: &'a ActivityActor,
        _activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error> {
        let activity = self.activity.clone();
        Box::pin(async move { Ok(activity) })
    }

    fn insert_activity<'a>(
        &'a mut self,
        _company_id: &'a str,
        record_id: &'a str,
        _command_key: &'a str,
        _created_by: &'a str,
        _plan: &'a erpx_server_crm::public::activities_native::ActivityCreatePlan,
        _input: &'a ActivityCreateInput,
        _now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        self.inserted = Some(record_id.to_owned());
        Box::pin(async { Ok(()) })
    }

    fn update_activity<'a>(
        &'a mut self,
        _company_id: &'a str,
        record_id: &'a str,
        _command_key: &'a str,
        _actor_id: &'a str,
        _record: &'a ActivityFacts,
        _input: &'a ActivityChangeInput,
        plan: &'a erpx_server_crm::public::activities_native::ActivityChangePlan,
        _now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        self.updated = Some((record_id.to_owned(), plan.status.clone()));
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
fn activity_json_accepts_zod_integer_forms_and_ignores_unknown_fields() {
    let input:ActivityCreateInput=serde_json::from_value(json!({"leadId":"lead-1","expectedVersion":1.0,"activityType":"call","subject":"Call","details":"Discuss","ownerId":"owner","dueDate":"2026-11-20","ignored":true})).unwrap();
    assert_eq!(input.expected_version,1);
    let change:ActivityChangeInput=serde_json::from_value(json!({"action":"complete","expectedVersion":2.0,"reason":"Done","extra":"discarded"})).unwrap();assert_eq!(change.expected_version,2);
    for version in [json!(1.5),json!(9007199254740992_u64)] {
        assert!(serde_json::from_value::<ActivityChangeInput>(json!({"action":"complete","expectedVersion":version,"reason":"Done"})).is_err());
    }
}

#[tokio::test]
async fn activity_access_and_state_precede_late_version_conflict() {
    let mut repo=Fake::new();repo.lead=Some(LeadFacts{status:"disqualified".into(),stage:None,version:2});
    assert_eq!(create_activity(&actor(),"key-closed",&create(),&mut repo,NOW.get()).await.unwrap_err().code,"CRM_ACTIVITY_SOURCE_CLOSED");
    let input=ActivityChangeInput{action:"complete".into(),expected_version:1,reason:"Done".into(), owner_id: None, due_date: None, };
    let mut no_access=actor();no_access.can_sales=false;let mut absent=Fake::new();absent.activity=None;
    assert_eq!(change_activity(&no_access,"missing","key-denied",&input,&mut absent,NOW.get()).await.unwrap_err().code,"PERMISSION_REQUIRED");
    for body in [json!({"action":"reassign","expectedVersion":1,"reason":"Needed"}), json!({"action":"reassign","expectedVersion":1,"reason":"Needed","ownerId":"","dueDate":"2026-11-20"}), json!({"action":"reassign","expectedVersion":1,"reason":"Needed","ownerId":"owner","dueDate":"2026-02-30"})] {
        let malformed: ActivityChangeInput=serde_json::from_value(body).unwrap();
        assert_eq!(change_activity(&no_access,"missing","key-invalid-reassign",&malformed,&mut absent,NOW.get()).await.unwrap_err().code,"VALIDATION_FAILED");
    }
    let valid_reassign:ActivityChangeInput=serde_json::from_value(json!({"action":"reassign","expectedVersion":1,"reason":"Needed","ownerId":"owner","dueDate":"2026-11-20"})).unwrap();
    assert_eq!(change_activity(&no_access,"missing","key-denied-reassign",&valid_reassign,&mut absent,NOW.get()).await.unwrap_err().code,"PERMISSION_REQUIRED");
    assert_eq!(change_activity(&actor(),"missing","key-valid-reassign",&valid_reassign,&mut absent,NOW.get()).await.unwrap_err().code,"CRM_ACTIVITY_REASSIGN_TS_OWNED");
    let invalid=ActivityChangeInput{reason:"no".into(),..input.clone()};
    assert_eq!(change_activity(&no_access,"missing","key-invalid",&invalid,&mut absent,NOW.get()).await.unwrap_err().code,"VALIDATION_FAILED");
    let mut invalid_create=create();invalid_create.subject="no".into();
    assert_eq!(create_activity(&no_access,"key-invalid-create",&invalid_create,&mut absent,NOW.get()).await.unwrap_err().code,"VALIDATION_FAILED");
    let mut closed=Fake::new();closed.activity=Some(ActivityFacts{owner_id:"other".into(),due_date:"2026-11-20".into(),status:"completed".into(),version:2});
    assert_eq!(change_activity(&actor(),"activity","key-foreign",&input,&mut closed,NOW.get()).await.unwrap_err().code,"CRM_ACTIVITY_OWNER_REQUIRED");
    closed.activity.as_mut().unwrap().owner_id="owner".into();
    assert_eq!(change_activity(&actor(),"activity","key-closed",&input,&mut closed,NOW.get()).await.unwrap_err().code,"CRM_ACTIVITY_STATE_CHANGED");
    assert!(closed.updated.is_none());
}

#[test]
fn text_validation_matches_zod_trim_and_unicode_boundaries() {
    let mut input = create();
    input.subject = format!("\u{feff}{}\u{feff}", "ก".repeat(200));
    assert_eq!(
        validate_create(&actor(), &input).unwrap().subject,
        "ก".repeat(200)
    );
    input.subject = "😀".repeat(2);
    assert!(validate_create(&actor(), &input).is_err());
    input.subject = "😀".repeat(3);
    assert!(validate_create(&actor(), &input).is_ok());
    input.subject = "😀".repeat(200);
    assert!(validate_create(&actor(), &input).is_ok());
    input.subject = "😀".repeat(201);
    assert!(validate_create(&actor(), &input).is_err());
    input.subject = "valid".into();
    input.details = "😀".repeat(2000);
    assert!(validate_create(&actor(), &input).is_ok());
    input.details = "😀".repeat(2001);
    assert!(validate_create(&actor(), &input).is_err());
    input.details = "valid".into();
    input.subject = "ab".into();
    assert!(validate_create(&actor(), &input).is_err());
    input.subject = "\u{0085}ab".into();
    assert!(validate_create(&actor(), &input).is_ok());
}

#[test]
fn activity_change_reason_uses_zod_unicode_boundaries() {
    let mut input=ActivityChangeInput {action:"complete".into(),expected_version:1,reason:"😀".repeat(2), owner_id: None, due_date: None, };
    assert!(validate_change(&actor(),&input,"owner","open").is_err());
    input.reason="😀".repeat(3);assert!(validate_change(&actor(),&input,"owner","open").is_ok());
    input.reason="😀".repeat(2000);assert!(validate_change(&actor(),&input,"owner","open").is_ok());
    input.reason="😀".repeat(2001);assert!(validate_change(&actor(),&input,"owner","open").is_err());
}

#[test]
fn create_rejects_bad_type_date_and_version() {
    let mut input = create();
    input.activity_type = "visit".into();
    assert_eq!(
        validate_create(&actor(), &input).unwrap_err().code,
        "VALIDATION_FAILED"
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
        "PERMISSION_REQUIRED"
    );
}

#[test]
fn complete_is_owner_only_cancel_allows_manager() {
    let complete = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
     owner_id: None, due_date: None, };
    assert!(validate_change(&actor(), &complete, "owner", "open").is_ok());
    assert_eq!(
        validate_change(&actor(), &complete, "other", "open")
            .unwrap_err()
            .code,
        "CRM_ACTIVITY_OWNER_REQUIRED"
    );
    let mut manager = actor();
    manager.can_manage = true;
    manager.user_id = "manager".into();
    let cancel = ActivityChangeInput {
        action: "cancel".into(),
        expected_version: 1,
        reason: "No longer needed".into(),
     owner_id: None, due_date: None, };
    assert_eq!(
        validate_change(&manager, &cancel, "other", "open")
            .unwrap()
            .status,
        "cancelled"
    );
    assert_eq!(
        validate_change(&actor(), &cancel, "other", "open")
            .unwrap_err()
            .code,
        "CRM_ACTIVITY_OWNER_REQUIRED"
    );
}

#[test]
fn reassign_intent_validates_schema_permissions_and_current_state_without_claiming_effects() {
    let input:ActivityChangeInput=serde_json::from_value(json!({"action":"reassign","expectedVersion":1,"ownerId":"next-owner","dueDate":"2026-11-20","reason":"  Handoff  "})).unwrap();
    let mut manager=actor(); manager.can_manage=true;
    let intent=prepare_activity_reassign(&manager,&input,"open").unwrap();
    assert_eq!((intent.expected_version,intent.owner_id.as_str(),intent.due_date.as_str(),intent.reason.as_str()),(1,"next-owner","2026-11-20","Handoff"));
    assert_eq!(prepare_activity_reassign(&actor(),&input,"completed").unwrap_err().code,"CRM_ACTIVITY_OWNER_REQUIRED");
    assert_eq!(prepare_activity_reassign(&manager,&input,"completed").unwrap_err().code,"CRM_ACTIVITY_STATE_CHANGED");
    let mut denied=manager.clone();denied.can_sales=false;
    assert_eq!(prepare_activity_reassign(&denied,&input,"open").unwrap_err().code,"PERMISSION_REQUIRED");
    let mut invalid=input.clone();invalid.owner_id=None;
    assert_eq!(prepare_activity_reassign(&denied,&invalid,"open").unwrap_err().code,"VALIDATION_FAILED");
    // The command path must remain closed until retained cross-owner proofs exist.
    assert_eq!(validate_change(&manager,&input,"owner","open").unwrap_err().code,"CRM_ACTIVITY_REASSIGN_TS_OWNED");
}

#[test]
fn reassign_and_closed_are_rejected() {
    let reassign = ActivityChangeInput {
        action: "reassign".into(),
        expected_version: 1,
        reason: "Needed".into(),
     owner_id: Some("owner".into()), due_date: Some("2026-11-20".into()), };
    assert_eq!(
        validate_change(&actor(), &reassign, "owner", "open")
            .unwrap_err()
            .code,
        "CRM_ACTIVITY_REASSIGN_TS_OWNED"
    );
    let done = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
     owner_id: None, due_date: None, };
    assert_eq!(
        validate_change(&actor(), &done, "owner", "completed")
            .unwrap_err()
            .code,
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
        create_activity(&actor(), "key-1", &create(), &mut repo, NOW.get())
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
    assert!(
        create_activity(&actor(), "key-1", &create(), &mut repo, NOW.get())
            .await
            .is_err()
    );
    assert!(repo.inserted.is_none());
}

#[tokio::test]
async fn create_and_complete_round_trip_through_fake() {
    let mut repo = Fake::new();
    let plan = create_activity(&actor(), "key-1", &create(), &mut repo, NOW.get())
        .await
        .unwrap();
    assert_eq!(plan.activity_type, "call");
    assert_eq!(repo.inserted.as_deref(), Some("crm-activity-key-1"));
    let complete = ActivityChangeInput {
        action: "complete".into(),
        expected_version: 1,
        reason: "Done".into(),
     owner_id: None, due_date: None, };
    let updated = change_activity(
        &actor(),
        "crm-activity-key-1",
        "key-2",
        &complete,
        &mut repo,
        NOW.get(),
    )
    .await
    .unwrap();
    assert_eq!(updated.status, "completed");
    assert_eq!(
        repo.updated,
        Some(("crm-activity-key-1".into(), "completed".into()))
    );
}

#[test]
fn reassign_wire_input_matches_actual_ts_schema_order_and_normalization() {
    use erpx_server_crm::public::activities_native::normalized_activity_reassign_input;
    let cases=serde_json::json!([
        {"reason":"  Review handoff  ","unknown":{"nested":true},"dueDate":"2026-12-05","ownerId":"new-owner","expectedVersion":1,"action":"reassign"},
        {"action":"reassign","expectedVersion":1.0,"ownerId":"ผู้รับ😀","dueDate":"2000-02-29","reason":"\u{feff}ส่งต่อ😀\u{00a0}"},
        {"action":"reassign","expectedVersion":9007199254740991_i64,"ownerId":"x","dueDate":"0000-02-29","reason":"good"},
        {"action":"reassign","expectedVersion":9007199254740992_i64,"ownerId":"x","dueDate":"2026-12-05","reason":"good"},
        {"action":"reassign","expectedVersion":1,"ownerId":"","dueDate":"2026-12-05","reason":"good"},
        {"action":"reassign","expectedVersion":1,"ownerId":"x","dueDate":"2026-02-29","reason":"good"},
        {"action":"reassign","expectedVersion":1,"ownerId":"x","dueDate":"2026-12-05","reason":" x "},
        {"action":"reassign","expectedVersion":1,"ownerId":null,"dueDate":"2026-12-05","reason":"good"},
        {"action":"reassign","expectedVersion":1,"dueDate":"2026-12-05","reason":"good"},
        {"action":"cancel","expectedVersion":1,"reason":"good"}
    ]);
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);process.stdout.write(JSON.stringify(cases.map(input=>{const r=m.crmActivityChangeSchema.safeParse(input);return r.success&&r.data.action==='reassign'?JSON.stringify(r.data):null})));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-crm/src/domain/activities.ts");
    let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).arg(cases.to_string()).output().unwrap();assert!(output.status.success(),"actual CRM TS schema unavailable");let expected:Vec<Option<String>>=serde_json::from_slice(&output.stdout).unwrap();
    for (index,value) in cases.as_array().unwrap().iter().enumerate(){let actual=serde_json::from_value::<ActivityChangeInput>(value.clone()).ok().and_then(|v|normalized_activity_reassign_input(&v).ok()).map(|v|serde_json::to_string(&v).unwrap());assert_eq!(actual,expected[index],"case {index}");}
}

#[test]
fn reassign_utf16_normalization_matches_real_ts_zod_for_surrogates() {
    use erpx_server_crm::public::activities_native::normalize_activity_reassign_utf16;
    fn units(v:&str)->Vec<u16>{v.encode_utf16().collect()}
    let action=units("reassign");let date=units("2026-12-05");
    let cases:Vec<(Vec<u16>,i64,Vec<u16>,Vec<u16>,Vec<u16>)>=vec![
        (action.clone(),1,vec![0xd800],date.clone(),vec![0xfeff,0xd800,97,98,0x00a0]),
        (action.clone(),1,vec![0xdc00],date.clone(),vec![0xd83d,0xde00,97,98]),
        (action.clone(),1,vec![97],date.clone(),vec![0xd83d,0xde00,97]),
        (action.clone(),1,vec![97],date.clone(),vec![0xd800,0xdc00,97]),
        (action.clone(),1,vec![0xd800,0xdc00],date.clone(),vec![0xd800,97,98]),
        (action.clone(),1,vec![],date.clone(),units("valid reason")),
        (action.clone(),0,vec![97],date.clone(),units("valid reason")),
        (action.clone(),1,vec![97],units("2026-02-29"),units("valid reason")),
        (units("cancel"),1,vec![97],date.clone(),units("valid reason")),
        (action.clone(),1,vec![97],date.clone(),vec![97;2000]),
        (action,1,vec![97],date,vec![97;2001]),
    ];
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const cases=JSON.parse(process.argv[2]);const str=a=>String.fromCharCode(...a),units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));process.stdout.write(JSON.stringify(cases.map(([action,version,owner,due,reason])=>{const r=m.crmActivityChangeSchema.safeParse({action:str(action),expectedVersion:version,ownerId:str(owner),dueDate:str(due),reason:str(reason),ignored:'strip'});return r.success&&r.data.action==='reassign'?{version:r.data.expectedVersion,owner:units(r.data.ownerId),due:units(r.data.dueDate),reason:units(r.data.reason)}:null})));"#;
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let module=root.join("packages/server-crm/src/domain/activities.ts");
    let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(module).arg(serde_json::to_string(&cases).unwrap()).output().unwrap();assert!(output.status.success(),"actual CRM TS schema unavailable");let expected:Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
    for (i,(action,version,owner,due,reason)) in cases.iter().enumerate(){let actual=normalize_activity_reassign_utf16(action,*version,owner,due,reason).ok().map(|v|serde_json::json!({"version":v.expected_version(),"owner":v.owner_id(),"due":v.due_date(),"reason":v.reason()})).unwrap_or(serde_json::Value::Null);assert_eq!(actual,expected[i],"case {i}");}
}
