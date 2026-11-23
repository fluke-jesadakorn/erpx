//! Pure domain planner for the wf3a13 native Workflow command-ledger write slice.
//! SQL and argument order are owned here. No caller SQL, predicate or mirror.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowWave3Statement {
    pub sql: String,
    pub values: Vec<Value>,
}

pub const COMMAND_RECEIPT: &str = "wf_command_receipt";
pub const COMMAND_AUTHORITY: &str = "wf_command_authority";
pub const COMMAND_AUDIT: &str = "wf_command_audit";
pub const COMMAND_OUTBOX: &str = "wf_command_outbox";
pub const LEGAL_AI_QUEUE: &str = "legal_ai_queue";
pub const AUDIT_DIRECT: &str = "wf_audit_direct";
pub const OUTBOX_DIRECT: &str = "wf_outbox_direct";
pub const ORG_DRAFT_AUDIT: &str = "wf_org_draft_audit";
pub const ORG_DRAFT_OUTBOX: &str = "wf_org_draft_outbox";
pub const WF_OPENAPPROVAL: &str = "wf_openapproval";
pub const WF_OPENAPPROVAL2: &str = "wf_openapproval2";
pub const WF_OPENAPPROVAL3: &str = "wf_openapproval3";
pub const WF_INITIALIZELEGACYAPPROVAL: &str = "wf_initializelegacyapproval";
pub const WF_INITIALIZELEGACYAPPROVAL2: &str = "wf_initializelegacyapproval2";
pub const WF_INITIALIZELEGACYAPPROVAL4: &str = "wf_initializelegacyapproval4";
pub const WF_INITIALIZELEGACYAPPROVAL5: &str = "wf_initializelegacyapproval5";
pub const WF_INITIALIZELEGACYAPPROVAL6: &str = "wf_initializelegacyapproval6";
pub const WF_PREPAREDECISION: &str = "wf_preparedecision";
pub const WF_PREPAREDECISION2: &str = "wf_preparedecision2";
pub const WF_PREPAREDECISION3: &str = "wf_preparedecision3";
pub const WF_CLOSEAPPROVALS: &str = "wf_closeapprovals";
pub const WF_CLOSEAPPROVALS2: &str = "wf_closeapprovals2";
pub const WF_REROUTEAPPROVAL: &str = "wf_rerouteapproval";
pub const WF_REROUTEAPPROVAL2: &str = "wf_rerouteapproval2";
pub const WF_REROUTEAPPROVAL3: &str = "wf_rerouteapproval3";
pub const WF_REROUTEAPPROVAL4: &str = "wf_rerouteapproval4";
pub const WF_LEGAL_ADVANCEROUTE: &str = "wf_legal_advanceroute";
pub const WF_LEGAL_REPLACEREVIEWER: &str = "wf_legal_replacereviewer";
pub const WF_LEGAL_RECORDREROUTE: &str = "wf_legal_recordreroute";
pub const WF_LEGAL_DECIDESTEP: &str = "wf_legal_decidestep";
pub const WF_LEGAL_RECORDDECISION: &str = "wf_legal_recorddecision";
pub const WF_LEGAL_CANCELROUTE: &str = "wf_legal_cancelroute";
pub const WF_LEGAL_ADVANCESTEP: &str = "wf_legal_advancestep";
pub const WF_LEGAL_APPROVEROUTE: &str = "wf_legal_approveroute";
pub const WF_FLOW_STAGEROUTESTATEMENTSEFFECT: &str = "wf_flow_stageroutestatementseffect";
pub const WF_FLOW_STAGEPREPAREFLOWCOMMANDSTEPS: &str = "wf_flow_stageprepareflowcommandsteps";
pub const WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT: &str = "wf_flow_stageassignflowprimaryeffect";
pub const WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT2: &str = "wf_flow_stageassignflowprimaryeffect2";
pub const WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT3: &str = "wf_flow_stageassignflowprimaryeffect3";
pub const WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT4: &str = "wf_flow_stageassignflowprimaryeffect4";
pub const WF_PORTAL_AUDIT: &str = "wf_portal_audit";
pub const WF_PORTAL_OUTBOX: &str = "wf_portal_outbox";
pub const WF_NATIVE_APPROVAL_PLANNING_0: &str = "wf_native_approval_planning_0";
pub const WF_NATIVE_APPROVAL_PLANNING_1: &str = "wf_native_approval_planning_1";
pub const WF_NATIVE_BETA_INVITATION_SOURCES_2: &str = "wf_native_beta_invitation_sources_2";
pub const WF_NATIVE_COMMAND_ASSERTIONS_3: &str = "wf_native_command_assertions_3";
pub const WF_NATIVE_FINANCE_BOOK_ADJUSTMENT_EFFECTS_4: &str =
    "wf_native_finance_book_adjustment_effects_4";
pub const WF_NATIVE_FINANCE_BUSINESS_CONTRACT_EFFECTS_5: &str =
    "wf_native_finance_business_contract_effects_5";
pub const WF_NATIVE_FINANCE_OWNER_DECISION_EFFECTS_6: &str =
    "wf_native_finance_owner_decision_effects_6";
pub const WF_NATIVE_FINANCE_OWNER_DECISION_EFFECTS_7: &str =
    "wf_native_finance_owner_decision_effects_7";
pub const WF_NATIVE_INVENTORY_COUNTS_SOURCES_8: &str = "wf_native_inventory_counts_sources_8";
pub const WF_NATIVE_INVENTORY_TRANSFERS_SOURCES_9: &str = "wf_native_inventory_transfers_sources_9";
pub const WF_NATIVE_LEAVE_REVIEW_FACTS_10: &str = "wf_native_leave_review_facts_10";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_11: &str = "wf_native_payroll_owner_facts_11";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_12: &str = "wf_native_payroll_owner_facts_12";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_13: &str = "wf_native_payroll_owner_facts_13";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_14: &str = "wf_native_payroll_owner_facts_14";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_15: &str = "wf_native_payroll_owner_facts_15";
pub const WF_NATIVE_PAYROLL_OWNER_FACTS_16: &str = "wf_native_payroll_owner_facts_16";
pub const WF_NATIVE_PAYROLL_STATUTORY_DECISION_EFFECTS_17: &str =
    "wf_native_payroll_statutory_decision_effects_17";
pub const WF_NATIVE_PEOPLE_EVALUATION_FACTS_18: &str = "wf_native_people_evaluation_facts_18";
pub const WF_NATIVE_PEOPLE_LIFECYCLE_AUDIT_EFFECTS_19: &str =
    "wf_native_people_lifecycle_audit_effects_19";
pub const WF_NATIVE_RUNTIME_JOB_EFFECTS_20: &str = "wf_native_runtime_job_effects_20";
pub const WF_NATIVE_RUNTIME_JOB_EFFECTS_21: &str = "wf_native_runtime_job_effects_21";
pub const WF_NATIVE_RUNTIME_JOB_EFFECTS_22: &str = "wf_native_runtime_job_effects_22";
pub const WF_NATIVE_RUNTIME_JOB_EFFECTS_23: &str = "wf_native_runtime_job_effects_23";
pub const WF_NATIVE_SYSTEM_PAYROLL_PREVIEW_EVENTS_24: &str =
    "wf_native_system_payroll_preview_events_24";
pub const WF_NATIVE_SYSTEM_PAYROLL_PREVIEW_EVENTS_25: &str =
    "wf_native_system_payroll_preview_events_25";
pub const WF_NATIVE_FINANCE_BUSINESS_CONTRACT_RESIDUAL: &str =
    "wf_native_finance_business_contract_residual";
pub const WF_NATIVE_TECH_PROVISION_OUTBOX: &str = "wf_native_tech_provision_outbox";
pub const WF_NATIVE_LIFECYCLE_OUTBOX: &str = "wf_native_lifecycle_outbox";
pub const WF_NATIVE_LIFECYCLE_WORKER_OUTBOX: &str = "wf_native_lifecycle_worker_outbox";
/// Owner-authored caller-unit reads for the Work UI staff facts family. The
/// mounted wave3-command planner resolves these to fixed SELECT statements;
/// the TypeScript bridge executes them on the caller's open unit. No SQL is
/// accepted from the wire and no command receipt/assertion kind is changed.
pub const WF_NATIVE_WORK_UI_APPROVAL_QUEUE: &str = "wf_native_work_ui_approval_queue";
pub const WF_NATIVE_WORK_UI_BETA_APPROVAL: &str = "wf_native_work_ui_beta_approval";
/// Owner-authored caller-unit read for the leave evidence decision family. The
/// mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned approval tables; the TypeScript bridge executes it on the
/// caller's open unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_LEAVE_EVIDENCE_DECISION: &str = "wf_native_leave_evidence_decision";
/// Owner-authored caller-unit read for the spending settings history family.
/// The mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned audit ledger; the TypeScript bridge executes it on the
/// caller's open unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_SPENDING_SETTINGS_HISTORY: &str = "wf_native_spending_settings_history";
/// Owner-authored caller-unit read for the operations staff task alert
/// approver family. The mounted wave3-command planner resolves it to one fixed
/// SELECT on the Workflow-owned approval tables; the TypeScript bridge executes
/// it on the caller's open unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_STAFF_TASK_ALERT_APPROVERS: &str = "wf_native_staff_task_alert_approvers";
/// Owner-authored caller-unit read for the department approval queue family.
/// The mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned approval tables; the TypeScript bridge executes it on the
/// caller's open unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE: &str = "wf_native_department_approval_queue";
/// Owner-authored caller-unit reads for the work report facts family. The
/// mounted wave3-command planner resolves each to one fixed SELECT on the
/// Workflow-owned approval tables; the TypeScript bridge executes it on the
/// caller's open unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_WORK_REPORT_DECISIONS: &str = "wf_native_work_report_decisions";
pub const WF_NATIVE_WORK_REPORT_SAVED_ROUTES: &str = "wf_native_work_report_saved_routes";
/// Owner-authored caller-unit read for the approval administration queue family.
/// The mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned approval tables; the TypeScript bridge executes it on the
/// caller's open company unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE: &str = "wf_native_approval_administration_queue";
/// Owner-authored caller-unit read for the legal decision history family. The
/// mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned approval tables; the TypeScript bridge executes it on the
/// caller's open company unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_LEGAL_DECISION_HISTORY: &str = "wf_native_legal_decision_history";
/// Owner-authored caller-unit read for the flow planning definitions instance
/// family. The mounted wave3-command planner resolves it to one fixed SELECT
/// on the Workflow-owned approval tables; the TypeScript bridge executes it on
/// the caller's open company unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_FLOW_DEFINITIONS_INSTANCE: &str = "wf_native_flow_definitions_instance";
/// Owner-authored caller-unit read for the finance sales-invoice completion
/// receipt family. The mounted wave3-command planner resolves it to one fixed
/// SELECT on the Workflow-owned command ledger; the TypeScript bridge executes
/// it on the caller's open company unit and carries no receipt/assertion
/// semantics.
pub const WF_NATIVE_FINANCE_SALES_COMPLETION: &str = "wf_native_finance_sales_completion";
/// Owner-authored caller-unit read for the finance AI source revision family.
/// The mounted wave3-command planner resolves it to one fixed SELECT on the
/// Workflow-owned command ledger; the TypeScript bridge executes it on the
/// caller's open company unit and carries no receipt/assertion semantics.
pub const WF_NATIVE_FINANCE_SOURCE_REVISION: &str = "wf_native_finance_source_revision";
pub const KINDS: &[&str] = &[
    WF_OPENAPPROVAL,
    WF_OPENAPPROVAL2,
    WF_OPENAPPROVAL3,
    WF_INITIALIZELEGACYAPPROVAL,
    WF_INITIALIZELEGACYAPPROVAL2,
    WF_INITIALIZELEGACYAPPROVAL4,
    WF_INITIALIZELEGACYAPPROVAL5,
    WF_INITIALIZELEGACYAPPROVAL6,
    WF_PREPAREDECISION,
    WF_PREPAREDECISION2,
    WF_PREPAREDECISION3,
    WF_CLOSEAPPROVALS,
    WF_CLOSEAPPROVALS2,
    WF_REROUTEAPPROVAL,
    WF_REROUTEAPPROVAL2,
    WF_REROUTEAPPROVAL3,
    WF_REROUTEAPPROVAL4,
    COMMAND_RECEIPT,
    COMMAND_AUTHORITY,
    COMMAND_AUDIT,
    COMMAND_OUTBOX,
    LEGAL_AI_QUEUE,
    AUDIT_DIRECT,
    OUTBOX_DIRECT,
    ORG_DRAFT_AUDIT,
    ORG_DRAFT_OUTBOX,
    WF_NATIVE_WORK_UI_APPROVAL_QUEUE,
    WF_NATIVE_WORK_UI_BETA_APPROVAL,
    WF_NATIVE_LEAVE_EVIDENCE_DECISION,
    WF_NATIVE_SPENDING_SETTINGS_HISTORY,
    WF_NATIVE_STAFF_TASK_ALERT_APPROVERS,
    WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE,
    WF_NATIVE_WORK_REPORT_DECISIONS,
    WF_NATIVE_WORK_REPORT_SAVED_ROUTES,
    WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE,
    WF_NATIVE_LEGAL_DECISION_HISTORY,
    WF_NATIVE_FLOW_DEFINITIONS_INSTANCE,
    WF_NATIVE_FINANCE_SALES_COMPLETION,
    WF_NATIVE_FINANCE_SOURCE_REVISION,
];

fn statement(kind: &str) -> Option<(&'static str, usize)> {
    Some(match kind {
        COMMAND_RECEIPT => (
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES(?,?,?,?,?,?,?,?)",
            8,
        ),
        COMMAND_AUTHORITY => (
            "UPDATE command_receipts SET authority_json=(authority_json::jsonb || \
             (SELECT jsonb_build_object('scopeKind',scope_kind,'scopeNodeId',scope_node_id) FROM assignments WHERE id=?))::text \
             WHERE command_id=?",
            2,
        ),
        COMMAND_AUDIT => (
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES(?,?,?,?,?,?,?,?,?,?,(SELECT authority_json FROM command_receipts WHERE command_id=?))",
            11,
        ),
        COMMAND_OUTBOX => (
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(?,?,?,?,?,?,?,?)",
            8,
        ),
        LEGAL_AI_QUEUE => ("INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at,actor_json) VALUES(?,?,?,?,?,'queued',?,?,?)", 8),
        AUDIT_DIRECT => (
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            11,
        ),
        OUTBOX_DIRECT => (
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(?,?,?,?,?,?,?,?)",
            8,
        ),
        ORG_DRAFT_AUDIT => (
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) \
             VALUES(?,?,?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)",
            7,
        ),
        ORG_DRAFT_OUTBOX => (
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)",
            5,
        ),
        WF_OPENAPPROVAL => (
            "UPDATE approval_instances SET state='superseded',version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked')",
            3,
        ),
        WF_OPENAPPROVAL2 => (
            "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?, ?,?)",
            13,
        ),
        WF_OPENAPPROVAL3 => (
            "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,approver_pool_json,required_role,stage,route_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?)",
            9,
        ),
        WF_INITIALIZELEGACYAPPROVAL => (
            "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority) VALUES(?,?,?,?,?)",
            5,
        ),
        WF_INITIALIZELEGACYAPPROVAL2 => (
            "UPDATE approval_instances SET state='pending',branch_id=?,department_id=?,blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?",
            4,
        ),
        WF_INITIALIZELEGACYAPPROVAL4 => (
            "INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,'initialize','system','Initialize pending request under department routing',?,?,?)",
            5,
        ),
        WF_INITIALIZELEGACYAPPROVAL5 => {
            ("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)", 10)
        }
        WF_INITIALIZELEGACYAPPROVAL6 => (
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
            8,
        ),
        WF_PREPAREDECISION => (
            "UPDATE approval_steps SET state=?,approver_user_id=? WHERE id=?",
            3,
        ),
        WF_PREPAREDECISION2 => (
            "UPDATE approval_instances SET state=?,current_step=?,version=version+1,updated_at=? WHERE id=?",
            4,
        ),
        WF_PREPAREDECISION3 => (
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            12,
        ),
        WF_CLOSEAPPROVALS => (
            "INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) SELECT ?||'-'||id,id,?,?,?,?,?,? FROM approval_instances WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')",
            9,
        ),
        WF_CLOSEAPPROVALS2 => (
            "UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')",
            4,
        ),
        WF_REROUTEAPPROVAL => (
            "UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,ARRAY[?::text,'primaryUserId'],to_jsonb(?::text),true)::text,version=version+1 WHERE company_id=? AND kind=? AND record_id=?",
            5,
        ),
        WF_REROUTEAPPROVAL2 => (
            "UPDATE approval_steps SET approver_user_id=?,approver_pool_json=?,authority=?,route_snapshot_json=? WHERE id=? AND state='pending'",
            5,
        ),
        WF_REROUTEAPPROVAL3 => (
            "UPDATE approval_instances SET blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?",
            2,
        ),
        WF_REROUTEAPPROVAL4 => (
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)",
            10,
        ),
        WF_LEGAL_ADVANCEROUTE => (
            "UPDATE approval_instances SET version=version+1,updated_at=? WHERE id=?",
            2,
        ),
        WF_LEGAL_REPLACEREVIEWER => ("UPDATE approval_steps SET approver_user_id=? WHERE id=?", 2),
        WF_LEGAL_RECORDREROUTE => (
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,previous_approver_user_id,approver_user_id) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)",
            10,
        ),
        WF_LEGAL_DECIDESTEP => ("UPDATE approval_steps SET state=? WHERE id=?", 2),
        WF_LEGAL_RECORDDECISION => (
            "INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?)",
            10,
        ),
        WF_LEGAL_CANCELROUTE => (
            "UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE id=?",
            3,
        ),
        WF_LEGAL_ADVANCESTEP => (
            "UPDATE approval_instances SET current_step=current_step+1,version=version+1,updated_at=? WHERE id=?",
            2,
        ),
        WF_LEGAL_APPROVEROUTE => (
            "UPDATE approval_instances SET state='approved',version=version+1,updated_at=? WHERE id=?",
            2,
        ),
        WF_FLOW_STAGEROUTESTATEMENTSEFFECT => (
            "INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at) VALUES(?,?,?,?,?,?)",
            6,
        ),
        WF_FLOW_STAGEPREPAREFLOWCOMMANDSTEPS => (
            "INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            12,
        ),
        WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT => (
            "UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,'{stock_gl,primaryUserId}',to_jsonb(?::text))::text,version=version+1 WHERE company_id=? AND kind='order' AND record_id=?",
            3,
        ),
        WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT2 => (
            "UPDATE flow_routes SET steps_json=?,version=version+1 WHERE company_id=? AND kind=? AND record_id=?",
            4,
        ),
        WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT3 => (
            "INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
            9,
        ),
        WF_FLOW_STAGEASSIGNFLOWPRIMARYEFFECT4 => (
            "INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,'assign',?,?,?,?,?)",
            11,
        ),
        WF_PORTAL_AUDIT => (
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            11,
        ),
        WF_PORTAL_OUTBOX => (
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
            8,
        ),
                WF_NATIVE_APPROVAL_PLANNING_0 => ("INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) SELECT ?||'-'||id,id,?,?,?,?,?,? FROM approval_instances WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')", 9),
        WF_NATIVE_APPROVAL_PLANNING_1 => ("UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')", 4),
        WF_NATIVE_BETA_INVITATION_SOURCES_2 => ("\"INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'beta.member.join','beta_member',?,1,'Verified invitation acceptance',?)\",", 6),
        WF_NATIVE_COMMAND_ASSERTIONS_3 => ("INSERT INTO command_assertions(command_id,step,ok) VALUES(?,?,CASE WHEN (%ASSERTION_CONDITION%) THEN 1 ELSE 0 END)", 2),
        WF_NATIVE_FINANCE_BOOK_ADJUSTMENT_EFFECTS_4 => ("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_FINANCE_BUSINESS_CONTRACT_EFFECTS_5 => ("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_FINANCE_OWNER_DECISION_EFFECTS_6 => ("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_FINANCE_OWNER_DECISION_EFFECTS_7 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_INVENTORY_COUNTS_SOURCES_8 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_INVENTORY_TRANSFERS_SOURCES_9 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_LEAVE_REVIEW_FACTS_10 => ("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_11 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_12 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_13 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_14 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_15 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) SELECT ?,?,?,?,?,?,?,input_digest,? FROM (SELECT \"id\",\"company_id\",\"pay_group_id\",\"country_pack_id\",\"period_start\",\"period_end\",\"pay_date\",\"currency\",\"mode\",\"status\",\"revision\",\"totals_json\",\"input_digest\",\"error_message\",\"version\",\"created_by\",\"approved_by\",\"posted_by\",\"created_at\",\"updated_at\",\"scope_engagement_id\",\"submitted_by\",\"submitted_digest\",\"submitted_at\",\"adjustment_of_run_id\" FROM hr_payroll_runs) hr_payroll_runs WHERE id=?", 9),
        WF_NATIVE_PAYROLL_OWNER_FACTS_16 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PAYROLL_STATUTORY_DECISION_EFFECTS_17 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PEOPLE_EVALUATION_FACTS_18 => ("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_PEOPLE_LIFECYCLE_AUDIT_EFFECTS_19 => ("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,'people_lifecycle',?,?,?,?,(SELECT authority_json FROM command_receipts WHERE command_id=?))", 10),
        WF_NATIVE_RUNTIME_JOB_EFFECTS_20 => ("UPDATE outbox_events SET lease_token=?,claim_until=?,attempts=attempts+1
    WHERE id=(SELECT e.id FROM outbox_events e WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL
      AND e.next_attempt_at<=? AND (e.claim_until IS NULL OR e.claim_until<?)
      )` : ''}
      ORDER BY e.created_at,e.id FOR UPDATE OF e SKIP LOCKED LIMIT 1)
    RETURNING id,event_type,aggregate_id,aggregate_version,payload_json,attempts,lease_token", 4),
        WF_NATIVE_RUNTIME_JOB_EFFECTS_21 => ("UPDATE outbox_events SET claim_until=? WHERE id=? AND lease_token=? AND processed_at IS NULL", 3),
        WF_NATIVE_RUNTIME_JOB_EFFECTS_22 => ("UPDATE outbox_events SET processed_at=?,published_at=?,claim_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?", 4),
        WF_NATIVE_RUNTIME_JOB_EFFECTS_23 => ("UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=?,last_error=?,dead_letter_at=? WHERE id=? AND lease_token=?", 5),
        WF_NATIVE_SYSTEM_PAYROLL_PREVIEW_EVENTS_24 => ("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)
          VALUES(?,?,'system:payroll-schedule','system:payroll-schedule','people.payroll.calculate','people_payroll',?,?,'Scheduled gross-pay preview',?)", 5),
        WF_NATIVE_SYSTEM_PAYROLL_PREVIEW_EVENTS_25 => ("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)
          VALUES(?,?,'people.payroll.calculate','people_payroll',?,?,?,?)", 6),
        WF_NATIVE_FINANCE_BUSINESS_CONTRACT_RESIDUAL => ("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)", 9),
        WF_NATIVE_TECH_PROVISION_OUTBOX => ("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'people.provision.applied','people_provision',?,1,?,?) ON CONFLICT DO NOTHING", 7),
        WF_NATIVE_LIFECYCLE_OUTBOX => ("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,'people_lifecycle',?,?,?,?)", 7),
        WF_NATIVE_LIFECYCLE_WORKER_OUTBOX => ("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'people.lifecycle.applied','people_lifecycle',?,?,?,?)", 6),
        // Owner-local reads: fixed SELECTs executed by the caller on its own
        // serializable unit. These carry no receipt/audit/outbox write ledger.
        WF_NATIVE_WORK_UI_APPROVAL_QUEUE => (
            "SELECT a.*,s.approver_user_id FROM approval_instances a LEFT JOIN approval_steps s ON s.instance_id=a.id AND s.ordinal=a.current_step WHERE a.company_id=? AND (?=0 OR a.record_type='expense') AND a.state IN ('pending','blocked') ORDER BY a.created_at LIMIT 250",
            2,
        ),
        WF_NATIVE_WORK_UI_BETA_APPROVAL => (
            "SELECT 1 FROM approval_instances WHERE id=? AND company_id=? AND record_type='expense'",
            2,
        ),
        WF_NATIVE_LEAVE_EVIDENCE_DECISION => (
            "SELECT 1 FROM approval_instances instance JOIN approval_decisions decision ON decision.instance_id=instance.id WHERE instance.company_id=? AND instance.record_type='leave' AND instance.record_id=? AND decision.action IN ('approve','reject','override') AND iam_same_person(decision.actor_user_id,?,instance.company_id) LIMIT 1",
            3,
        ),
        WF_NATIVE_SPENDING_SETTINGS_HISTORY => (
            "SELECT 'Company spending rules' title,record_version version,actor_id actor,created_at at,'Spending rules revision' kind FROM audit_events WHERE action='spending.template.save' AND record_id=? AND authority_json::jsonb->>'companyId'=? ORDER BY created_at DESC",
            2,
        ),
        WF_NATIVE_STAFF_TASK_ALERT_APPROVERS => (
            "SELECT s.approver_user_id FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id AND s.state='pending' WHERE i.company_id=? AND i.record_type='task' AND i.record_id=? AND i.state='pending'",
            2,
        ),
        WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE => (
            "SELECT i.record_type kind,i.record_id id,i.updated_at FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id AND s.ordinal=i.current_step WHERE i.company_id=? AND i.state='pending' AND s.state='pending' AND s.stage='department_approval' AND i.record_type IN ('expense','leave','overtime') ORDER BY i.updated_at DESC LIMIT 150",
            1,
        ),
        WF_NATIVE_WORK_REPORT_DECISIONS => (
            "SELECT i.record_id,d.id,d.action,d.actor_user_id actor,d.created_at at,d.reason,COALESCE(s.stage,s.authority,'') step,i.cycle,s.route_snapshot_json policy FROM approval_instances i JOIN approval_decisions d ON d.instance_id=i.id LEFT JOIN approval_steps s ON s.id=d.step_id WHERE i.company_id=? AND i.record_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY d.created_at,d.id",
            2,
        ),
        WF_NATIVE_WORK_REPORT_SAVED_ROUTES => (
            "SELECT i.record_id,i.cycle,s.ordinal,s.approver_user_id,s.stage,s.authority,s.route_snapshot_json FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id WHERE i.company_id=? AND i.record_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY i.cycle,s.ordinal",
            2,
        ),
        WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE => (
            "SELECT record_type,record_id FROM approval_instances WHERE company_id=? AND state IN ('pending','blocked') AND (?::text IS NULL OR (record_type=? AND record_id=?)) ORDER BY created_at LIMIT 50",
            4,
        ),
        WF_NATIVE_LEGAL_DECISION_HISTORY => (
            "SELECT d.* FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_type='legal' AND i.record_id=? AND i.company_id=? ORDER BY d.insertion_order",
            2,
        ),
        WF_NATIVE_FLOW_DEFINITIONS_INSTANCE => (
            "SELECT * FROM approval_instances WHERE company_id=? AND record_type=? AND record_id=? ORDER BY insertion_order DESC LIMIT 1",
            3,
        ),
        WF_NATIVE_FINANCE_SALES_COMPLETION => (
            "SELECT result_json FROM command_receipts WHERE record_id=? AND operation='finance.sales-invoice.prepare' AND authority_json::jsonb->>'companyId'=? ORDER BY created_at DESC LIMIT 1",
            2,
        ),
        WF_NATIVE_FINANCE_SOURCE_REVISION => (
            "SELECT (SELECT count(*) FROM command_receipts WHERE authority_json::jsonb->>'companyId'=? AND (operation LIKE 'finance.%' OR operation LIKE 'commerce.%' OR operation LIKE 'expense.%' OR operation LIKE 'pms.%' OR operation LIKE 'people.payroll.%') AND operation NOT LIKE 'finance.ai.%' AND operation NOT LIKE 'expense.ai.%') AS revision",
            1,
        ),
_ => return None,
    })
}

pub fn supported(kind: &str) -> bool {
    statement(kind).is_some()
}

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "WORKFLOW_WAVE3_WRITE_INVALID",
        "Review the Workflow owner write request.",
    )
}

pub fn plan(kind: &str, values: Vec<Value>) -> Result<WorkflowWave3Statement, BusinessError> {
    // Assertion writes carry the audited code-owned condition as their last
    // value; the remaining values are the positional binds. Bound the
    // fragment and reject statement/comment separators. The bound admits the
    // legitimate manufacturing file-purchase witness (observed 22578 bytes
    // for two-Company concurrent binds) while staying well under the 64 KiB
    // transport body limit; authority stays narrow because the fragment is
    // still code-owned, placeholder-exact, and executed with the caller-unit
    // receipt/replay/audit.
    if kind == WF_NATIVE_COMMAND_ASSERTIONS_3 {
        let (template, _) = statement(kind).ok_or_else(invalid)?;
        if values.len() < 3 || values.len() > 64 {
            return Err(invalid());
        }
        let condition = values
            .last()
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if condition.trim().is_empty()
            || condition.len() > 32768
            || condition.contains(';')
            || condition.contains("--")
            || condition.contains("/*")
            || condition.contains("*/")
        {
            return Err(invalid());
        }
        let sql = template.replace("%ASSERTION_CONDITION%", condition);
        let values = values[..values.len() - 1].to_vec();
        if sql.matches('?').count() != values.len() {
            return Err(invalid());
        }
        return Ok(WorkflowWave3Statement { sql, values });
    }
    let (sql, expected) = statement(kind).ok_or_else(invalid)?;
    if values.is_empty() || values.len() > 64 || values.len() != expected {
        return Err(invalid());
    }
    if sql.matches('?').count() != expected {
        return Err(invalid());
    }
    Ok(WorkflowWave3Statement {
        sql: sql.to_owned(),
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_are_parameterized_and_placeholder_exact() {
        for kind in KINDS {
            let (sql, expected) = statement(kind).expect("statement");
            assert!(sql.contains('?'));
            assert_eq!(sql.matches('?').count(), expected);
        }
    }

    #[test]
    fn planner_rejects_mismatch_and_unknown_kinds() {
        assert!(plan(COMMAND_RECEIPT, vec![json!(1)]).is_err());
        assert!(plan("unknown", vec![json!(1)]).is_err());
    }

    /// The assertion-condition bound admits the legitimate two-Company
    /// concurrent-binds witness (observed 22578 bytes) while rejecting
    /// anything over the bound, empty conditions, statement/comment
    /// separators and placeholder-inexact fragments.
    #[test]
    fn assertion_condition_bound_admits_concurrent_binds() {
        fn condition_of(len: usize) -> String {
            let mut condition = String::from("EXISTS(SELECT 1 FROM command_receipts WHERE operation=$1");
            while condition.len() < len {
                condition.push_str(" AND TRUE");
            }
            condition.truncate(len);
            condition
        }
        let binds = || vec![json!("command-1"), json!(1)];
        let mut admitted = binds();
        admitted.push(json!(condition_of(22578)));
        let planned = plan(WF_NATIVE_COMMAND_ASSERTIONS_3, admitted)
            .expect("concurrent-binds witness must be admitted");
        assert!(planned.sql.contains("EXISTS(SELECT 1"));
        for oversized in [32769, 65536] {
            let mut values = binds();
            values.push(json!(condition_of(oversized)));
            assert!(
                plan(WF_NATIVE_COMMAND_ASSERTIONS_3, values).is_err(),
                "condition of {oversized} bytes must be rejected"
            );
        }
        for forbidden in ["", "   ", "SELECT 1; SELECT 2", "x--y", "x/*y", "x*/y"] {
            let mut values = binds();
            values.push(json!(forbidden));
            assert!(
                plan(WF_NATIVE_COMMAND_ASSERTIONS_3, values).is_err(),
                "condition {forbidden:?} must be rejected"
            );
        }
        let mut inexact = binds();
        inexact.push(json!("EXISTS(SELECT 1 FROM t WHERE a=?)"));
        assert!(
            plan(WF_NATIVE_COMMAND_ASSERTIONS_3, inexact).is_err(),
            "placeholder-inexact condition must be rejected"
        );
    }

    /// Owner-local Work UI read kinds stay fixed, SELECT-only and on the
    /// Workflow-owned approval tables; no caller SQL or write verb is admitted.
    #[test]
    fn work_ui_read_kinds_are_fixed_owner_local_selects() {
        for kind in [
            WF_NATIVE_WORK_UI_APPROVAL_QUEUE,
            WF_NATIVE_WORK_UI_BETA_APPROVAL,
        ] {
            let (sql, expected) = statement(kind).expect("registered owner read kind");
            assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
            assert_eq!(expected, 2);
            assert_eq!(sql.matches('?').count(), expected);
            assert!(sql.contains("approval_instances"), "read must stay owner-local: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
            }
            assert!(supported(kind));
            assert!(plan(kind, vec![json!("company"), json!(0)]).is_ok());
            assert!(plan(kind, vec![json!("company")]).is_err());
        }
        assert!(!supported("wf_native_work_ui_unknown"));
    }

    /// The owner-local leave evidence decision read stays fixed, SELECT-only
    /// and on Workflow-owned approval tables; the bridge carries no receipt or
    /// assertion semantics and the planner never accepts caller SQL or writes.
    #[test]
    fn leave_evidence_read_kind_is_fixed_owner_local_select() {
        let (sql, expected) = statement(WF_NATIVE_LEAVE_EVIDENCE_DECISION)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 3);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(
            sql.contains("approval_instances") && sql.contains("approval_decisions"),
            "read must stay owner-local: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_LEAVE_EVIDENCE_DECISION));
        assert!(
            plan(
                WF_NATIVE_LEAVE_EVIDENCE_DECISION,
                vec![json!("company"), json!("leave-1"), json!("user-1")]
            )
            .is_ok()
        );
        assert!(plan(WF_NATIVE_LEAVE_EVIDENCE_DECISION, vec![json!("company")]).is_err());
    }

    /// The owner-local spending settings history read stays fixed, SELECT-only
    /// and on the Workflow-owned audit ledger; the bridge carries no receipt or
    /// assertion semantics and the planner never accepts caller SQL or writes.
    #[test]
    fn spending_settings_history_read_kind_is_fixed_owner_local_select() {
        let (sql, expected) = statement(WF_NATIVE_SPENDING_SETTINGS_HISTORY)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 2);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(sql.contains("audit_events"), "read must stay owner-local: {sql}");
        assert!(
            sql.contains("spending.template.save") && sql.contains("authority_json"),
            "read must keep the exact owner predicate: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_SPENDING_SETTINGS_HISTORY));
        assert!(
            plan(
                WF_NATIVE_SPENDING_SETTINGS_HISTORY,
                vec![json!("company"), json!("company")]
            )
            .is_ok()
        );
        assert!(plan(WF_NATIVE_SPENDING_SETTINGS_HISTORY, vec![json!("company")]).is_err());
    }

    /// The owner-local staff task alert approver read stays fixed, SELECT-only
    /// and on the Workflow-owned approval tables; the bridge carries no receipt
    /// or assertion semantics and the planner never accepts caller SQL or writes.
    #[test]
    fn staff_task_alert_approvers_read_kind_is_fixed_owner_local_select() {
        let (sql, expected) = statement(WF_NATIVE_STAFF_TASK_ALERT_APPROVERS)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 2);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(
            sql.contains("approval_instances") && sql.contains("approval_steps"),
            "read must stay owner-local: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_STAFF_TASK_ALERT_APPROVERS));
        assert!(
            plan(
                WF_NATIVE_STAFF_TASK_ALERT_APPROVERS,
                vec![json!("company"), json!("task-1")]
            )
            .is_ok()
        );
        assert!(plan(WF_NATIVE_STAFF_TASK_ALERT_APPROVERS, vec![json!("company")]).is_err());
    }

    /// The owner-local department approval queue read stays fixed, SELECT-only
    /// and on the Workflow-owned approval tables; the bridge carries no receipt
    /// or assertion semantics and the planner never accepts caller SQL or writes.
    #[test]
    fn department_approval_queue_read_kind_is_fixed_owner_local_select() {
        let (sql, expected) = statement(WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 1);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(
            sql.contains("approval_instances") && sql.contains("approval_steps"),
            "read must stay owner-local: {sql}"
        );
        assert!(
            sql.contains("stage='department_approval'"),
            "read must keep the exact owner predicate: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE));
        assert!(
            plan(WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE, vec![json!("company")]).is_ok()
        );
        assert!(plan(WF_NATIVE_DEPARTMENT_APPROVAL_QUEUE, vec![]).is_err());
    }

    /// The owner-local work report facts reads stay fixed, SELECT-only and on
    /// the Workflow-owned approval tables; the bridge carries no receipt or
    /// assertion semantics and the planner never accepts caller SQL or writes.
    #[test]
    fn work_report_facts_read_kinds_are_fixed_owner_local_selects() {
        for (kind, company_predicate) in [
            (WF_NATIVE_WORK_REPORT_DECISIONS, "i.company_id=?"),
            (WF_NATIVE_WORK_REPORT_SAVED_ROUTES, "i.company_id=?"),
        ] {
            let (sql, expected) = statement(kind).expect("registered owner read kind");
            assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
            assert_eq!(expected, 2);
            assert_eq!(sql.matches('?').count(), expected);
            assert!(
                sql.contains("approval_instances") && sql.contains("approval_steps"),
                "read must stay owner-local: {sql}"
            );
            assert!(sql.contains(company_predicate), "read must stay company-scoped: {sql}");
            assert!(
                sql.contains("jsonb_array_elements_text(?::jsonb)"),
                "read must keep the exact cohort bind: {sql}"
            );
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
            }
            assert!(supported(kind));
            assert!(
                plan(kind, vec![json!("company"), json!("[\"record-1\"]")]).is_ok()
            );
            assert!(plan(kind, vec![json!("company")]).is_err());
        }
    }

    /// The owner-local approval administration queue read stays fixed,
    /// SELECT-only and company-scoped on the Workflow-owned approval tables;
    /// the bridge carries no receipt or assertion semantics and the planner
    /// never accepts caller SQL or writes. Null record/record-kind binds keep
    /// the original unfiltered pending/blocked queue behavior.
    #[test]
    fn approval_administration_queue_read_kind_is_fixed_company_scoped_select() {
        let (sql, expected) = statement(WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 4);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(sql.contains("approval_instances"), "read must stay owner-local: {sql}");
        assert!(sql.contains("company_id=?"), "read must stay company-scoped: {sql}");
        assert!(
            sql.contains("?::text IS NULL OR (record_type=? AND record_id=?)"),
            "read must keep the exact owner predicate: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE));
        assert!(
            plan(
                WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE,
                vec![json!("company"), Value::Null, Value::Null, Value::Null]
            )
            .is_ok()
        );
        assert!(
            plan(
                WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE,
                vec![json!("company"), json!("expense-1"), json!("expense"), json!("expense-1")]
            )
            .is_ok()
        );
        assert!(
            plan(WF_NATIVE_APPROVAL_ADMINISTRATION_QUEUE, vec![json!("company")]).is_err()
        );
    }

    /// The owner-local legal decision history read stays fixed, SELECT-only and
    /// company-scoped on the Workflow-owned approval tables; the bridge carries
    /// no receipt or assertion semantics and the planner never accepts caller
    /// SQL or writes.
    #[test]
    fn legal_decision_history_read_kind_is_fixed_company_scoped_select() {
        let (sql, expected) = statement(WF_NATIVE_LEGAL_DECISION_HISTORY)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 2);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(
            sql.contains("approval_decisions") && sql.contains("approval_instances"),
            "read must stay owner-local: {sql}"
        );
        assert!(sql.contains("i.company_id=?"), "read must stay company-scoped: {sql}");
        assert!(
            sql.contains("i.record_type='legal'") && sql.contains("ORDER BY d.insertion_order"),
            "read must keep the exact owner predicate and order: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_LEGAL_DECISION_HISTORY));
        assert!(
            plan(
                WF_NATIVE_LEGAL_DECISION_HISTORY,
                vec![json!("legal-1"), json!("company")]
            )
            .is_ok()
        );
        assert!(plan(WF_NATIVE_LEGAL_DECISION_HISTORY, vec![json!("legal-1")]).is_err());
    }

    /// The owner-local flow definitions instance read stays fixed, SELECT-only
    /// and company-scoped on the Workflow-owned approval tables; the bridge
    /// carries no receipt or assertion semantics and the planner never accepts
    /// caller SQL or writes.
    #[test]
    fn flow_definitions_instance_read_kind_is_fixed_company_scoped_select() {
        let (sql, expected) = statement(WF_NATIVE_FLOW_DEFINITIONS_INSTANCE)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 3);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(sql.contains("approval_instances"), "read must stay owner-local: {sql}");
        assert!(sql.contains("company_id=?"), "read must stay company-scoped: {sql}");
        assert!(
            sql.contains("record_type=? AND record_id=?")
                && sql.contains("ORDER BY insertion_order DESC LIMIT 1"),
            "read must keep the exact owner predicate and order: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_FLOW_DEFINITIONS_INSTANCE));
        assert!(
            plan(
                WF_NATIVE_FLOW_DEFINITIONS_INSTANCE,
                vec![json!("company"), json!("expense"), json!("record-1")]
            )
            .is_ok()
        );
        assert!(plan(WF_NATIVE_FLOW_DEFINITIONS_INSTANCE, vec![json!("company")]).is_err());
    }

    /// The owner-local finance sales-invoice completion receipt read stays
    /// fixed, SELECT-only and company-scoped on the Workflow-owned command
    /// ledger; the bridge carries no receipt or assertion semantics and the
    /// planner never accepts caller SQL or writes.
    #[test]
    fn finance_sales_completion_read_kind_is_fixed_company_scoped_select() {
        let (sql, expected) = statement(WF_NATIVE_FINANCE_SALES_COMPLETION)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 2);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(sql.contains("command_receipts"), "read must stay owner-local: {sql}");
        assert!(
            sql.contains("authority_json::jsonb->>'companyId'=?"),
            "read must stay company-scoped: {sql}"
        );
        assert!(
            sql.contains("record_id=?")
                && sql.contains("operation='finance.sales-invoice.prepare'")
                && sql.contains("ORDER BY created_at DESC LIMIT 1"),
            "read must keep the exact owner predicate and order: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_FINANCE_SALES_COMPLETION));
        assert!(
            plan(
                WF_NATIVE_FINANCE_SALES_COMPLETION,
                vec![json!("invoice-1"), json!("company")]
            )
            .is_ok()
        );
        assert!(
            plan(WF_NATIVE_FINANCE_SALES_COMPLETION, vec![json!("invoice-1")]).is_err()
        );
    }

    /// The owner-local finance AI source revision read stays fixed, SELECT-only
    /// and company-scoped on the Workflow-owned command ledger; the bridge
    /// carries no receipt or assertion semantics and the planner never accepts
    /// caller SQL or writes. The exact owner operation-kind predicate is kept.
    #[test]
    fn finance_source_revision_read_kind_is_fixed_company_scoped_select() {
        let (sql, expected) = statement(WF_NATIVE_FINANCE_SOURCE_REVISION)
            .expect("registered owner read kind");
        assert!(sql.starts_with("SELECT "), "read must be SELECT-only: {sql}");
        assert_eq!(expected, 1);
        assert_eq!(sql.matches('?').count(), expected);
        assert!(sql.contains("command_receipts"), "read must stay owner-local: {sql}");
        assert!(
            sql.contains("authority_json::jsonb->>'companyId'=?"),
            "read must stay company-scoped: {sql}"
        );
        assert!(
            sql.contains("operation LIKE 'finance.%'")
                && sql.contains("operation LIKE 'people.payroll.%'")
                && sql.contains("operation NOT LIKE 'finance.ai.%'")
                && sql.contains("operation NOT LIKE 'expense.ai.%'"),
            "read must keep the exact owner operation-kind predicate: {sql}"
        );
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!sql.contains(forbidden), "read must not mutate: {forbidden}");
        }
        assert!(supported(WF_NATIVE_FINANCE_SOURCE_REVISION));
        assert!(plan(WF_NATIVE_FINANCE_SOURCE_REVISION, vec![json!("company")]).is_ok());
        assert!(plan(WF_NATIVE_FINANCE_SOURCE_REVISION, vec![]).is_err());
    }
}
