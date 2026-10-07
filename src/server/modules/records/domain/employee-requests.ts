import { samePerson } from "@/lib/iam";
import { z } from "zod";
import { serviceCategories, type RequestKind } from "@/lib/employee-services";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { RecordScope, ApprovalRoute } from "@/server/modules/operations/public/staff-policy";
export const employeeRequestSchema = z.object({
    kind: z.enum(["service", "equipment", "purchase"]),
    category: z.enum(serviceCategories.map(([id]) => id)).default("it"),
    title: z.string().trim().max(200).default(""),
    description: z.string().trim().max(10000).default(""),
    location: z.string().trim().max(300).default(""),
    impact: z.enum(["normal", "urgent", "critical"]).default("normal"),
    branchId: z.string().min(1).nullable().default(null),
    departmentId: z.string().min(1).nullable().default(null),
    neededDate: z.union([z.iso.date(), z.literal("")]).default(""),
    currency: z
        .union([z.string().regex(/^[A-Z]{3}$/), z.literal("")])
        .default(""),
    items: z
        .array(z.object({
        description: z.string().trim().max(500).default(""),
        quantity: z
            .string()
            .regex(/^(?:\d{1,9}(?:\.\d{1,3})?)?$/)
            .default(""),
        unit: z.string().trim().min(1).max(30).default("each"),
        estimatedCost: z
            .string()
            .regex(/^(?:\d{1,12}(?:\.\d{1,2})?)?$/)
            .default(""),
    }))
        .max(20)
        .default([]),
    parentRequestId: z.string().max(180).nullable().default(null),
    expectedVersion: z.number().int().positive().default(1),
});
export const requestActionSchema = z.object({
    action: z.enum([
        "submit",
        "approve",
        "return",
        "reject",
        "assign",
        "start",
        "ask",
        "reply",
        "resolve",
        "confirm",
        "reopen",
        "cancel",
        "repair",
        "link-order",
        "comment",
    ]),
    expectedVersion: z.number().int().positive(),
    override: z.boolean().default(false),
    note: z.string().trim().max(10000).default(""),
    assigneeUserId: z.string().max(160).nullable().default(null),
    teamId: z.string().max(180).nullable().default(null),
    approverUserId: z.string().max(160).nullable().default(null),
    purchaseOrderId: z.string().max(180).nullable().default(null),
    assetReference: z.string().trim().max(180).default(""),
    attachmentSetIds: z.array(z.string().min(1)).max(20).optional(),
});
export type EmployeeRequest = RecordScope & {
    engagement_id?: string | null;
    id: string;
    kind: RequestKind;
    category: string;
    title: string;
    description: string;
    location: string;
    impact: string;
    needed_date: string | null;
    items_json: string;
    currency: string;
    requester_user_id: string;
    created_by: string;
    parent_request_id: string | null;
    state: string;
    team_id: string | null;
    assignee_user_id: string | null;
    routing_json: string;
    approval_json: string;
    blocked_reason: string | null;
    resolution: string;
    purchase_order_id: string | null;
    handover_json: string;
    version: number;
    evidence_version: number;
    created_at: number;
    updated_at: number;
};
export type SavedApproval = {
    currentStep?: number;
    decisions?: {
        stage: string;
        userId: string;
        note: string;
        at: number;
        version: number;
    }[];
    pool?: string[];
    decision?: string;
    decidedBy?: string;
    version?: number;
    evidenceVersion?: number;
    route?: ApprovalRoute;
    repairedBy?: string;
};
export const terminal = ["closed", "cancelled", "rejected"];
export const editable = ["draft", "returned"];
export function requestOwn(actor: WorkContext, row: EmployeeRequest) {
    return [row.requester_user_id, row.created_by].some(id => samePerson(actor, id));
}
