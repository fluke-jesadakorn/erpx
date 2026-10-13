import { z } from "zod";
import { decimalToUnits } from "@/lib/finance/finance-money";
export type Row = Record<string, string | number | null>;
export const rate = z
    .string()
    .regex(/^\d+(\.\d{1,6})?$/)
    .refine((v) => decimalToUnits(v, 6) > BigInt(0));
export const groupCode = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/);
export const category = z.enum([
    "asset",
    "liability",
    "equity",
    "revenue",
    "expense",
    "other_income",
    "other_expense",
]);
export const groupAccount = z.object({
    code: groupCode,
    name: z.string().trim().min(1).max(200),
    category,
});
export const groupReportEditionSchema = z
    .object({
    startDate: z.string().date(),
    endDate: z.string().date(),
    currency: z
        .string()
        .length(3)
        .transform((v) => v.toUpperCase()),
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    entities: z
        .array(z.object({
        companyId: z.string().min(1),
        chartId: z.string().min(1),
        periodIds: z.array(z.string().min(1)).min(1).max(24),
        method: z.enum(["full", "equity"]).default("full"),
    }))
        .min(2)
        .max(20),
    mappings: z
        .array(z.object({
        accountId: z.string().min(1),
        group: groupAccount,
        fxType: z.enum(["current", "average", "historical"]),
        historicalRate: rate.optional(),
    }))
        .min(1)
        .max(10000),
    rates: z
        .array(z.object({
        currency: z
            .string()
            .length(3)
            .transform((v) => v.toUpperCase()),
        current: rate,
        average: rate,
    }))
        .max(50)
        .default([]),
    intercompanyPairIds: z.array(z.string().min(1)).max(500).default([]),
    acquisitionEliminations: z
        .array(z.object({
        acquisitionId: z.string().min(1),
        investmentAccountId: z.string().min(1),
        goodwill: groupAccount,
        nci: groupAccount,
        bargainPurchase: groupAccount,
    }))
        .max(20)
        .default([]),
    adjustments: z
        .array(z.object({
        calculationRunId: z.string().min(1),
        ownershipId: z.string().min(1),
        acquisitionId: z.string().min(1).optional(),
        output: z.enum([
            "goodwill",
            "bargainPurchase",
            "nciClosing",
            "nciProfitShare",
            "equityMethodClosing",
        ]),
        debit: groupAccount,
        credit: groupAccount,
        existingDebitBalance: z
            .string()
            .regex(/^-?\d+(\.\d{1,2})?$/)
            .default("0.00"),
    }))
        .max(100)
        .default([]),
    reason: z.string().trim().min(3).max(2000),
})
    .superRefine((v, c) => {
    for (const [path, values] of [["intercompanyPairIds", v.intercompanyPairIds], ["acquisitionEliminations", v.acquisitionEliminations.map(a => a.acquisitionId)], ["adjustments", v.adjustments.map(a => `${a.ownershipId}:${a.output}`)]] as const) {
        if (new Set(values).size !== values.length)
            c.addIssue({ code: "custom", path: [path], message: "Retain each document pair, acquisition or ownership adjustment once." });
    }
    if (v.startDate > v.endDate)
        c.addIssue({
            code: "custom",
            path: ["endDate"],
            message: "The reporting end must follow its start.",
        });
    if (new Set(v.entities.map((e) => e.companyId)).size !== v.entities.length)
        c.addIssue({
            code: "custom",
            path: ["entities"],
            message: "Select one explicit book per Company.",
        });
    if (new Set(v.mappings.map((m) => m.accountId)).size !== v.mappings.length)
        c.addIssue({
            code: "custom",
            path: ["mappings"],
            message: "Map each account once.",
        });
    if (new Set(v.rates.map((r) => r.currency)).size !== v.rates.length)
        c.addIssue({
            code: "custom",
            path: ["rates"],
            message: "Provide each currency's rates once.",
        });
});
export const groupReportDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export type Input = z.infer<typeof groupReportEditionSchema>;
export const profitCategory = (c: string) => ["revenue", "expense", "other_income", "other_expense"].includes(c);
