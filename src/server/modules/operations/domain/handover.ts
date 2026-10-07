import type { StaffPerson } from './staff-policy';
import { z } from "zod";
import { type HandoverItem } from "../domain/handover-time";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
export type HandoverView = {
    date: string;
    ownerKey: string;
    items: HandoverItem[];
    defaultIds: string[];
    people: {
        id: string;
        name: string;
    }[];
    unavailable: boolean;
    reason: string | null;
};
export const handoverPublishSchema = z.object({
    date: z.iso.date(), body: z.string().trim().min(1).max(3000),
    sources: z.array(z.object({ id: z.string().min(1).max(160), sourceVersion: z.number().int().positive(),
        kind: z.enum(["staff_task", "pms_record"]).default("staff_task") })).min(1).max(5),
    recipientIds: z.array(z.string().min(1).max(160)).min(1).max(20),
}).strict().superRefine((input, context) => {
    if (new Set(input.sources.map(item => item.id)).size !== input.sources.length)
        context.addIssue({ code: "custom", path: ["sources"], message: "Choose each task once." });
    if (new Set(input.recipientIds).size !== input.recipientIds.length)
        context.addIssue({ code: "custom", path: ["recipientIds"], message: "Choose each recipient once." });
});
export function peopleInScope(actor: WorkContext, people: StaffPerson[]) {
    return [...new Map(people.filter(person => person.user_id !== actor.userId &&
            (!actor.departmentId || person.department_id === actor.departmentId) &&
            (!actor.branchId || person.branch_id === actor.branchId))
            .map(person => [person.user_id, person] as const)).values()];
}
export type HandoverAcknowledgement = {
    postId: string;
    postVersion: number;
    canAcknowledge: boolean;
    acknowledged: boolean;
    recipientCount: number;
    acknowledgedCount: number;
};
