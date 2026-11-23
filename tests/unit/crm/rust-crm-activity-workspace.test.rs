//! Unit facts for the native CRM activity workspace read port.
//!
//! SQL parity is asserted against an independent reconstruction of the retained
//! TypeScript reader statements: the TS templates (with `?` markers and the
//! closed projection templates) are assembled here and positional markers are
//! assigned in bind order, then compared byte-for-byte with the adapter consts.
//! View/work rules and the repository/directory use cases are exercised against
//! in-memory fakes. No PostgreSQL, composition or purchase check here.
use erpx_server_crm::public::activity_workspace::{
    ACTIVITIES_SQL, ActivityActor, ActivityWorkspaceDirectory, ActivityWorkspaceFuture,
    ActivityWorkspaceRepository, CrmActivityRow, CrmLeadRow, HISTORY_SQL, LEAD_SQL,
    RECIPIENTS_SQL, WORK_ROWS_SQL, activity_can_change, activity_work, activity_workspace,
    due_date_millis, effective_manager, work_items, workspace_view,
};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};

// --- Independent transcription of the retained TypeScript reader SQL. ---
const TS_SCOPE: &str =
    "l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)";
const TS_ASSIGNMENTS: &str = r#"SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM assignments"#;
const TS_HR_PEOPLE: &str = r#"SELECT "id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone" FROM hr_people"#;
const TS_HR_ENGAGEMENTS: &str = r#"SELECT "id","person_id","company_id","branch_id","department_id","worker_type","employment_type","status","start_date","end_date","manager_person_id","job_title","currency","version","created_by","created_at","updated_at","pay_group_id","level_id","position_id","self_service_top_level" FROM hr_engagements"#;
const TS_ELIGIBLE: &str = concat!(
    "EXISTS(SELECT 1 FROM (${assignments}) a JOIN (${hr_people}) p ON p.user_id=a.user_id AND p.company_id=a.company_id ",
    "JOIN (${hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id ",
    "WHERE a.company_id=o.company_id AND a.user_id=c.owner_id ",
    "AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ",
    "AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ",
    "AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') ",
    "AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) ",
    "OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) ",
    "AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ",
    "AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') ",
    "AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))"
);

fn expand(sql: &str) -> String {
    sql.replace("${assignments}", TS_ASSIGNMENTS)
        .replace("${hr_people}", TS_HR_PEOPLE)
        .replace("${hr_engagements}", TS_HR_ENGAGEMENTS)
        .replace("${scope}", TS_SCOPE)
        .replace("${filter}", "true")
}

/// `?` -> `$n` in bind order, the only mechanical change sqlx requires.
fn markers(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut index = 0;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            out.push('$');
            out.push_str(&index.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

fn expected(sql: &str, eligible_alias: Option<&str>) -> String {
    let expanded = expand(sql);
    let bound = match eligible_alias {
        Some(owner) => expand(TS_ELIGIBLE)
            .replace("o.", "l.")
            .replace("c.owner_id", owner)
            .pipe(|eligible| expanded.replace("${eligible}", &eligible)),
        None => expanded,
    };
    markers(&bound)
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

#[test]
fn adapter_sql_is_byte_identical_to_the_retained_ts_reader() {
    let lead = concat!(
        "SELECT l.id,l.company_id,l.branch_id,l.department_id,l.title,l.status,l.version,v.stage ",
        "FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id ",
        "WHERE l.id=? AND ${scope} AND ${filter}"
    );
    assert_eq!(LEAD_SQL, expected(lead, None));

    let activities = concat!(
        "SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,",
        "(SELECT legal_name FROM (${hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,",
        "CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible ",
        "FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id ",
        "WHERE x.company_id=? AND x.lead_id=? AND ${scope} AND ${filter} ORDER BY x.due_date,x.created_at,x.id"
    );
    assert_eq!(ACTIVITIES_SQL, expected(activities, Some("l.owner_id")));

    let work = concat!(
        "SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,",
        "(SELECT legal_name FROM (${hr_people}) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,",
        "CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible ",
        "FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id ",
        "WHERE ${scope} AND ${filter} ORDER BY x.due_date,x.updated_at DESC"
    );
    assert_eq!(WORK_ROWS_SQL, expected(work, Some("l.owner_id")));

    let history = "SELECT e.* FROM crm_lead_activity_events e JOIN crm_lead_activities x ON x.id=e.activity_id WHERE x.company_id=? AND x.lead_id=? ORDER BY e.created_at DESC,e.version DESC";
    assert_eq!(HISTORY_SQL, markers(history));

    let recipients = concat!(
        "SELECT DISTINCT a.user_id id,p.legal_name name FROM (${assignments}) a ",
        "JOIN (${hr_people}) p ON p.user_id=a.user_id AND p.company_id=a.company_id ",
        "JOIN crm_leads l ON l.id=? WHERE a.company_id=? AND ${eligible} ORDER BY p.legal_name"
    );
    assert_eq!(RECIPIENTS_SQL, expected(recipients, Some("a.user_id")));
}

// --- Domain rule facts. ---

fn actor() -> ActivityActor {
    ActivityActor {
        user_id: "owner".into(),
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        can_sales: true,
        can_manage: false,
        has_read: true,
        same_person_ids: vec![],
    }
}

fn lead() -> CrmLeadRow {
    CrmLeadRow {
        id: "lead".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        title: "New lead".into(),
        status: "open".into(),
        stage: None,
        version: 1,
    }
}

fn activity() -> CrmActivityRow {
    CrmActivityRow {
        id: "activity".into(),
        company_id: "company".into(),
        lead_id: "lead".into(),
        activity_type: "call".into(),
        subject: "Call the customer".into(),
        details: "Discuss scope".into(),
        owner_id: "owner".into(),
        due_date: "2026-11-20".into(),
        status: "open".into(),
        outcome: None,
        version: 1,
        created_by: "owner".into(),
        created_at: 1,
        updated_at: 2,
        lead_title: "New lead".into(),
        lead_status: "open".into(),
        lead_stage: None,
        branch_id: "branch".into(),
        department_id: None,
        owner_name: Some("Owner Name".into()),
        owner_eligible: 1,
    }
}

#[test]
fn due_date_parsing_matches_js_epoch_milliseconds() {
    assert_eq!(due_date_millis("1970-01-01"), 0);
    assert_eq!(due_date_millis("2026-11-20"), 1_795_132_800_000);
    assert_eq!(due_date_millis("2000-02-29"), 951_782_400_000);
    assert_eq!(due_date_millis("0000-01-01"), -62_167_219_200_000);
    assert_eq!(due_date_millis("not-a-date"), 0);
}

#[test]
fn workspace_flags_follow_sales_manager_and_lead_state() {
    let view = workspace_view(&lead(), &[activity()], &actor());
    assert!(view.can_create);
    assert!(!view.can_manage);
    let item = &view.items[0];
    assert!(item.owner_eligible_flag);
    assert!(item.can_complete);
    assert!(!item.can_manage);
    assert!(item.can_change);
    assert_eq!(view.lead.version, 1);

    let mut disqualified = lead();
    disqualified.status = "disqualified".into();
    assert!(!workspace_view(&disqualified, &[], &actor()).can_create);
    let mut won = lead();
    won.stage = Some("won".into());
    assert!(!workspace_view(&won, &[], &actor()).can_create);

    let mut manager = actor();
    manager.can_manage = true;
    assert!(effective_manager(&manager));
    let managed = workspace_view(&lead(), &[activity()], &manager);
    assert!(managed.can_manage);
    assert!(managed.items[0].can_manage);

    let mut no_sales = actor();
    no_sales.can_sales = false;
    let denied = workspace_view(&lead(), &[activity()], &no_sales);
    assert!(!denied.can_create);
    assert!(!denied.items[0].can_complete);
    assert!(!denied.items[0].can_change);
}

#[test]
fn can_change_uses_same_person_directory_or_manager() {
    let mut other = activity();
    other.created_by = "creator".into();
    other.owner_id = "someone-else".into();
    let mut a = actor();
    a.same_person_ids = vec!["creator".into()];
    assert!(activity_can_change(&other, &a, false));
    a.same_person_ids = vec![];
    assert!(!activity_can_change(&other, &a, false));
    assert!(activity_can_change(&other, &a, true));
    other.status = "completed".into();
    assert!(!activity_can_change(&other, &a, true));
}

#[test]
fn work_projection_carries_queue_blocker_and_relationships() {
    let mut rows = vec![activity()];
    let mut ineligible = activity();
    ineligible.id = "ineligible".into();
    ineligible.owner_eligible = 0;
    ineligible.owner_name = None;
    rows.push(ineligible);
    let mut completed = activity();
    completed.id = "completed".into();
    completed.status = "completed".into();
    rows.push(completed);

    let items = work_items(&rows, &actor());
    assert_eq!(items[0].queue, "todo");
    assert_eq!(items[0].relationships, vec!["assigned"]);
    assert_eq!(items[0].responsible_queue, "Owner Name");
    assert_eq!(items[0].due_at, due_date_millis("2026-11-20"));
    assert!(items[0].blocker.is_none());

    assert_eq!(items[1].queue, "waiting");
    assert_eq!(items[1].responsible_queue, "Assigned salesperson");
    assert_eq!(
        items[1].blocker.as_deref(),
        Some("Assigned owner is no longer eligible; a sales manager must reassign with a reason")
    );

    assert_eq!(items[2].queue, "completed");
    assert_eq!(items[2].next_action, "Activity outcome recorded");
    assert!(items[2].blocker.is_none());

    let value = serde_json::to_value(&items[0]).unwrap();
    assert_eq!(value["type"], "crm_activity");
    assert_eq!(value["updatedAt"], 2);
    assert_eq!(value["responsibleQueue"], "Owner Name");
    assert_eq!(value["dueAt"], due_date_millis("2026-11-20"));
}

// --- Application use cases against in-memory fakes. ---

struct FakeRepo {
    present: bool,
    rows: Vec<CrmActivityRow>,
}

impl ActivityWorkspaceRepository for FakeRepo {
    type Error = BusinessError;

    fn lead<'a>(
        &'a mut self,
        _actor: &'a ActivityActor,
        _lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Option<CrmLeadRow>, Self::Error> {
        let row = self.present.then(lead);
        Box::pin(async move { Ok(row) })
    }

    fn activities<'a>(
        &'a mut self,
        _actor: &'a ActivityActor,
        _lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error> {
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }

    fn history<'a>(
        &'a mut self,
        _actor: &'a ActivityActor,
        _lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<Value>, Self::Error> {
        Box::pin(async move { Ok(vec![json!({"action": "create", "version": 1})]) })
    }

    fn work_rows<'a>(
        &'a mut self,
        _actor: &'a ActivityActor,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error> {
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

struct FakeDirectory;

impl ActivityWorkspaceDirectory for FakeDirectory {
    type Error = BusinessError;

    fn recipients<'a>(
        &'a mut self,
        _company_id: &'a str,
        _lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<erpx_server_crm::public::activity_workspace::ActivityRecipient>, Self::Error> {
        Box::pin(async move {
            Ok(vec![erpx_server_crm::public::activity_workspace::ActivityRecipient {
                id: "owner".into(),
                name: "Owner Name".into(),
            }])
        })
    }
}

#[tokio::test]
async fn workspace_and_work_use_cases_enforce_read_and_scope() {
    let mut repo = FakeRepo {
        present: true,
        rows: vec![activity()],
    };
    let mut directory = FakeDirectory;
    let data = activity_workspace(&actor(), "lead", &mut repo, &mut directory)
        .await
        .unwrap();
    assert_eq!(data.view.items.len(), 1);
    assert_eq!(data.recipients[0].name, "Owner Name");
    assert_eq!(data.history[0]["action"], "create");
    assert_eq!(serde_json::to_value(&data).unwrap()["canCreate"], true);

    assert_eq!(activity_work(&actor(), &mut repo).await.unwrap()[0].queue, "todo");

    let mut no_read = actor();
    no_read.has_read = false;
    assert_eq!(
        activity_workspace(&no_read, "lead", &mut repo, &mut directory)
            .await
            .unwrap_err()
            .code,
        "CRM_ACTIVITY_READ_REQUIRED"
    );
    assert!(activity_work(&no_read, &mut repo).await.unwrap().is_empty());

    let mut missing = FakeRepo {
        present: false,
        rows: vec![],
    };
    assert_eq!(
        activity_workspace(&actor(), "missing", &mut missing, &mut directory)
            .await
            .unwrap_err()
            .code,
        "CRM_LEAD_NOT_FOUND"
    );
}
