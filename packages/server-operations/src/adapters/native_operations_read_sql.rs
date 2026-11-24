//! Owner-owned caller-unit read plans for Operations. Generated from the
//! retired TypeScript SELECT literal with its exact positional payload; the
//! axum `command-plan` route resolves the kind here and returns the statement
//! for the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, `bind_count` the positional
/// argument count and `scope` the routing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Native successor of the TypeScript `workflow-projection-source` read. The
/// owner plans this literal; the caller executes it on its open unit with the
/// same `[id]` bind. The typed native port `workflow_projection_source` is
/// mounted on the caller's connection and carries the same owner columns.
pub const RD_TASK_PROJECTION_SOURCE: &str = "operations_rd_task_projection_source";
const SQL_TASK_PROJECTION_SOURCE: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?";
const READ_TASK_PROJECTION_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_TASK_PROJECTION_SOURCE,
    sql: SQL_TASK_PROJECTION_SOURCE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `app-upload-task-source` read. The owner
/// plans this literal; the caller executes it on its open unit with the same
/// `[id, company_id]` bind.
pub const RD_APP_UPLOAD_TASK_STATE: &str = "operations_rd_app_upload_task_state";
const SQL_APP_UPLOAD_TASK_STATE: &str =
    "SELECT state FROM staff_tasks WHERE id=? AND company_id=?";
const READ_APP_UPLOAD_TASK_STATE: NativeReadStatement = NativeReadStatement {
    kind: RD_APP_UPLOAD_TASK_STATE,
    sql: SQL_APP_UPLOAD_TASK_STATE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `workflow-flow-routing-facts` read. The
/// owner plans this literal; the caller executes it on its open unit with the
/// same `[company_id, id]` bind. `id` may be a string, number or null exactly as
/// the retired owner literal bound it.
pub const RD_FLOW_DEFINITIONS_DELEGATION: &str = "operations_rd_flow_definitions_delegation";
const SQL_FLOW_DEFINITIONS_DELEGATION: &str = "SELECT delegate_user_id FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY created_at DESC,id DESC LIMIT 1";
const READ_FLOW_DEFINITIONS_DELEGATION: NativeReadStatement = NativeReadStatement {
    kind: RD_FLOW_DEFINITIONS_DELEGATION,
    sql: SQL_FLOW_DEFINITIONS_DELEGATION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript beta-management `taskCompany` read. The
/// owner plans this literal; the caller executes it on its open unit with the
/// same `[id]` bind.
pub const RD_BETA_MANAGEMENT_TASK_COMPANY: &str = "operations_rd_beta_management_task_company";
const SQL_BETA_MANAGEMENT_TASK_COMPANY: &str =
    "SELECT company_id FROM staff_tasks WHERE id=?";
const READ_BETA_MANAGEMENT_TASK_COMPANY: NativeReadStatement = NativeReadStatement {
    kind: RD_BETA_MANAGEMENT_TASK_COMPANY,
    sql: SQL_BETA_MANAGEMENT_TASK_COMPANY,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript beta-management `taskFileOwner` fragment.
/// Unlike the other owner reads this statement is a correlated `EXISTS(...)`
/// predicate, not a standalone query: the owner plans the exact fragment and
/// the trusted caller embeds the returned SQL text in its own attachment query.
/// The `?` is bound by that caller, so the plan route carries no values for
/// this kind (`bind_count: 0`).
pub const RD_BETA_MANAGEMENT_TASK_FILE_OWNER: &str = "operations_rd_beta_management_task_file_owner";
const SQL_BETA_MANAGEMENT_TASK_FILE_OWNER: &str =
    "EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)";
const READ_BETA_MANAGEMENT_TASK_FILE_OWNER: NativeReadStatement = NativeReadStatement {
    kind: RD_BETA_MANAGEMENT_TASK_FILE_OWNER,
    sql: SQL_BETA_MANAGEMENT_TASK_FILE_OWNER,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the duplicated TypeScript routine-template-schedules
/// read. Both owners previously bound the same literal on their own unit; the
/// owner plans it once here and each caller executes it on its open unit with
/// the same `[template_id, company_id]` bind.
pub const RD_ROUTINE_TEMPLATE_SCHEDULES: &str = "operations_rd_routine_template_schedules";
const SQL_ROUTINE_TEMPLATE_SCHEDULES: &str =
    "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at";
const READ_ROUTINE_TEMPLATE_SCHEDULES: NativeReadStatement = NativeReadStatement {
    kind: RD_ROUTINE_TEMPLATE_SCHEDULES,
    sql: SQL_ROUTINE_TEMPLATE_SCHEDULES,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `field-command-owner` exception routing
/// read. The owner plans this literal; the caller executes it on its open unit
/// with the same `[id, company_id]` bind.
pub const RD_FIELD_EXCEPTION: &str = "operations_rd_field_exception";
const SQL_FIELD_EXCEPTION: &str =
    "SELECT source_kind FROM operations_exceptions WHERE id=? AND company_id=?";
const READ_FIELD_EXCEPTION: NativeReadStatement = NativeReadStatement {
    kind: RD_FIELD_EXCEPTION,
    sql: SQL_FIELD_EXCEPTION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `field-command-owner` policy routing
/// read. The owner plans this literal; the caller executes it on its open unit
/// with the same `[id, company_id]` bind.
pub const RD_FIELD_POLICY: &str = "operations_rd_field_policy";
const SQL_FIELD_POLICY: &str =
    "SELECT source_kinds_json FROM operations_queue_policies WHERE id=? AND company_id=?";
const READ_FIELD_POLICY: NativeReadStatement = NativeReadStatement {
    kind: RD_FIELD_POLICY,
    sql: SQL_FIELD_POLICY,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `legacy-routine-access` templates read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[company_id, creator_user_id, is_owner]` bind.
pub const RD_LEGACY_ROUTINE_TEMPLATES: &str = "operations_rd_legacy_routine_templates";
const SQL_LEGACY_ROUTINE_TEMPLATES: &str =
    "SELECT t.* FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NULL AND (t.published=1 OR t.creator_user_id=? OR ?=1) ORDER BY t.id";
const READ_LEGACY_ROUTINE_TEMPLATES: NativeReadStatement = NativeReadStatement {
    kind: RD_LEGACY_ROUTINE_TEMPLATES,
    sql: SQL_LEGACY_ROUTINE_TEMPLATES,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript `operations-exceptions` workspace queue
/// policies read. The owner plans this literal; the caller executes it on its
/// open unit with the same `[company_id]` bind. Field-only policies are excluded.
pub const RD_OPERATIONS_QUEUE_POLICIES: &str = "operations_rd_operations_queue_policies";
const SQL_OPERATIONS_QUEUE_POLICIES: &str =
    "SELECT * FROM operations_queue_policies WHERE company_id=? AND NOT (jsonb_typeof(source_kinds_json::jsonb)='array' AND source_kinds_json::jsonb<>'[]'::jsonb AND source_kinds_json::jsonb <@ '\"field_service_order\"'::jsonb) ORDER BY area,revision DESC";
const READ_OPERATIONS_QUEUE_POLICIES: NativeReadStatement = NativeReadStatement {
    kind: RD_OPERATIONS_QUEUE_POLICIES,
    sql: SQL_OPERATIONS_QUEUE_POLICIES,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `operations-exceptions` detail events
/// read. The owner plans this literal; the caller executes it on its open unit
/// with the same `[exception_id]` bind.
pub const RD_OPERATIONS_EXCEPTION_EVENTS: &str = "operations_rd_operations_exception_events";
const SQL_OPERATIONS_EXCEPTION_EVENTS: &str =
    "SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version";
const READ_OPERATIONS_EXCEPTION_EVENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_OPERATIONS_EXCEPTION_EVENTS,
    sql: SQL_OPERATIONS_EXCEPTION_EVENTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `record-purchases` fixed task-source
/// read. The owner plans this literal; the caller executes it on its open unit
/// with the same `[id, company_id]` bind and takes the first row.
pub const RD_RECORD_TASK_PROJECT: &str = "operations_rd_record_task_project";
const SQL_RECORD_TASK_PROJECT: &str =
    "SELECT project_id FROM staff_tasks WHERE id=? AND company_id=?";
const READ_RECORD_TASK_PROJECT: NativeReadStatement = NativeReadStatement {
    kind: RD_RECORD_TASK_PROJECT,
    sql: SQL_RECORD_TASK_PROJECT,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `work-task-purchases` fixed task-source
/// read. The owner plans this literal; the caller executes it on its open unit
/// with the same `[id, company_id]` bind and takes the first row.
pub const RD_WORK_TASK_SOURCE: &str = "operations_rd_work_task_source";
const SQL_WORK_TASK_SOURCE: &str =
    "SELECT id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version FROM staff_tasks WHERE id=? AND company_id=?";
const READ_WORK_TASK_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_WORK_TASK_SOURCE,
    sql: SQL_WORK_TASK_SOURCE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `task-attachment-purchases` assignee
/// identity read. The owner plans this literal; the caller executes it on its
/// open unit with the same `[assignee_user_id, user_id, company_id]` bind and
/// reads the `assigned` column exactly as the retired `.first('assigned')` did.
pub const RD_TASK_ATTACHMENT_ASSIGNEE: &str = "operations_rd_task_attachment_assignee";
const SQL_TASK_ATTACHMENT_ASSIGNEE: &str = "SELECT iam_same_person(?,?,?) assigned";
const READ_TASK_ATTACHMENT_ASSIGNEE: NativeReadStatement = NativeReadStatement {
    kind: RD_TASK_ATTACHMENT_ASSIGNEE,
    sql: SQL_TASK_ATTACHMENT_ASSIGNEE,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript `work-task-alert-purchases` source read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[now, now, id, company_id]` bind and takes the first row exactly
/// as the retired `.first()` did. Both tables are owner-local.
pub const RD_WORK_TASK_ALERT_SOURCE: &str = "operations_rd_work_task_alert_source";
const SQL_WORK_TASK_ALERT_SOURCE: &str = "SELECT t.id,t.company_id,t.project_id,t.template_id,t.version,t.title,t.due_at,t.assignee_user_id,t.creator_user_id,t.state,(SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>? ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) delegate_user_id FROM staff_tasks t WHERE t.id=? AND t.company_id=?";
const READ_WORK_TASK_ALERT_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_WORK_TASK_ALERT_SOURCE,
    sql: SQL_WORK_TASK_ALERT_SOURCE,
    bind_count: 4,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[
    READ_TASK_PROJECTION_SOURCE,
    READ_APP_UPLOAD_TASK_STATE,
    READ_FLOW_DEFINITIONS_DELEGATION,
    READ_BETA_MANAGEMENT_TASK_COMPANY,
    READ_BETA_MANAGEMENT_TASK_FILE_OWNER,
    READ_ROUTINE_TEMPLATE_SCHEDULES,
    READ_FIELD_EXCEPTION,
    READ_FIELD_POLICY,
    READ_LEGACY_ROUTINE_TEMPLATES,
    READ_OPERATIONS_QUEUE_POLICIES,
    READ_OPERATIONS_EXCEPTION_EVENTS,
    READ_RECORD_TASK_PROJECT,
    READ_WORK_TASK_SOURCE,
    READ_TASK_ATTACHMENT_ASSIGNEE,
    READ_WORK_TASK_ALERT_SOURCE,
];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_TASK_PROJECTION_SOURCE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_TASK_PROJECTION_SOURCE));
        assert!(!is_read("operations_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_TASK_PROJECTION_SOURCE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?"
        );
    }

    #[test]
    fn app_upload_task_state_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_APP_UPLOAD_TASK_STATE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_APP_UPLOAD_TASK_STATE));
    }

    #[test]
    fn app_upload_task_state_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_APP_UPLOAD_TASK_STATE).expect("read kind");
        assert_eq!(read.sql, "SELECT state FROM staff_tasks WHERE id=? AND company_id=?");
    }

    #[test]
    fn flow_definitions_delegation_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_FLOW_DEFINITIONS_DELEGATION).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(upper.contains("STAFF_TASK_DELEGATIONS"));
        assert!(upper.contains("DELEGATE_USER_ID"));
        assert!(is_read(RD_FLOW_DEFINITIONS_DELEGATION));
    }

    #[test]
    fn flow_definitions_delegation_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_FLOW_DEFINITIONS_DELEGATION).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT delegate_user_id FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY created_at DESC,id DESC LIMIT 1"
        );
    }

    #[test]
    fn beta_management_task_company_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_BETA_MANAGEMENT_TASK_COMPANY).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BETA_MANAGEMENT_TASK_COMPANY));
    }

    #[test]
    fn beta_management_task_company_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_BETA_MANAGEMENT_TASK_COMPANY).expect("read kind");
        assert_eq!(read.sql, "SELECT company_id FROM staff_tasks WHERE id=?");
    }

    #[test]
    fn beta_management_task_file_owner_is_an_owner_planned_correlated_fragment() {
        let read = read_statement(RD_BETA_MANAGEMENT_TASK_FILE_OWNER).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("EXISTS("), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert!(upper.contains("STAFF_TASKS"));
        assert!(upper.contains("A.OWNER_ID"));
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BETA_MANAGEMENT_TASK_FILE_OWNER));
    }

    #[test]
    fn beta_management_task_file_owner_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_BETA_MANAGEMENT_TASK_FILE_OWNER).expect("read kind");
        assert_eq!(
            read.sql,
            "EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)"
        );
    }

    #[test]
    fn routine_template_schedules_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_ROUTINE_TEMPLATE_SCHEDULES).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_ROUTINE_TEMPLATE_SCHEDULES));
    }

    #[test]
    fn routine_template_schedules_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_ROUTINE_TEMPLATE_SCHEDULES).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at"
        );
    }

    #[test]
    fn field_command_owner_plans_are_holding_scoped_and_ledger_free() {
        for kind in [RD_FIELD_EXCEPTION, RD_FIELD_POLICY] {
            let read = read_statement(kind).expect("read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"), "{}", read.kind);
            assert!(!upper.contains("INSERT"));
            assert!(!upper.contains("UPDATE"));
            assert!(!upper.contains("DELETE"));
            assert_eq!(read.bind_count, 2);
            assert_eq!(read.sql.matches('?').count(), 2);
            assert_eq!(read.scope, "holding");
            assert!(is_read(kind));
        }
    }

    #[test]
    fn field_command_owner_sql_matches_retired_typescript_literals() {
        let exception = read_statement(RD_FIELD_EXCEPTION).expect("read kind");
        assert_eq!(
            exception.sql,
            "SELECT source_kind FROM operations_exceptions WHERE id=? AND company_id=?"
        );
        let policy = read_statement(RD_FIELD_POLICY).expect("read kind");
        assert_eq!(
            policy.sql,
            "SELECT source_kinds_json FROM operations_queue_policies WHERE id=? AND company_id=?"
        );
    }

    #[test]
    fn legacy_routine_templates_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_LEGACY_ROUTINE_TEMPLATES).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.sql.matches('?').count(), 3);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_LEGACY_ROUTINE_TEMPLATES));
    }

    #[test]
    fn legacy_routine_templates_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_LEGACY_ROUTINE_TEMPLATES).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT t.* FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NULL AND (t.published=1 OR t.creator_user_id=? OR ?=1) ORDER BY t.id"
        );
    }

    #[test]
    fn operations_exceptions_plans_are_holding_scoped_and_ledger_free() {
        for kind in [RD_OPERATIONS_QUEUE_POLICIES, RD_OPERATIONS_EXCEPTION_EVENTS] {
            let read = read_statement(kind).expect("read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"), "{}", read.kind);
            assert!(!upper.contains("INSERT"));
            assert!(!upper.contains("UPDATE"));
            assert!(!upper.contains("DELETE"));
            assert_eq!(read.bind_count, 1);
            assert_eq!(read.sql.matches('?').count(), 1);
            assert_eq!(read.scope, "holding");
            assert!(is_read(kind));
        }
    }

    #[test]
    fn operations_exceptions_sql_matches_retired_typescript_literals() {
        let policies = read_statement(RD_OPERATIONS_QUEUE_POLICIES).expect("read kind");
        assert_eq!(
            policies.sql,
            "SELECT * FROM operations_queue_policies WHERE company_id=? AND NOT (jsonb_typeof(source_kinds_json::jsonb)='array' AND source_kinds_json::jsonb<>'[]'::jsonb AND source_kinds_json::jsonb <@ '\"field_service_order\"'::jsonb) ORDER BY area,revision DESC"
        );
        let events = read_statement(RD_OPERATIONS_EXCEPTION_EVENTS).expect("read kind");
        assert_eq!(
            events.sql,
            "SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version"
        );
    }

    #[test]
    fn fixed_task_source_plans_are_holding_scoped_and_ledger_free() {
        for kind in [RD_RECORD_TASK_PROJECT, RD_WORK_TASK_SOURCE] {
            let read = read_statement(kind).expect("read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"), "{}", read.kind);
            assert!(!upper.contains("INSERT"));
            assert!(!upper.contains("UPDATE"));
            assert!(!upper.contains("DELETE"));
            assert_eq!(read.bind_count, 2);
            assert_eq!(read.sql.matches('?').count(), 2);
            assert_eq!(read.scope, "holding");
            assert!(is_read(kind));
        }
    }

    #[test]
    fn fixed_task_source_sql_matches_retired_typescript_literals() {
        let project = read_statement(RD_RECORD_TASK_PROJECT).expect("read kind");
        assert_eq!(
            project.sql,
            "SELECT project_id FROM staff_tasks WHERE id=? AND company_id=?"
        );
        let source = read_statement(RD_WORK_TASK_SOURCE).expect("read kind");
        assert_eq!(
            source.sql,
            "SELECT id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version FROM staff_tasks WHERE id=? AND company_id=?"
        );
    }

    #[test]
    fn task_attachment_assignee_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_TASK_ATTACHMENT_ASSIGNEE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.sql.matches('?').count(), 3);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_TASK_ATTACHMENT_ASSIGNEE));
    }

    #[test]
    fn task_attachment_assignee_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_TASK_ATTACHMENT_ASSIGNEE).expect("read kind");
        assert_eq!(read.sql, "SELECT iam_same_person(?,?,?) assigned");
    }

    #[test]
    fn work_task_alert_source_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_WORK_TASK_ALERT_SOURCE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_WORK_TASK_ALERT_SOURCE));
    }

    #[test]
    fn work_task_alert_source_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_WORK_TASK_ALERT_SOURCE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT t.id,t.company_id,t.project_id,t.template_id,t.version,t.title,t.due_at,t.assignee_user_id,t.creator_user_id,t.state,(SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>? ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) delegate_user_id FROM staff_tasks t WHERE t.id=? AND t.company_id=?"
        );
    }
}
