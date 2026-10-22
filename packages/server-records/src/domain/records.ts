import { z } from "zod";
export type RecordField = {
    key: string;
    label: string;
    type: string;
    value: string | number | boolean;
    options?: string[];
};
export type WorkspaceRecord = {
    moduleId: string;
    id: string;
    sectionId: string;
    title: string;
    description: string;
    status: "draft" | "in_review" | "approved" | "completed" | "rejected";
    fields: RecordField[];
    version: number;
    createdBy: string;
    updatedAt: number;
};
export type RecordRow = {
    module_id: string;
    id: string;
    section_id: string;
    title: string;
    description: string;
    status: WorkspaceRecord["status"];
    fields_json: string;
    version: number;
    created_by: string;
    updated_at: number;
};
export const fieldValue = z.union([z.string().max(4000), z.number().finite(), z.boolean()]);
export const updateRecordSchema = z.object({
    expectedVersion: z.number().int().positive(),
    fields: z.record(z.string().min(1).max(80), fieldValue),
});
export const recordFieldSchema = z.object({ key: z.string().min(1).max(80), label: z.string().min(1).max(120), type: z.string().min(1).max(30), value: fieldValue, options: z.array(z.string().max(120)).max(50).optional() });
export const createRecordSchema = z.object({ sectionId: z.string().min(1).max(128), title: z.string().trim().min(1).max(160), description: z.string().trim().max(2000).default(""), fields: z.array(recordFieldSchema).min(1).max(64) });
export const transitionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    action: z.enum(["submit", "approve", "reject", "complete", "reopen"]),
    note: z.string().max(1000).default(""),
});
export const addReferenceSchema = z.object({
    targetModule: z.string().min(1), targetId: z.string().min(1),
    kind: z.enum(["dependsOn", "blocks", "billedBy", "fulfills", "authoredWith"]),
});
export const suggestionSchema = z.object({ fieldKey: z.string().min(1).max(80) });
export const suggestionDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    action: z.enum(["apply", "reject"]),
});
export const addPrincipalMentionSchema = z.object({
    targetKind: z.literal("person"),
    targetId: z.string().min(1).max(128),
    targetLabel: z.string().min(1).max(160),
});
export const nextStatus: Record<string, Partial<Record<z.infer<typeof transitionSchema>["action"], WorkspaceRecord["status"]>>> = {
    draft: { submit: "in_review" }, in_review: { approve: "approved", reject: "rejected" },
    approved: { complete: "completed", reopen: "draft" }, rejected: { reopen: "draft" }, completed: { reopen: "draft" },
};
export type RecordScopeSelection = {
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    args: string[];
};
