import { z } from "zod";
import { startSupportClock, supportSlaPolicySchema, type SupportClock, type SupportSlaPolicy } from "@erpx/server-support/domain/support-clock";
import type { SupportRow as Row } from "@erpx/server-support/domain/support-facts";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
const reason = z.string().trim().min(3).max(2000), version = z.number().int().positive();
export const supportPolicyCreateSchema = z.object({ name: z.string().trim().min(3).max(160), policy: supportSlaPolicySchema, reason });
export const supportPolicyChangeSchema = z.object({ action: z.enum(['approve', 'retire']), expectedVersion: version, reason });
export const supportClockAttachSchema = z.object({ expectedVersion: version, policyId: z.string().min(1), policyVersion: version, reason });
export type SupportPolicyCreate = z.infer<typeof supportPolicyCreateSchema>;
export type SupportPolicyChange = z.infer<typeof supportPolicyChangeSchema>;
export type SupportClockAttach = z.infer<typeof supportClockAttachSchema>;
export function supportClockValue(row: Row): SupportClock | null { return row.sla_clock_json ? JSON.parse(String(row.sla_clock_json)) as SupportClock : null; }
export function supportPolicyWorkItems(a: WorkContext, rows: Row[]): WorkItem[] { return rows.map(p => { const pending = p.status === 'draft', canAct = pending && hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId && !samePerson(a, p.created_by) && Number(p.actor_eligible) > 0; return { id: String(p.id), type: 'support_policy', title: `Resolution-time policy · ${p.name}`, detail: 'Review the proposed working calendar and target before use.', href: '/support/sla', status: String(p.status), updatedAt: Number(p.updated_at), queue: pending ? canAct ? 'todo' : 'waiting' : 'completed', canAct, responsibleQueue: 'Independent Company Support manager', nextAction: pending ? 'Review or retire the proposed policy' : 'Read the retained policy snapshot', blocker: pending && !Number(p.reviewers) ? 'No independent eligible Company Support manager; administrator coverage is required' : null, relationships: [p.created_by === a.userId ? 'requested' : canAct ? 'approval' : 'team'] }; }); }
export function supportPolicyClock(policy: SupportSlaPolicy, now: number, error: (status: number, code: string, message: string) => Error) { try {
    return startSupportClock(policy, now);
}
catch {
    throw error(422, 'SUPPORT_CALENDAR_CAPACITY', 'This calendar cannot fit the target within 366 days. Review its working days, hours and holidays.');
} }
