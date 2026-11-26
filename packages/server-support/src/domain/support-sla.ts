import { z } from "zod";
import { startSupportClock, supportSlaPolicySchema, type SupportClock, type SupportSlaPolicy } from "@erpx/server-support/domain/support-clock";
import type { SupportRow as Row } from "@erpx/server-support/domain/support-facts";
const reason = z.string().trim().min(3).max(2000), version = z.number().int().positive();
export const supportPolicyCreateSchema = z.object({ name: z.string().trim().min(3).max(160), policy: supportSlaPolicySchema, reason });
export const supportPolicyChangeSchema = z.object({ action: z.enum(['approve', 'retire']), expectedVersion: version, reason });
export const supportClockAttachSchema = z.object({ expectedVersion: version, policyId: z.string().min(1), policyVersion: version, reason });
export type SupportPolicyCreate = z.infer<typeof supportPolicyCreateSchema>;
export type SupportPolicyChange = z.infer<typeof supportPolicyChangeSchema>;
export type SupportClockAttach = z.infer<typeof supportClockAttachSchema>;
export function supportClockValue(row: Row): SupportClock | null { return row.sla_clock_json ? JSON.parse(String(row.sla_clock_json)) as SupportClock : null; }
export function supportPolicyClock(policy: SupportSlaPolicy, now: number, error: (status: number, code: string, message: string) => Error) { try {
    return startSupportClock(policy, now);
}
catch {
    throw error(422, 'SUPPORT_CALENDAR_CAPACITY', 'This calendar cannot fit the target within 366 days. Review its working days, hours and holidays.');
} }
