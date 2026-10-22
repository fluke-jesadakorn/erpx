import { z } from "zod";
export type Row = Record<string, string | number | null>;
export const money = z.string().regex(/^\d+(\.\d{1,2})?$/), reason = z.string().trim().min(3).max(2000);
export const ownershipEditionSchema = z
    .object({
    investorCompanyId: z.string().min(1),
    investeeCompanyId: z.string().min(1),
    method: z.enum(["subsidiary", "equity", "cost", "excluded"]),
    ownershipBps: z.number().int().min(0).max(10000),
    votingBps: z.number().int().min(0).max(10000),
    controlAssessment: reason,
    effectiveFrom: z.string().date(),
    effectiveTo: z.string().date().optional(),
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    reason,
})
    .superRefine((v, c) => {
    if (Boolean(v.parentId) !== Boolean(v.parentVersion))
        c.addIssue({
            code: "custom",
            path: ["parentVersion"],
            message: "Retain the exact ownership edition being revised.",
        });
    if (v.investorCompanyId === v.investeeCompanyId)
        c.addIssue({
            code: "custom",
            message: "Select different investor and investee Companies.",
        });
    if (v.effectiveTo && v.effectiveTo < v.effectiveFrom)
        c.addIssue({
            code: "custom",
            message: "The effective end cannot precede its start.",
        });
    if (v.method === "equity" && v.ownershipBps === 0)
        c.addIssue({
            code: "custom",
            message: "An equity-method interest needs a positive ownership percentage.",
        });
});
export const acquisitionEditionSchema = z
    .object({
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    ownershipId: z.string().min(1),
    ownershipVersion: z.number().int().positive(),
    acquisitionDate: z.string().date(),
    currency: z
        .string()
        .length(3)
        .transform((v) => v.toUpperCase()),
    consideration: money,
    previousInterest: money.default("0"),
    nciMeasurement: z.enum(["fair_value", "proportionate"]),
    nciFairValue: money.default("0"),
    identifiableNetAssets: money,
    deferredTax: money.default("0"),
    cguReference: z.string().trim().min(1).max(200),
    reason,
})
    .superRefine((input, context) => {
    if (Boolean(input.parentId) !== Boolean(input.parentVersion))
        context.addIssue({
            code: "custom",
            path: ["parentVersion"],
            message: "Retain the exact acquisition edition being corrected.",
        });
});
export const groupEvidenceDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason,
});
