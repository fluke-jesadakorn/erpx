import { z } from "zod";
import { allocateUnits, decimalToUnits, multiplyBps, unitsToDecimal, } from "@/lib/finance-money";
const money = z.string().regex(/^\d+(?:\.\d{1,2})?$/);
export const businessContractSchema = z
    .object({
    chartId: z.string().min(1),
    partyId: z.string().min(1),
    number: z.string().trim().min(1).max(80),
    name: z.string().trim().min(1).max(160),
    currency: z.string().length(3),
    startDate: z.string().date(),
    endDate: z.string().date(),
    parentId: z.string().optional(),
    expectedVersion: z.number().int().positive().optional(),
    reason: z.string().trim().min(3).max(2000),
    cycle: z.enum(["once", "monthly", "usage"]),
    pricing: z.discriminatedUnion("kind", [
        z.object({ kind: z.literal("flat"), amount: money }),
        z.object({
            kind: z.literal("tier"),
            tiers: z
                .array(z.object({
                upTo: z.number().int().positive().nullable(),
                unitPrice: money,
            }))
                .min(1)
                .max(50),
        }),
    ]),
    receivableAccountId: z.string().min(1),
    deferredAccountId: z.string().min(1),
    taxBps: z.union([z.literal(0), z.literal(700)]).default(0),
    taxCode: z.string().max(64).optional(),
    taxAccountId: z.string().optional(),
    dimensions: z.record(z.string().max(80), z.string().max(160)).default({}),
    obligations: z
        .array(z.object({
        key: z.string().regex(/^[a-zA-Z0-9_-]{1,40}$/),
        name: z.string().trim().min(1).max(160),
        standalonePrice: money,
        revenueAccountId: z.string().min(1),
        mode: z.enum(["point", "monthly", "milestone"]),
        milestones: z
            .array(z.object({
            date: z.string().date(),
            weight: z.number().int().positive().max(1000000),
        }))
            .max(120)
            .default([]),
    }))
        .min(1)
        .max(50),
})
    .superRefine((v, c) => {
    const issue = (message: string) => c.addIssue({ code: "custom", message });
    if (v.endDate < v.startDate)
        issue("Contract end must follow its start.");
    if (v.parentId && !v.expectedVersion)
        issue("An amendment must name its reviewed source version.");
    if (v.taxBps && (!v.taxAccountId || !v.taxCode))
        issue("Map the ordinary Thai VAT account and tax code.");
    if (new Set(v.obligations.map((o) => o.key)).size !== v.obligations.length)
        issue("Obligation keys must be unique.");
    if (v.obligations.some((o) => decimalToUnits(o.standalonePrice) <= BigInt(0)))
        issue("Standalone selling prices must be positive.");
    if (v.pricing.kind === "flat" &&
        decimalToUnits(v.pricing.amount) <= BigInt(0))
        issue("Flat price must be positive.");
    if (v.pricing.kind === "tier") {
        if (v.cycle !== "usage")
            issue("Tier pricing requires a usage contract.");
        const tiers = v.pricing.tiers;
        if (tiers.at(-1)?.upTo !== null ||
            tiers.some((t, i) => i < tiers.length - 1 &&
                (t.upTo === null || (i > 0 && t.upTo <= (tiers[i - 1].upTo ?? 0)))))
            issue("Tier boundaries must increase and end with an unlimited tier.");
    }
    for (const o of v.obligations) {
        if (o.mode === "milestone" &&
            (!o.milestones.length ||
                new Set(o.milestones.map((m) => m.date)).size !==
                    o.milestones.length ||
                o.milestones.some((m) => m.date < v.startDate || m.date > v.endDate)))
            issue("Milestones require unique dates inside the contract term.");
        if (o.mode !== "milestone" && o.milestones.length)
            issue("Use milestones only for milestone recognition.");
    }
});
export type BusinessContractInput = z.infer<typeof businessContractSchema>;
export const businessBillingSchema = z.object({
    expectedVersion: z.number().int().positive(),
    month: z
        .string()
        .regex(/^\d{4}-(0[1-9]|1[0-2])$/)
        .optional(),
    invoiceDate: z.string().date(),
    dueDate: z.string().date(),
    usageUnits: z.number().int().nonnegative().max(1000000000).default(0),
    usageEvidence: z.string().trim().max(2000).default(""),
});
export type RecognitionItem = {
    key: string;
    obligationKey: string;
    name: string;
    date: string;
    accountId: string;
    amount: string;
};
const day = (date: string) => Date.parse(date + "T00:00:00Z") / 86400000;
const iso = (value: number) => new Date(value * 86400000).toISOString().slice(0, 10);
function safe(value: bigint) {
    if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(0))
        throw new Error("Contract money exceeds supported precision.");
    return value;
}
export function contractBillingCalculation(contract: BusinessContractInput, raw: z.input<typeof businessBillingSchema>) {
    const input = businessBillingSchema.parse(raw);
    if (input.dueDate < input.invoiceDate)
        throw new Error("Due date cannot precede invoice date.");
    if (contract.cycle !== "usage" && input.usageUnits)
        throw new Error("Record usage only for usage contracts.");
    if (contract.cycle === "usage" && !input.usageEvidence)
        throw new Error("Reviewed usage evidence is required, including zero usage.");
    let start = contract.startDate, end = contract.endDate, monthDays = 0;
    if (contract.cycle !== "once") {
        if (!input.month)
            throw new Error("Select the billing month.");
        const [year, month] = input.month.split("-").map(Number), from = `${input.month}-01`, to = new Date(Date.UTC(year, month, 0)).toISOString().slice(0, 10);
        monthDays = day(to) - day(from) + 1;
        start = start > from ? start : from;
        end = end < to ? end : to;
        if (end < start)
            throw new Error("Billing month is outside this contract term.");
    }
    let subtotal = BigInt(0);
    if (contract.pricing.kind === "flat") {
        subtotal = decimalToUnits(contract.pricing.amount);
        if (contract.cycle === "usage")
            subtotal *= BigInt(input.usageUnits);
        if (contract.cycle === "monthly")
            subtotal =
                (subtotal * BigInt(day(end) - day(start) + 1) +
                    BigInt(Math.floor(monthDays / 2))) /
                    BigInt(monthDays);
    }
    else {
        let previous = 0, remaining = input.usageUnits;
        for (const t of contract.pricing.tiers) {
            const units = Math.min(remaining, (t.upTo ?? input.usageUnits) - previous);
            subtotal += BigInt(units) * decimalToUnits(t.unitPrice);
            remaining -= units;
            previous = t.upTo ?? input.usageUnits;
            if (!remaining)
                break;
        }
    }
    safe(subtotal);
    const tax = multiplyBps(subtotal, contract.taxBps), allocations = allocateUnits(subtotal, contract.obligations.map((o) => ({
        key: o.key,
        weight: decimalToUnits(o.standalonePrice),
    })));
    const items: RecognitionItem[] = [];
    for (const o of contract.obligations) {
        const total = allocations.find((a) => a.key === o.key)!.units;
        const dates: {
            key: string;
            weight: bigint;
        }[] = [];
        if (o.mode === "point")
            dates.push({ key: end, weight: BigInt(1) });
        if (o.mode === "monthly") {
            let cursor = start;
            while (cursor <= end) {
                const [year, month] = cursor.slice(0, 7).split("-").map(Number), last = new Date(Date.UTC(year, month, 0)).toISOString().slice(0, 10), through = last < end ? last : end;
                dates.push({
                    key: through,
                    weight: BigInt(day(through) - day(cursor) + 1),
                });
                cursor = iso(day(through) + 1);
                if (dates.length > 240)
                    throw new Error("Recognition is limited to 240 months.");
            }
        }
        if (o.mode === "milestone")
            for (const m of o.milestones.filter((m) => m.date >= start && m.date <= end))
                dates.push({ key: m.date, weight: BigInt(m.weight) });
        if (!dates.length)
            throw new Error(`No recognition date for obligation ${o.name} in the service window.`);
        for (const a of allocateUnits(total, dates))
            items.push({
                key: `${o.key}:${a.key}`,
                obligationKey: o.key,
                name: o.name,
                date: a.key,
                accountId: o.revenueAccountId,
                amount: unitsToDecimal(a.units),
            });
    }
    return {
        serviceFrom: start,
        serviceTo: end,
        billingKey: contract.cycle === "once" ? "once" : input.month!,
        subtotal: unitsToDecimal(subtotal),
        tax: unitsToDecimal(tax),
        total: unitsToDecimal(safe(subtotal + tax)),
        allocations: allocations.map((a) => ({
            key: a.key,
            amount: unitsToDecimal(a.units),
        })),
        items,
    };
}
