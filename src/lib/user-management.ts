import { z } from "zod";

export const attributeTypes = [
  "text",
  "number",
  "date",
  "checkbox",
  "single-select",
  "multi-select",
] as const;
export const targetKinds = ["person", "group", "unit"] as const;
const id = z.string().min(1).max(180);
const base = {
  id: id.optional(),
  expectedVersion: z.number().int().nonnegative(),
  reason: z.string().trim().min(3).max(1000),
};
export const attributeInput = z
  .object({
    ...base,
    label: z.string().trim().min(2).max(100),
    help: z.string().max(500).default(""),
    target: z.enum(targetKinds),
    type: z.enum(attributeTypes),
    required: z.boolean().default(false),
    options: z.array(z.string().trim().min(1).max(100)).max(100).default([]),
    archived: z.boolean().default(false),
  })
  .superRefine((v, c) => {
    if (
      v.type.endsWith("select") &&
      (!v.options.length || new Set(v.options).size !== v.options.length)
    )
      c.addIssue({
        code: "custom",
        message: "Provide distinct choices for this field.",
      });
  });
export type AttributeDefinition = z.infer<typeof attributeInput> & {
  id: string;
  version: number;
};
export const conditionInput = z.object({
  field: id,
  op: z.enum([
    "eq",
    "neq",
    "contains",
    "gt",
    "gte",
    "lt",
    "lte",
    "empty",
    "not-empty",
  ]),
  value: z
    .union([z.string().max(1000), z.number().finite(), z.boolean()])
    .optional(),
});
export const ruleInput = z.object({
  mode: z.enum(["all", "any"]),
  conditions: z.array(conditionInput).min(1).max(20),
});
export type MembershipRule = z.infer<typeof ruleInput>;
export const groupInput = z
  .object({
    ...base,
    name: z.string().trim().min(2).max(100),
    kind: z.enum(["manual", "automatic"]),
    parentId: id.nullable().default(null),
    scopeNodeId: id.nullable().default(null),
    rule: ruleInput.nullable().default(null),
    archived: z.boolean().default(false),
  })
  .superRefine((v, c) => {
    if ((v.kind === "automatic") !== !!v.rule)
      c.addIssue({
        code: "custom",
        message: "Automatic groups require a rule; manual groups do not.",
      });
  });
export type LocalGroup = z.infer<typeof groupInput> & {
  id: string;
  version: number;
  members: string[];
};
export const policyGrantInput = z.object({
  roleId: id,
  expectedRoleRevision: z.number().int().positive(),
  scopeKind: z.enum(["holding", "branch", "department"]),
  scopeNodeId: id.nullable(),
  validUntil: z.number().int().positive(),
});
export const policyInput = z.object({
  ...base,
  name: z.string().trim().min(2).max(100),
  grants: z.array(policyGrantInput).min(1).max(20),
  archived: z.boolean().default(false),
});
export type AccessPolicy = z.infer<typeof policyInput> & {
  id: string;
  version: number;
};
export const attachmentInput = z.object({
  ...base,
  policyId: id,
  target: z.enum(["group", "unit"]),
  targetId: id,
  exclusions: z.array(id).max(100).default([]),
  archived: z.boolean().default(false),
});
export type PolicyAttachment = z.infer<typeof attachmentInput> & {
  id: string;
  version: number;
};
export const membershipInput = z.object({
  groupId: id,
  expectedVersion: z.number().int().positive(),
  personIds: z.array(id).min(1).max(200),
  action: z.enum(["add", "remove"]),
  reason: base.reason,
});
export const valuesInput = z.object({
  target: z.enum(targetKinds),
  objectId: id,
  expectedVersion: z.number().int().nonnegative(),
  values: z.record(z.string(), z.unknown()),
  reason: base.reason,
});
export type AttributeValues = z.infer<typeof valuesInput> & { version: number };
export const managementChange = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("attribute"), input: attributeInput }),
  z.object({ operation: z.literal("group"), input: groupInput }),
  z.object({ operation: z.literal("policy"), input: policyInput }),
  z.object({ operation: z.literal("attachment"), input: attachmentInput }),
  z.object({ operation: z.literal("membership"), input: membershipInput }),
  z.object({ operation: z.literal("values"), input: valuesInput }),
]);
export type ManagementChange = z.infer<typeof managementChange>;
export function validateAttributeValue(
  field: AttributeDefinition,
  value: unknown,
) {
  const empty =
    value === undefined ||
    value === null ||
    value === "" ||
    (Array.isArray(value) && !value.length);
  if (empty) {
    if (field.required) throw Error(`${field.label} is required.`);
    return null;
  }
  const valid =
    field.type === "text"
      ? typeof value === "string" && value.length <= 2000
      : field.type === "number"
        ? typeof value === "number" && Number.isFinite(value)
        : field.type === "checkbox"
          ? typeof value === "boolean"
          : field.type === "date"
            ? typeof value === "string" &&
              /^\d{4}-\d{2}-\d{2}$/.test(value) &&
              !Number.isNaN(Date.parse(value)) &&
              new Date(value).toISOString().slice(0, 10) === value
            : field.type === "single-select"
              ? typeof value === "string" && field.options.includes(value)
              : Array.isArray(value) &&
                new Set(value).size === value.length &&
                value.every(
                  (v) => typeof v === "string" && field.options.includes(v),
                );
  if (!valid) throw Error(`Choose a valid value for ${field.label}.`);
  return value;
}
export const personRuleFields = [
  { id: "worker_type", label: "Worker type", type: "text" },
  { id: "employment_type", label: "Employment type", type: "text" },
  { id: "job_title", label: "Job title", type: "text" },
  { id: "branch_id", label: "Branch", type: "text" },
  { id: "department_id", label: "Department", type: "text" },
  { id: "start_date", label: "Start date", type: "date" },
] as const;
export function ruleMatches(
  rule: MembershipRule,
  facts: Record<string, unknown>,
) {
  const matches = rule.conditions.map((c) => {
    const v = facts[c.field],
      empty = v == null || v === "" || (Array.isArray(v) && !v.length);
    if (c.op === "empty") return empty;
    if (empty) return false;
    if (c.op === "not-empty") return true;
    if (c.op === "eq") return v === c.value;
    if (c.op === "neq") return v !== c.value;
    if (c.op === "contains") return Array.isArray(v) && v.includes(c.value);
    if (
      typeof v !== typeof c.value ||
      !(typeof v === "number" || typeof v === "string")
    )
      return false;
    const comparison =
      typeof v === "number"
        ? v - (c.value as number)
        : v === (c.value as string)
          ? 0
          : v < (c.value as string)
            ? -1
            : 1;
    return c.op === "gt"
      ? comparison > 0
      : c.op === "gte"
        ? comparison >= 0
        : c.op === "lt"
          ? comparison < 0
          : comparison <= 0;
  });
  return rule.mode === "all" ? matches.every(Boolean) : matches.some(Boolean);
}
export type ManagementPerson = {
  id: string;
  name: string;
  userId: string | null;
  nodes: string[];
  facts: Record<string, unknown>;
};
export type PolicyCandidate = {
  id: string;
  personId: string;
  userId: string;
  attachmentId: string;
  policyName: string;
  roleId: string;
  status: string;
  reviewId: string | null;
  reviewVersion: number;
  assignmentId: string | null;
  validUntil: number;
  explanation: string;
};
export type ManagementSnapshot = {
  attributes: AttributeDefinition[];
  groups: LocalGroup[];
  policies: AccessPolicy[];
  attachments: PolicyAttachment[];
  values: AttributeValues[];
  people: ManagementPerson[];
  nodes: { id: string; parentId: string | null; label: string; kind: string }[];
  candidates: PolicyCandidate[];
  canManage: boolean;
  canDefine: boolean;
  canManagePeople: boolean;
  canManageOrganization: boolean;
  canReview: boolean;
};
export type ManagementImpact = {
  membersAdded: string[];
  membersRemoved: string[];
  accessRemoved: string[];
  accessPending: string[];
  accessBlocked: string[];
  matchingPeople: string[];
};
