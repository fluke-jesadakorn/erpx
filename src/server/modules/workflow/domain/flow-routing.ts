import { hasWorkflowPermission } from "@/lib/permissions";
import type { PermissionActor } from "@/lib/iam";
import type { FlowKind, FlowStep } from "@/lib/flow";
import type { RecordScope } from "@/server/modules/operations/public/staff-policy";
export type Row = Record<string, string | number | null>;
export type Definition = {
    allowedRoles: string[];
    key: string;
    action: string;
    role: string;
    ids: string[];
    preferred?: string;
    blocker?: string;
    teamName?: string;
};
export type Route = {
    version: number;
    defaults_revision: number;
    steps_json: string;
};
export type SavedStep = {
    primaryUserId: string | null;
    teamName: string;
    defaultRevision: number;
};
export type DefaultRow = {
    revision: number;
    step_key: string;
    primary_user_id: string;
};
export const str = (value: Row[string]) => value == null ? "" : String(value);
export const scopeFor = (row: Row): RecordScope => ({ company_id: str(row.company_id), branch_id: str(row.branch_id) || null, department_id: str(row.department_id) || null });
export const json = <T,>(value: Row[string], fallback: T): T => value ? JSON.parse(String(value)) as T : fallback;
export function flowRoleAllowed(authority: string | PermissionActor, step: FlowStep | null) {
    const actor = typeof authority === "string" ? { role: authority } : authority;
    return !!step && (step.allowedRoles?.length ? step.allowedRoles.some(role => hasWorkflowPermission(actor, role)) : ["requester", "assignee", "team_lead", "team_member", "personal_approver"].includes(step.role) || hasWorkflowPermission(actor, ["named", "department"].includes(step.role) ? "people_manager" : step.role));
}
export type FlowHistoryRow = {
    kind: string;
    record_id: string;
    action: string;
    actor_name: string;
    created_at: number;
    reason: string;
    step_key: string;
};
export function flowCommandTarget(operation: string, id: string, input: unknown): {
    kind: FlowKind;
    id: string;
    action: string;
} | null {
    if (operation.startsWith("leave.self.") || operation.startsWith("leave.policy.") || operation.startsWith("leave.cancellation"))
        return null;
    if (["record.archive", "record.mention", "record.reference", "record.suggestion", "record.suggestion.decide"].includes(operation))
        return null;
    const rules: [
        string,
        FlowKind
    ][] = [["expense.", "expense"], ["leave.", "leave"], ["task.", "staff_task"], ["employee.request.", "request"], ["inventory.count.", "stock_count"], ["inventory.transfer.", "transfer"], ["commerce.order.", "order"], ["commerce.invoice.settle", "invoice"], ["finance.invoice.post", "invoice"], ["finance.journal.post", "journal"], ["people.payroll.", "payroll"], ["record.", "record"]];
    const rule = rules.find(([prefix]) => operation.startsWith(prefix));
    if (!rule)
        return null;
    let action = operation.slice(rule[0].endsWith(".") ? rule[0].length : operation.lastIndexOf(".") + 1);
    if (action === "transition" && input && typeof input === "object")
        action = String((input as Record<string, unknown>).action ?? "");
    if (["create", "update", "revise", "attachment.remove", "ai", "post.reverse", "recalculate", "review-export", "schedule", "delegate", "delegate-revoke"].includes(action) || action.startsWith("ai.") || action.startsWith("payment.") || action.startsWith("mapping.") || action.startsWith("correction.") || action.startsWith("adjustment.") || action.startsWith("final-pay.") || action.startsWith("advance.") || action.startsWith("accrual.") || action === "reverse")
        return null;
    return { kind: rule[1], id, action };
}
