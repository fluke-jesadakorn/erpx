import "server-only";
import type { WorkContext } from "./authorization";
import { publicScope } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { sourceRecordLabel } from "@/lib/source-record";
import type { WorkReview, WorkReviewKind, ReviewDecision } from "@/lib/work-review";
import { holdingResources } from "./runtime";
import { employeeRequestDetail, type SavedApproval } from "./employee-requests";
import { expenseDetail } from "./expenses";
import { getLeave } from "./leave";
import { getLeaveEvidence } from "./work";
import { taskDetail } from "./staff-tasks";
import { resolveFlow } from "./flow-routing";
import type { ApprovalView } from "./approvals";
import { attachmentContentUrl } from "@/lib/attachment-client";
import { ApiError } from "./http";
import { minorToDecimal } from "@/lib/finance-money";

export async function getWorkReview(actor: WorkContext, kind: WorkReviewKind, id: string): Promise<WorkReview> {
  const { write } = await holdingResources(actor.holdingId);
  const base = workspaceBase(publicScope(actor));
  const href = `${base}/${kind === "expense" ? "expenses" : kind === "leave" ? "tasks/people_payroll" : kind === "staff_task" ? "staff-tasks" : "requests"}/${encodeURIComponent(id)}`;
  const result: WorkReview = { source: { kind, id, href, label: sourceRecordLabel(kind) }, title: "", requester: "", status: "", version: 0,
    description: "", facts: [], evidence: [], steps: [], currentStep: "", nextStep: "", blocker: null, permittedActions: [] };
  let actions: string[] = [];
  let approval: ApprovalView | undefined;
  let requesterId = "";
  if (kind === "expense") {
    const detail = await expenseDetail(actor, id), row = detail.expense;
    result.title = String(row.merchant || "Expense claim");
    result.requester = String(row.legal_name || "Claimant");
    result.status = String(row.status); result.version = Number(row.version);
    result.description = String(row.business_purpose ?? row.description ?? "");
    result.facts = [{ label: "Amount", value: `${row.currency} ${minorToDecimal(Number(row.amount_minor) + Number(row.tax_minor))}` },
      { label: "Expense date", value: String(row.expense_date ?? "—") },
      { label: "Settlement", value: row.request_kind === "payment" ? "Company must pay" : "Employee reimbursement" }];
    result.evidence = detail.attachments.map(file => ({ name: String(file.original_name), href: `${href}#attachments`,contentHref:`/api/v1${base}/attachments/${file.id}/content`,previewHref:`/api/v1${base}/attachments/${file.id}/content?variant=preview`,mediaType:String(file.media_type??""),bytes:Number(file.bytes) }));
    approval = detail.approval; actions = detail.permittedActions;
    if (["accounts_review", "accounts_approval", "gl_posting", "payment"].includes(String(row.workflow_stage))) {
      result.fullReviewReason = "Review accounting details and payment evidence in the full expense workspace.";
      actions = [];
    }
  } else if (kind === "leave") {
    const row = await getLeave(actor.holdingId, id, actor);
    result.title = row.number; requesterId = row.created_by;
    result.status = row.state; result.version = row.version; result.description = row.handover_note;
    result.facts = [{ label: "Leave type", value: row.leave_type_name ?? "Leave" }, { label: "Dates", value: `${row.start_date} → ${row.end_date ?? row.start_date}` },
      { label: "Requested units", value: String(row.calculated_units ?? row.days) }];
    approval = row.approval; actions = row.permittedActions;
    try { result.evidence = (await getLeaveEvidence(actor.holdingId, id, actor)).map(file => ({ name: file.original_name, href: `${href}#evidence`,contentHref:file.set_id?attachmentContentUrl(base,file.id,"original"):`/api/v1${base}/attachments/${file.id}/content`,previewHref:file.preview_available?(file.set_id?attachmentContentUrl(base,file.id,"preview"):`/api/v1${base}/attachments/${file.id}/content?variant=preview`):undefined,mediaType:String(file.media_type??""),bytes:Number(file.bytes) })); }
    catch (error) { if (!(error instanceof ApiError) || ![403,404].includes(error.status)) throw error;
      result.evidenceMessage = "Protected leave evidence is available only to authorized readers."; }
    if (row.cancellation_state === "pending_review") {
      result.fullReviewReason = "Review this leave cancellation in its full workspace."; actions = [];
    }
  } else if (kind === "staff_task") {
    const detail = await taskDetail(write, actor, id), row = detail.task;
    result.title = row.title; result.requester = row.creator_name; result.status = row.state; result.version = row.version;
    result.description = row.description; result.facts = [{ label: "Assigned to", value: row.assignee_name }, { label: "Project", value: row.project_title ?? "Standalone task" },
      ...(row.completion_note ? [{ label: "Completion evidence", value: row.completion_note }] : [])];
    result.evidence = detail.attachments.map(file => ({ name: file.original_name, href: `${href}#attachments`,contentHref:`/api/v1${base}/attachments/${file.id}/content`,previewHref:`/api/v1${base}/attachments/${file.id}/content?variant=preview`,mediaType:String(file.media_type??""),bytes:Number(file.bytes) }));
    approval = detail.approval; actions = detail.permittedActions; result.blocker = detail.blockedReason;
  } else {
    const detail = await employeeRequestDetail(write, actor, id), row = detail.request;
    if (`${row.kind}_request` !== kind) throw new ApiError(404, "NOT_FOUND", "Request not found.");
    result.title = row.title; requesterId = row.requester_user_id; result.status = row.state; result.version = row.version;
    result.description = row.description; result.blocker = detail.blockedReason;
    result.currentStep = detail.responsibleQueue; result.nextStep = detail.nextAction;
    result.facts = [{ label: "Impact", value: row.impact }, { label: "Needed by", value: row.needed_date ?? "—" },
      ...detail.items.map(item => ({ label: item.description || "Requested item", value: `${item.quantity} ${item.unit ?? "each"} · ${row.currency} ${item.estimatedCost || "Cost needed"}` }))];
    result.evidence = detail.attachments.map(file => ({ name: file.original_name, href: `${href}#attachments`,contentHref:`/api/v1${base}/attachments/${file.id}/content`,previewHref:`/api/v1${base}/attachments/${file.id}/content?variant=preview`,mediaType:String(file.media_type??""),bytes:Number(file.bytes) }));
    actions = detail.actions;
    const saved = JSON.parse(row.approval_json) as SavedApproval;
    const routeSteps = saved.route?.steps ?? [];
    const people = routeSteps.length ? (await write.prepare(`SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND user_id IN (${routeSteps.map(()=>"?").join(",")}) AND merged_into_id IS NULL`)
      .bind(actor.companyId,...routeSteps.map(step=>step.userId)).all<{user_id:string;legal_name:string}>()).results : [];
    const names = new Map(people.map(person=>[person.user_id,person.legal_name]));
    result.steps = routeSteps.map((step, index) => ({ label: (step.stage ?? step.authority).replaceAll("_", " "), person: names.get(step.userId) ?? "Named reviewer",
      state: index < (saved.currentStep ?? 0) ? "recorded" : index === (saved.currentStep ?? 0) ? "current" : "upcoming" }));
    if (detail.responsibleQueue === "Accounting preparation") {
      result.fullReviewReason = "Prepare the accounting and spending details in the full request workspace."; actions = [];
    }
  }
  if (requesterId) result.requester = (await write.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1")
    .bind(actor.companyId, requesterId).first<{ legal_name: string }>())?.legal_name ?? "Company member";
  if (approval) {
    result.steps = approval.steps.map(step => ({ label: step.stage?.replaceAll("_", " ") ?? `${step.authority} review`, person: step.approver_name ?? "Named approver", state: step.state }));
    result.blocker = approval.blockedReason ?? result.blocker;
    result.currentStep = approval.currentApprover?.approver_name ?? "No active review";
    result.nextStep = approval.steps.find(step => step.ordinal > (approval!.currentApprover?.ordinal ?? Infinity))?.approver_name ?? "Continue the source workflow after this review";
  }
  const flow = await resolveFlow(write, actor, kind.endsWith("_request") ? "request" : kind as "expense" | "leave" | "staff_task", id);
  if (flow.recordVersion !== result.version) throw new ApiError(409, "VERSION_CONFLICT", "This record changed while the review was loading. Reload and review its current version.");
  if (!result.steps.length) result.steps = flow.steps.map(step => ({ label: step.action, person: step.primary?.name ?? step.teamName, state: step.status }));
  result.currentStep ||= flow.current?.action ?? "No active review";
  result.nextStep = flow.next ? `${flow.next.action} · ${flow.next.primary?.name ?? flow.next.teamName}` : result.nextStep || "Continue in the source workspace";
  result.source.version = result.version;
  result.reasonMinimum = { approve: kind === "leave" ? 0 : kind === "expense" ? 3 : 2, reject: kind === "expense" ? 3 : 2, return: 2 };
  result.permittedActions = actions.filter((action): action is ReviewDecision => ["approve", "reject", "return"].includes(action));
  result.actionKind=result.permittedActions.length?"approval":"work";
  return result;
}
