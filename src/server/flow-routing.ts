import { policyRead, policyNow, hasPolicyReadSnapshot } from "./iam-read-snapshot";
import {currentPersonPrincipals} from "./iam-person";
import { hasWorkflowPermission } from "@/lib/permissions";
import type { PermissionActor } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import type { FlowKind, FlowPerson, FlowStep, FlowView } from "@/lib/flow";
import { assertion, executeCommand, replayCommand } from "./commands";
import { activeStaffSelect, companyAuthority, authorityGuard, notification, requestRoute, type StaffActor, type RecordScope } from "./staff-policy";
import { expenseRoleUsers, expenseRoute, expensePoolCandidates, expenseAuthoritySnapshot, spendingStepCandidates, spendingUsers } from "./expense-routing";
import type { ApprovalInstance, ApprovalStep } from "./approvals";
import { resolveServiceTeam, teamHandlers } from "./service-teams";
import type { SavedApproval } from "./employee-requests";
import { ApiError } from "./http";

type Row = Record<string, string | number | null>;
const tables: Record<FlowKind, string> = { expense: "finance_expenses", leave: "leave_requests", staff_task: "staff_tasks", request: "employee_requests", stock_count: "inventory_counts", transfer: "inventory_transfers", order: "commerce_orders", invoice: "finance_invoices", journal: "finance_journals", payroll: "hr_payroll_runs", record: "workspace_records" };
export async function flowRecord(db: SqlDatabase, companyId: string, kind: FlowKind, id: string) {
  const row = await policyRead(db, JSON.stringify(["flow-record", companyId, kind, id]), () => db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=?`).bind(companyId, id).first<Row>());
  if (!row) throw new ApiError(404, "FLOW_NOT_FOUND", "Workflow document not found in this company.");
  return row;
}
type Definition = { allowedRoles:string[]; key: string; action: string; role: string; ids: string[]; preferred?: string; blocker?: string; teamName?: string };
type Route = { version: number; defaults_revision: number; steps_json: string };
type SavedStep = { primaryUserId: string | null; teamName: string; defaultRevision: number };
type DefaultRow = { revision: number; step_key: string; primary_user_id: string };
const str = (value: Row[string]) => value == null ? "" : String(value);
const scopeFor = (row: Row): RecordScope => ({ company_id: str(row.company_id), branch_id: str(row.branch_id) || null, department_id: str(row.department_id) || null });
const json = <T,>(value: Row[string], fallback: T): T => value ? JSON.parse(String(value)) as T : fallback;

async function definitions(db: SqlDatabase, kind: FlowKind, row: Row, actor:StaffActor) {
  const scope = scopeFor(row), owner = str(row.requester_user_id || row.claimant_user_id || row.creator_user_id || row.created_by);
  const draftOwner = str(row.created_by || row.creator_user_id) || owner;
  const excluded = new Set([owner, str(row.created_by), str(row.prepared_by)]);
  const candidateRoles = new WeakMap<string[], string[]>();
  const roles = async (names: string[], independent = false, target = scope) => {
    const ids = [...new Set((await Promise.all(names.map(role => expenseRoleUsers(db, target, role)))).flat())].filter(id => !independent || !excluded.has(id));
    candidateRoles.set(ids,names); return ids;
  };
  const steps: Definition[] = [];
  const add = (key: string, action: string, role: string, ids: string[], preferred?: string, blocker?: string) => { steps.push({ key, action, role, ids, preferred, blocker, allowedRoles:candidateRoles.get(ids) ?? (key==="draft" && ["stock_count","transfer"].includes(kind) ? ["warehouse_manager","operations_manager","holding_owner"] : key==="draft" && kind==="order" ? [role,role==="purchasing_operator"?"purchasing_manager":"sales_manager","holding_owner"] : ["requester","assignee","team_lead","team_member","personal_approver"].includes(role) ? [] : [["named","department"].includes(role) ? "people_manager" : role]) }); };
  let current = str(row.workflow_stage || row.state || row.status), terminal = false;
  if (row.cancelled_at || ["cancelled", "completed", "closed", "reimbursed", "reversed", "void"].includes(current)) terminal = true;
  const approvalKind = kind === "staff_task" ? "task" : kind;
  const instance = ["expense", "leave", "staff_task"].includes(kind) ? await db.prepare("SELECT * FROM approval_instances WHERE company_id=? AND record_type=? AND record_id=? ORDER BY insertion_order DESC LIMIT 1").bind(scope.company_id, approvalKind, row.id).first<ApprovalInstance>() : null;
  const approvalSteps = instance ? (await db.prepare("SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal").bind(instance.id).all<ApprovalStep>()).results : [];
  const decisions = approvalSteps.filter(s => s.state === "approved").map(s => s.approver_user_id);
  if (kind === "expense") {
    if (row.workflow_version !== 2) current = ({ draft: "draft", rejected: "draft", in_review: "department_approval", approved: "gl_posting", posted: "payment", reimbursed: "completed" } as Record<string,string>)[str(row.status)] ?? current;
    add("draft", "Prepare and submit claim", "requester", [draftOwner], draftOwner);
    const fresh = approvalSteps.length ? null : await expenseRoute(db, scope.company_id, owner, [str(row.prepared_by || row.created_by)],false,row.request_kind === "payment" ? "payment" : "expense");
    for (const [index, key, label, role] of [[0,"department_approval","Approve business purpose","people_manager"],[1,"accounts_review","Prepare and check Debit / Credit","finance_preparer"],[2,"accounts_approval","Approve accounting preparation","finance_approver"]] as const) {
      const saved = approvalSteps.find(s => s.stage === key) ?? (approvalSteps.some(s=>s.stage) ? undefined : approvalSteps[index]);
      const proposed = fresh?.steps.find(s=>s.stage===key);
      if (key === "department_approval" && !saved && ((approvalSteps.length>0) || (fresh && !fresh.blockedReason && !proposed))) continue;
      let ids = saved ? saved.approver_pool_json && instance ? await expensePoolCandidates(db, instance, saved, approvalSteps) : [saved.approver_user_id] : proposed?.candidateUserIds ?? (proposed ? [proposed.userId] : []);
      if (saved?.state === "approved") ids = [saved.approver_user_id];
      add(key, label, role, ids, saved && !saved.approver_pool_json ? saved.approver_user_id : undefined, !ids.length ? fresh?.blockedReason ?? undefined : undefined);
    }
    const accounting = json<{preparedBy?: string}>(row.accounting_json, {});
    const strict=approvalSteps.some(s=>json<{spendingPolicy?:number}>(s.route_snapshot_json??null,{}).spendingPolicy);
    const posters = (strict?(await spendingUsers(db,scope,"finance_approver")).map(p=>p.user_id!).filter(id=>!excluded.has(id)):await roles(["finance_approver"], true)).filter(id => id !== accounting.preparedBy);
    add("gl_posting", "Review and post balanced GL", "finance_approver", posters, str(row.approved_by) || undefined);
    const payers = (strict?(await spendingUsers(db,scope,"treasury")).map(p=>p.user_id!).filter(id=>!excluded.has(id)):await roles(["treasury"], true)).filter(id => id !== accounting.preparedBy && id !== row.approved_by && !decisions.includes(id));
    if (row.journal_id) {
      const journal = await db.prepare("SELECT posted_by FROM finance_journals WHERE id=? AND company_id=?").bind(row.journal_id, scope.company_id).first<{posted_by: string}>();
      add("payment", "Record payment and settle payable", "treasury", payers.filter(id => id !== journal?.posted_by));
    } else add("payment", "Record payment and settle payable", "treasury", payers);
    if (row.status === "rejected") current = "draft";
  } else if (kind === "leave" || kind === "staff_task") {
    add("draft", "Prepare and submit", "requester", [draftOwner], draftOwner);
    if (kind === "leave") {
      const route = approvalSteps.length ? null : await requestRoute(db, scope.company_id, owner, "leave");
      const list = approvalSteps.length ? approvalSteps : (route?.steps ?? []).map((s, ordinal) => ({ ordinal, authority: s.authority, approver_user_id: s.userId }));
      for (const s of list) add(`approval_${s.ordinal}`, s.authority === "hr_admin" ? "HR approval" : "Department approval", s.authority, [s.approver_user_id], s.approver_user_id);
      current = current === "pending_approval" ? `approval_${instance?.current_step ?? 0}` : current === "rejected" ? "draft" : current;
      terminal ||= current === "approved";
    } else {
      const start = json<string[]>(row.start_approvers_json, []), complete = json<string[]>(row.completion_approvers_json, []);
      start.forEach((id,i) => add(`start_${i}`, "Approve task start", "personal_approver", [id], id));
      add("work", "Start / complete assigned work", "assignee", [str(row.assignee_user_id)], str(row.assignee_user_id));
      complete.forEach((id,i) => add(`completion_${i}`, "Approve task completion", "personal_approver", [id], id));
      current = current === "start_approval" ? `start_${instance?.current_step ?? 0}` : current === "completion_approval" ? `completion_${instance?.current_step ?? 0}` : ["ready","in_progress","rework"].includes(current) ? "work" : current;
      // Rerouted named approvals remain authoritative.
      for (const s of approvalSteps) { const d = steps.find(d => d.key === `${instance?.phase}_${s.ordinal}`); if (d) { d.ids = [s.approver_user_id]; d.preferred = s.approver_user_id; } }
    }
  } else if (kind === "request") {
    add("draft", "Prepare and submit request", "requester", [draftOwner], draftOwner);
    if (row.kind !== "service") {
      const approval = json<SavedApproval>(row.approval_json, {});
      const strict = approval.route?.steps.some(s=>s.snapshot?.spendingPolicy);
      const fresh = !approval.route && row.kind==="purchase" ? await expenseRoute(db,scope.company_id,owner,[str(row.created_by)],false,"purchase") : null;
      if(strict || fresh) {
        const route=approval.route??fresh!;
        const labels:Record<string,string>={department_approval:"Approve business purpose",accounts_review:"Prepare planned spending details",accounts_approval:"Approve planned spending before ordering"};
        for(const [index,step] of route.steps.entries()){
          const decision=approval.decisions?.find(d=>d.stage===step.stage);
          const pool=decision?[decision.userId]:await spendingStepCandidates(db,scope,owner,[str(row.created_by)],step,(approval.decisions??[]).filter(d=>route.steps.findIndex(s=>s.stage===d.stage)<index).map(d=>d.userId));
          add(`purchase_${step.stage}`,labels[step.stage!]??"Approve spending",step.requiredRole??"people_manager",pool,decision?.userId);
        }
        if(current==="awaiting_approval")current=`purchase_${route.steps[approval.currentStep??0]?.stage}`;
      }else{
        const route=await expenseRoute(db,scope.company_id,owner,[str(row.created_by)],true);
        add("awaiting_approval","Approve department request","people_manager",(route.steps[0]?.candidateUserIds??[]).filter(id=>!approval.pool||approval.pool.includes(id)));
      }
    }
    const resolved = row.team_id ? null : await resolveServiceTeam(db, scope, str(row.category));
    const teamId = str(row.team_id) || resolved?.team.id || null;
    const handlers = await teamHandlers(db, scope, teamId);
    const leads = handlers.filter(h => h.responsibility === "lead");
    add("queued", "Assign request to a team member", "team_lead", leads.map(h => h.user_id));
    add("in_progress", "Handle request and record outcome", "team_member", handlers.map(h => h.user_id), str(row.assignee_user_id) || undefined);
    if (teamId) {
      const team = await db.prepare("SELECT name FROM service_teams WHERE company_id=? AND id=?").bind(scope.company_id, teamId).first<{name: string}>();
      for (const s of steps.filter(s => ["queued","in_progress"].includes(s.key))) s.teamName = team?.name;
    }
    add("waiting_requester", "Reply with the requested information", "requester", [owner], owner);
    add("confirm", "Confirm the completed work", "requester", [owner], owner);
    current = ["returned","draft"].includes(current) ? "draft" : ["resolved","fulfilled"].includes(current) ? "confirm" : current;
    terminal ||= current === "rejected";
  } else if (kind === "stock_count" || kind === "transfer") {
    const target = kind === "transfer" ? { ...scope, branch_id: str(row.target_branch_id) || null, department_id: null } : { ...scope, branch_id: null, department_id: null };
    add("draft", "Prepare and submit warehouse document", "warehouse_manager", [owner], owner);
    add("submitted", kind === "transfer" ? row.delivery_mode==="transit"?"Approve transport for source dispatch":"Confirm receipt and post stock transfer" : "Approve stock count for GL review", kind === "transfer" ? "warehouse_manager" : "finance_approver", await roles(kind === "transfer" ? ["warehouse_manager","operations_manager","finance_approver","holding_owner"] : ["finance_approver","holding_owner"], true, target));
    if (kind === "stock_count") add("approved", "Review Debit / Credit and post stock adjustment", "finance_approver", await roles(["finance_approver","holding_owner"], true,target),str(row.approved_by)||undefined);
    current = current === "rejected" ? "draft" : current;
    terminal ||= current === "posted";
  } else if (kind === "order") {
    const purchase = row.kind === "purchase";
    add("draft", "Prepare and submit order", purchase ? "purchasing_operator" : "sales_operator", [owner], owner);
    add("submitted", "Approve order", purchase ? "purchasing_manager" : "sales_manager", await roles(purchase ? ["purchasing_manager","holding_owner"] : ["sales_manager","holding_owner"], true));
    add("confirmed", row.kind === "quote" ? "Convert quotation to sales order" : purchase ? "Receive goods" : "Fulfil sales order", row.kind === "quote" ? "sales_operator" : "warehouse_manager", await roles(row.kind === "quote" ? ["sales_operator","sales_manager","holding_owner"] : ["warehouse_manager","operations_manager","holding_owner"]));
    add("stock_gl", "Review and post goods movement GL", "finance_approver", await roles(["finance_approver","holding_owner"],true));
    add("invoicing", "Prepare invoice for independent GL review", "finance_preparer", await roles(["finance_preparer","finance_approver","holding_owner"]));
    const pending = await db.prepare("SELECT 1 FROM commerce_fulfillments WHERE order_id=? AND invoice_id IS NULL LIMIT 1").bind(row.id).first();
    if(purchase&&pending){
      const ready=await db.prepare("SELECT 1 FROM commerce_fulfillments f WHERE f.order_id=? AND f.invoice_id IS NULL AND (NOT EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.fulfillment_id=f.id) OR EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.fulfillment_id=f.id AND m.status='approved')) LIMIT 1").bind(row.id).first();
      if(!ready){const step=steps.find(s=>s.key==='invoicing');if(step)step.blocker='Supplier invoice matching requires independent review or supplier correction before AP preparation.';}
    }
    const stockGl = await db.prepare("SELECT 1 FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE f.order_id=? AND j.status IN ('draft','in_review') LIMIT 1").bind(row.id).first();
    if (current === "fulfilled" && stockGl) current = "stock_gl";
    else if (current === "fulfilled" && pending) current = "invoicing";
    terminal ||= ["fulfilled","converted"].includes(current);
    if (current === "partially_fulfilled") current = "confirmed";
  } else if (kind === "invoice") {
    const request = row.source_type==="commerce_fulfillment" ? await db.prepare("SELECT r.requester_user_id,r.created_by,r.approval_json FROM employee_requests r JOIN commerce_fulfillments f ON f.order_id=r.purchase_order_id JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=r.company_id WHERE f.id=? AND r.company_id=?").bind(row.source_id,scope.company_id).first<{requester_user_id:string;created_by:string;approval_json:string}>() : null;
    const savedApproval=request?JSON.parse(request.approval_json) as SavedApproval:null;
    const strict=savedApproval?.route?.steps.some(s=>s.snapshot?.spendingPolicy);
    const excludedPayers=new Set([str(row.created_by),str(row.approved_by),request?.requester_user_id,request?.created_by,...(savedApproval?.decisions??[]).map(d=>d.userId)]);
    if(row.journal_id){const journal=await db.prepare("SELECT posted_by FROM finance_journals WHERE id=? AND company_id=?").bind(row.journal_id,scope.company_id).first<{posted_by:string}>();if(journal)excludedPayers.add(journal.posted_by);}
    const accountants=strict?(await spendingUsers(db,scope,"finance_approver")).map(p=>p.user_id!).filter(id=>id!==row.created_by&&id!==request?.requester_user_id&&id!==request?.created_by):await roles(["finance_approver","holding_owner"],true);
    add("draft", "Review invoice Debit / Credit and post GL", "finance_approver", accountants);
    const payers=row.kind==="payable"?(await spendingUsers(db,scope,"treasury")).map(p=>p.user_id!):await roles(["treasury"],true);
    add("posted", row.kind === "receivable" ? "Record receipt and settle receivable" : "Record payment and attach bank slip", "treasury", payers.filter(id=>!excludedPayers.has(id)));
    current = current === "in_review" ? "draft" : current === "partially_paid" ? "posted" : current;
    terminal ||= ["paid","settled","voided"].includes(current) || (current === "posted" && Number(row.outstanding_minor) === 0);
  } else if (kind === "journal") {
    let sourcePrimary:string|undefined;
    if (["goods_receipt","delivery"].includes(str(row.source_type))) {
      const delivery=await db.prepare("SELECT order_id FROM commerce_fulfillments WHERE id=?").bind(row.source_id).first<{order_id:string}>();
      if (delivery) sourcePrimary=(await resolveFlow(db,actor,"order",delivery.order_id)).steps.find(s=>s.key==="stock_gl")?.primary?.userId;
    }
    add("draft", "Review Debit / Credit and post GL", "finance_approver", await roles(["finance_approver","holding_owner"], true),sourcePrimary);
    current = current === "in_review" ? "draft" : current;
    terminal ||= current === "posted";
  } else if (kind === "payroll") {
    add("calculate", "Calculate payroll preview", "payroll_preparer", await roles(["payroll_preparer","hr_admin"]));
    add("preview", "Review payroll preview", "payroll_approver", await roles(["payroll_approver","people_manager"], true));
    add("approved", "Production payroll GL and payment", "finance_approver", [], undefined, "A validated statutory payroll adapter is required. Approved previews cannot create GL or payment obligations.");
    if (["draft","calculating","failed"].includes(current)) current = "calculate";
  } else {
    add("draft", "Prepare and submit record", "requester", [owner], owner);
    add("in_review", "Review and approve record", "people_manager", await roles(["people_manager","holding_owner"], true));
    add("approved", "Complete approved work", "assignee", [owner], owner);
    if (current === "rejected") current = "draft";
  }
  return { scope, steps, current, terminal, instance, approvalSteps };
}

export function flowRoleAllowed(authority:string|PermissionActor,step:FlowStep|null) {
  const actor=typeof authority==="string"?{role:authority}:authority;
  return !!step && (step.allowedRoles?.length ? step.allowedRoles.some(role=>hasWorkflowPermission(actor,role)) : ["requester","assignee","team_lead","team_member","personal_approver"].includes(step.role) || hasWorkflowPermission(actor,["named","department"].includes(step.role)?"people_manager":step.role));
}

type FlowHistoryRow = { kind: string; record_id: string; action: string; actor_name: string; created_at: number; reason: string; step_key: string };
// Batch common reference data once for a Work read. Command/detail reads retain
// their original per-document queries and do not reuse authorization decisions.
function sharedFlowReferences(db: SqlDatabase, companyId: string) {
  return policyRead(db, JSON.stringify(["flow-references", companyId]), async () => {
    const [routes, defaults, events] = await Promise.all([
      db.prepare("SELECT * FROM flow_routes WHERE company_id=?").bind(companyId).all<Route & { kind: string; record_id: string }>(),
      db.prepare("SELECT * FROM flow_defaults WHERE company_id=? ORDER BY CASE WHEN department_id IS NOT NULL THEN 3 WHEN branch_id IS NOT NULL THEN 2 ELSE 1 END DESC,revision DESC").bind(companyId).all<DefaultRow & { kind: string; branch_id: string | null; department_id: string | null }>(),
      db.prepare("SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? ORDER BY created_at,id").bind(companyId).all<FlowHistoryRow>(),
    ]);
    const histories = new Map<string, FlowHistoryRow[]>();
    for (const event of events.results) {
      const key = `${event.kind}:${event.record_id}`;
      const rows = histories.get(key) ?? [];
      rows.push(event); histories.set(key, rows);
    }
    return { routes: new Map(routes.results.map(row => [`${row.kind}:${row.record_id}`, row])), defaults: defaults.results, histories,
      revision: defaults.results.reduce((max, row) => Math.max(max, row.revision), 0) };
  });
}
export async function resolveFlow(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
  return policyRead(db, JSON.stringify(["flow-view", actor, kind, id]), () => readFlow(db, actor, kind, id));
}
async function readFlow(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string): Promise<FlowView> {
  const row = await flowRecord(db, actor.companyId, kind, id);
  const d = await definitions(db, kind, row, actor);
  const shared = hasPolicyReadSnapshot() ? await sharedFlowReferences(db, actor.companyId) : null;
  const route = shared ? shared.routes.get(`${kind}:${id}`) : await db.prepare("SELECT * FROM flow_routes WHERE company_id=? AND kind=? AND record_id=?").bind(actor.companyId, kind, id).first<Route>();
  const saved = route ? JSON.parse(route.steps_json) as Record<string,SavedStep> : {};
  const defaultsRevision = route?.defaults_revision ?? shared?.revision ?? (await db.prepare("SELECT COALESCE(MAX(revision),0) revision FROM flow_defaults WHERE company_id=?").bind(actor.companyId).first<{revision:number}>())!.revision;
  const defaults = shared ? shared.defaults.filter(row => row.kind === kind && (row.branch_id === null || row.branch_id === d.scope.branch_id) && (row.department_id === null || row.department_id === d.scope.department_id) && row.revision <= defaultsRevision) : (await db.prepare(`SELECT revision,step_key,primary_user_id FROM flow_defaults WHERE company_id=? AND kind=? AND (branch_id IS NULL OR branch_id=?) AND (department_id IS NULL OR department_id=?) AND revision<=? ORDER BY CASE WHEN department_id IS NOT NULL THEN 3 WHEN branch_id IS NOT NULL THEN 2 ELSE 1 END DESC,revision DESC`).bind(actor.companyId,kind,d.scope.branch_id,d.scope.department_id,defaultsRevision).all<DefaultRow>()).results;
  const now = policyNow();
  const people = await policyRead(db, JSON.stringify(["flow-people",actor.companyId]), async () => (await db.prepare(`SELECT s.user_id,s.legal_name,s.department_id,n.display_name team_name FROM (${activeStaffSelect}) s LEFT JOIN organization_nodes n ON n.company_id=s.company_id AND n.id=s.department_id AND n.status='active'`).bind(actor.companyId,now,now).all<{user_id:string;legal_name:string;department_id:string|null;team_name:string|null}>()).results);
  const known = new Map(people.map(p => [p.user_id, {userId:p.user_id,name:p.legal_name,teamId:p.department_id,teamName:p.team_name ?? "Company"} satisfies FlowPerson]));
  if (kind === "request") {
    const members = await policyRead(db, JSON.stringify(["flow-members",actor.companyId]), async () => (await db.prepare("SELECT DISTINCT a.user_id,COALESCE(p.legal_name,a.user_id) name FROM assignments a LEFT JOIN hr_people p ON p.company_id=a.company_id AND p.user_id=a.user_id AND p.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?").bind(actor.companyId,now,now).all<{user_id:string;name:string}>()).results);
    for (const member of members) if (!known.has(member.user_id)) known.set(member.user_id,{userId:member.user_id,name:member.name,teamId:null,teamName:"Company"});
  }
  const historyRows = shared ? shared.histories.get(`${kind}:${id}`) ?? [] : (await db.prepare("SELECT action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? AND kind=? AND record_id=? ORDER BY created_at,id").bind(actor.companyId,kind,id).all<{action:string;actor_name:string;created_at:number;reason:string;step_key:string}>()).results;
  const history = historyRows.map(e => ({action:e.action,actorName:e.actor_name,at:e.created_at,reason:e.reason,step:e.step_key}));
  const identities=await currentPersonPrincipals(db,actor.companyId,[...d.steps.flatMap(s=>[...s.ids,...(s.preferred?[s.preferred]:[])]),...Object.values(saved).flatMap(s=>s.primaryUserId?[s.primaryUserId]:[]),...defaults.map(d=>d.primary_user_id)]);
  const steps: FlowStep[] = d.steps.map(s => {
    const candidates = [...new Set(s.ids.map(id=>identities.get(id)??id))].flatMap(userId => known.has(userId) ? [known.get(userId)!] : []);
    const configured = defaults.find(v => v.step_key === s.key);
    const authoritative = (kind === "leave" || kind === "staff_task" || kind === "journal" || kind === "request" && s.key === "in_progress") ? s.preferred : undefined;
    const selected = authoritative ?? saved[s.key]?.primaryUserId ?? s.preferred ?? configured?.primary_user_id ?? (candidates.length === 1 ? candidates[0].userId : null);
    const primary = candidates.find(p => p.userId === (selected?identities.get(selected)??selected:null)) ?? null;
    const status = !d.terminal && s.key === d.current ? "current" : history.some(e => e.step === s.key && !["assign","configure"].includes(e.action)) || d.approvalSteps.some(a => (a.stage === s.key || `approval_${a.ordinal}` === s.key) && a.state === "approved") ? "recorded" : "upcoming";
    return {key:s.key,action:s.action,role:s.role,allowedRoles:s.allowedRoles,teamName:s.teamName ?? primary?.teamName ?? s.role.replaceAll("_"," "),primary,candidates,status,blocker:s.blocker ?? (primary ? null : selected ? "The primary staff member is no longer eligible. Reassign with a reason." : candidates.length ? "Select a primary staff member from the eligible team." : "No eligible staff member. Configure active staff and role coverage.")};
  });
  const current = steps.find(s => s.status === "current") ?? null;
  const next = kind === "request" && current?.key === "in_progress" ? steps.find(s => s.key === "confirm") ?? null : kind === "request" && current?.key === "waiting_requester" ? steps.find(s => s.key === "in_progress") ?? null : current ? steps[steps.indexOf(current)+1] ?? null : null;
  const blocker = d.terminal ? null : str(row.blocked_reason) || current?.blocker || (!current ? "This document requires routing repair before proceeding." : null);
  return {kind,recordId:id,recordVersion:Number(row.version),version:route?.version ?? 0,defaultsRevision,state:str(row.workflow_stage || row.state || row.status),steps,current,next,canAct:!blocker && current?.primary?.userId === actor.userId && flowRoleAllowed(actor,current),canManage:await companyAuthority(db,actor),blocker,history};
}

async function routeStatements(db: SqlDatabase, actor: StaffActor, key: string, view: FlowView) {
  const args = [actor.companyId,view.kind,view.recordId];
  if (view.version) return [assertion(db,key,`flow-version-${view.kind}`,"EXISTS(SELECT 1 FROM flow_routes WHERE company_id=? AND kind=? AND record_id=? AND version=?)",[...args,view.version])];
  const revision = view.defaultsRevision;
  const saved = Object.fromEntries(view.steps.map(s => [s.key,{primaryUserId:s.primary?.userId ?? null,teamName:s.teamName,defaultRevision:revision}]));
  return [assertion(db,key,`flow-defaults-version-${view.kind}`,"(SELECT COALESCE(MAX(revision),0) FROM flow_defaults WHERE company_id=?)=?",[actor.companyId,revision]),db.prepare("INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at) VALUES(?,?,?,?,?,?)").bind(...args,revision,JSON.stringify(saved),Date.now())];
}

/** Only the document's existing business command can advance its state. */
export async function prepareFlowCommand(db: SqlDatabase, actor: StaffActor, key: string, kind: FlowKind, id: string, action: string, input: unknown, version: number) {
  const snap = await expenseAuthoritySnapshot(db,actor.companyId);
  const view = await resolveFlow(db,actor,kind,id);
  if (kind === "order" && action === "invoice") {
    const step = view.steps.find(s => s.key === "invoicing") ?? null;
    view.current = step; view.blocker = step?.blocker ?? null; view.canAct = !view.blocker && step?.primary?.userId === actor.userId && flowRoleAllowed(actor,step);
  }
  if (kind === "payroll" && action === "calculate") {
    const step = view.steps.find(s => s.key === "calculate") ?? null;
    view.current = step; view.blocker = step?.blocker ?? null; view.canAct = !view.blocker && step?.primary?.userId === actor.userId && flowRoleAllowed(actor,step);
  }
  // Cancellation, rerouting and comments retain their stricter domain-specific policy.
  const administrative = ["cancel","reverse","reopen","comment","repair","reroute"].includes(action.split(".").at(-1)!);
  if (!administrative && !view.canAct) throw new ApiError(view.blocker ? 409 : 403,"FLOW_PRIMARY_REQUIRED",view.blocker ?? `This action belongs to ${view.current?.primary?.name ?? "the responsible staff member"}.`);
  const reason = typeof input === "object" && input !== null ? String((input as Record<string,unknown>).reason ?? (input as Record<string,unknown>).note ?? "") : "";
  const person = await db.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL").bind(actor.companyId,actor.userId).first<{legal_name:string}>();
  const steps: SqlStatement[] = [snap.guard(key,"flow-authority"),...await routeStatements(db,actor,key,view),db.prepare("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,actor.companyId,kind,id,view.current?.key ?? view.state,action,actor.userId,person?.legal_name ?? actor.userId,reason,version,Date.now())];
  if (kind === "journal") {
    const parent=await db.prepare("SELECT f.order_id FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE j.id=? AND j.company_id=?").bind(id,actor.companyId).first<{order_id:string}>();
    if (parent) steps.push(...await routeStatements(db,actor,key,await resolveFlow(db,actor,"order",parent.order_id)));
  }
  // The owner update is committed between the guards and this durable notification.
  const stateColumn = ["leave","staff_task","request","order"].includes(kind) ? "state" : "status";
  const approvalIndex = `(SELECT current_step FROM approval_instances WHERE company_id=d.company_id AND record_id=d.id ORDER BY insertion_order DESC LIMIT 1)`;
  const state = kind === "expense" ? "CASE WHEN d.workflow_version=2 THEN d.workflow_stage ELSE CASE d.status WHEN 'in_review' THEN 'department_approval' WHEN 'approved' THEN 'gl_posting' WHEN 'posted' THEN 'payment' WHEN 'rejected' THEN 'draft' ELSE d.status END END" : kind === "leave" ? `CASE WHEN d.state='pending_approval' THEN 'approval_' || (${approvalIndex})::text ELSE d.state END` : kind === "staff_task" ? `CASE WHEN d.state IN ('ready','in_progress','rework') THEN 'work' WHEN d.state='start_approval' THEN 'start_' || (${approvalIndex})::text WHEN d.state='completion_approval' THEN 'completion_' || (${approvalIndex})::text ELSE d.state END` : kind === "request" ? "CASE WHEN d.state IN ('resolved','fulfilled') THEN 'confirm' WHEN d.state='returned' THEN 'draft' WHEN d.state='awaiting_approval' AND d.approval_json::jsonb #>> '{route,steps,0,snapshot,spendingPolicy}' IS NOT NULL THEN 'purchase_' || (d.approval_json::jsonb #> '{route,steps}' -> COALESCE((d.approval_json::jsonb ->> 'currentStep')::int,0) ->> 'stage') ELSE d.state END" : kind === "invoice" ? "CASE WHEN d.status='partially_paid' THEN 'posted' ELSE d.status END" : kind === "order" ? "CASE WHEN d.state='partially_fulfilled' THEN 'confirmed' WHEN d.state='fulfilled' AND EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE f.order_id=d.id AND j.status IN ('draft','in_review')) THEN 'stock_gl' WHEN d.state='fulfilled' AND EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.order_id=d.id AND f.invoice_id IS NULL) THEN 'invoicing' ELSE d.state END" : `d.${stateColumn}`;
  const recipient = kind === "request" ? `CASE WHEN d.state='in_progress' THEN d.assignee_user_id ELSE r.steps_json::jsonb -> (${state}) ->> 'primaryUserId' END` : `r.steps_json::jsonb -> (${state}) ->> 'primaryUserId'`;
  const after = [db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT ?,d.company_id,${recipient},?,d.id,d.version,?,? FROM ${tables[kind]} d JOIN flow_routes r ON r.company_id=d.company_id AND r.kind=? AND r.record_id=d.id
    WHERE d.company_id=? AND d.id=? AND (${recipient}) IS NOT NULL AND (${recipient})!=? ON CONFLICT DO NOTHING`)
    .bind(crypto.randomUUID(),kind,"Workflow updated: review your next action",Date.now(),kind,actor.companyId,id,actor.userId)];
  after.push(db.prepare(`UPDATE flow_events e SET next_step_key=(${state}),next_user_id=(${recipient}) FROM ${tables[kind]} d JOIN flow_routes r ON r.company_id=d.company_id AND r.kind=? AND r.record_id=d.id WHERE e.command_id=? AND d.company_id=? AND d.id=?`).bind(kind,key,actor.companyId,id));
  if (kind === "journal") after.unshift(db.prepare("UPDATE commerce_orders o SET version=o.version+1,updated_at=? FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE f.order_id=o.id AND j.id=? AND j.company_id=? AND j.status='posted'").bind(Date.now(),id,actor.companyId));
  if (kind === "journal") after.push(db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT ?,o.company_id,r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId','order',o.id,o.version,'Goods movement GL posted: prepare the invoice',? FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN flow_routes r ON r.company_id=o.company_id AND r.kind='order' AND r.record_id=o.id JOIN finance_journals j ON j.id=f.journal_id WHERE j.id=? AND j.status='posted' AND f.invoice_id IS NULL AND (r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId') IS NOT NULL ON CONFLICT DO NOTHING`).bind(crypto.randomUUID(),Date.now(),id));
  return { before:steps, after, view };
}

export async function assignFlowPrimary(db: SqlDatabase, actor: StaffActor, kind: FlowKind, id: string, input: {stepKey:string;userId:string;expectedVersion:number;recordVersion:number;reason:string;saveDefault:boolean}, key:string) {
  const prior = await replayCommand(db,actor,key,"flow.assign",id,{kind,...input}); if (prior) return prior;
  if (!await companyAuthority(db,actor)) throw new ApiError(403,"FLOW_ADMIN_REQUIRED","A company administrator must assign workflow responsibilities.");
  const snap = await expenseAuthoritySnapshot(db,actor.companyId);
  const view = await resolveFlow(db,actor,kind,id), step = view.steps.find(s => s.key === input.stepKey);
  if (view.version !== input.expectedVersion || view.recordVersion !== input.recordVersion) throw new ApiError(409,"VERSION_CONFLICT","The workflow changed. Refresh before assigning.");
  if (!view.current || !step || view.steps.indexOf(step) < view.steps.indexOf(view.current)) throw new ApiError(409,"FLOW_STEP_LOCKED","Historical steps cannot be reassigned.");
  const person = step.candidates.find(p => p.userId === input.userId);
  if (!person) throw new ApiError(422,"FLOW_STAFF_INELIGIBLE","Choose an active eligible staff member for this step.");
  const row = await flowRecord(db,actor.companyId,kind,id), scope = scopeFor(row);
  const existing = await db.prepare("SELECT steps_json FROM flow_routes WHERE company_id=? AND kind=? AND record_id=?").bind(actor.companyId,kind,id).first<{steps_json:string}>();
  const saved: Record<string,SavedStep> = existing ? JSON.parse(existing.steps_json) : Object.fromEntries(view.steps.map(s => [s.key,{primaryUserId:s.primary?.userId ?? null,teamName:s.teamName,defaultRevision:0}]));
  const linked:SqlStatement[]=[];
  if (kind === "journal") {
    const parent=await db.prepare("SELECT f.order_id FROM commerce_fulfillments f JOIN finance_journals j ON j.id=f.journal_id WHERE j.id=? AND j.company_id=?").bind(id,actor.companyId).first<{order_id:string}>();
    if (parent) {
      const parentFlow=await resolveFlow(db,actor,"order",parent.order_id),parentStep=parentFlow.steps.find(s=>s.key==="stock_gl");
      if (!parentStep?.candidates.some(p=>p.userId===input.userId)) throw new ApiError(422,"FLOW_STAFF_INELIGIBLE","The accountant must also be eligible for the source order.");
      linked.push(...await routeStatements(db,actor,key,parentFlow),db.prepare("UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,'{stock_gl,primaryUserId}',to_jsonb(?::text))::text,version=version+1 WHERE company_id=? AND kind='order' AND record_id=?").bind(input.userId,actor.companyId,parent.order_id));
    }
  }
  const administrator=await db.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL").bind(actor.companyId,actor.userId).first<{legal_name:string}>();
  saved[step.key] = {...saved[step.key],primaryUserId:person.userId,teamName:step.teamName};
  return executeCommand(db,actor,key,"flow.assign",{kind,...input},id,view.recordVersion,{id,version:(view.version || 1)+1},[
    authorityGuard(db,key,actor),snap.guard(key,"flow-authority"),...await routeStatements(db,actor,key,view),...linked,
    assertion(db,key,"flow-document-version",`EXISTS(SELECT 1 FROM ${tables[kind]} WHERE company_id=? AND id=? AND version=?)`,[actor.companyId,id,view.recordVersion]),
    db.prepare("UPDATE flow_routes SET steps_json=?,version=version+1 WHERE company_id=? AND kind=? AND record_id=?").bind(JSON.stringify(saved),actor.companyId,kind,id),
    ...(input.saveDefault ? [db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,branch_id,department_id,primary_user_id,created_by,reason,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(actor.companyId,kind,step.key,scope.branch_id,scope.department_id,person.userId,actor.userId,input.reason,Date.now())] : []),
    db.prepare("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES(?,?,?,?,?,?,'assign',?,?,?,?,?)").bind(crypto.randomUUID(),key,actor.companyId,kind,id,step.key,actor.userId,administrator?.legal_name ?? actor.userId,`${input.reason} → ${person.name}`,view.recordVersion,Date.now()),
    notification(db,scope,person.userId,kind,id,view.recordVersion,`${step.action}: assigned to you`),
  ],{companyId:scope.company_id,branchId:scope.branch_id,departmentId:scope.department_id});
}

export function flowCommandTarget(operation: string, id: string, input: unknown): {kind:FlowKind;id:string;action:string} | null {
  if(operation.startsWith("leave.self.")||operation.startsWith("leave.policy.")||operation.startsWith("leave.cancellation"))return null;
  if(["record.archive","record.mention","record.reference","record.suggestion","record.suggestion.decide"].includes(operation))return null;
  const rules: [string,FlowKind][] = [["expense.","expense"],["leave.","leave"],["task.","staff_task"],["employee.request.","request"],["inventory.count.","stock_count"],["inventory.transfer.","transfer"],["commerce.order.","order"],["commerce.invoice.settle","invoice"],["finance.invoice.post","invoice"],["finance.journal.post","journal"],["people.payroll.","payroll"],["record.","record"]];
  const rule = rules.find(([prefix]) => operation.startsWith(prefix));
  if (!rule) return null;
  let action = operation.slice(rule[0].endsWith(".") ? rule[0].length : operation.lastIndexOf(".")+1);
  if (action === "transition" && input && typeof input === "object") action = String((input as Record<string,unknown>).action ?? "");
  if (["create","update","revise","attachment.remove","ai","post.reverse","recalculate","review-export"].includes(action) || action.startsWith("ai.") || action.startsWith("payment.") || action === "reverse") return null;
  return {kind:rule[1],id,action};
}
