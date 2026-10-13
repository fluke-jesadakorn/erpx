import { z } from "zod";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance/finance-money";
const positiveMoney = z
    .string()
    .regex(/^\d+(\.\d{1,2})?$/)
    .refine((v) => decimalToUnits(v) > BigInt(0));
export const instrumentSchema = z
    .object({
    kind: z.enum(["lease", "loan"]),
    number: z.string().trim().min(1).max(100),
    name: z.string().trim().min(1).max(200),
    chartId: z.string().min(1),
    currency: z
        .string()
        .length(3)
        .transform((v) => v.toUpperCase()),
    startDate: z.string().date(),
    firstDueDate: z.string().date().optional(),
    months: z.number().int().min(1).max(240),
    annualRateBps: z.number().int().min(0).max(100000),
    payment: positiveMoney,
    principal: positiveMoney.optional(),
    bankAccountId: z.string().min(1),
    liabilityAccountId: z.string().min(1),
    interestAccountId: z.string().min(1),
    rightOfUseAccountId: z.string().min(1).optional(),
    accumulatedAccountId: z.string().min(1).optional(),
    depreciationAccountId: z.string().min(1).optional(),
    dimensions: z.record(z.string(), z.string()).default({}),
    reason: z.string().trim().min(3).max(2000),
})
    .superRefine((v, ctx) => {
    if (v.firstDueDate && v.firstDueDate <= v.startDate)
        ctx.addIssue({
            code: "custom",
            path: ["firstDueDate"],
            message: "The first payment must fall after commencement or remeasurement.",
        });
    if (v.kind === "loan" && !v.principal)
        ctx.addIssue({
            code: "custom",
            path: ["principal"],
            message: "A loan requires the actual funded principal.",
        });
    if (v.kind === "lease" &&
        (!v.rightOfUseAccountId ||
            !v.accumulatedAccountId ||
            !v.depreciationAccountId))
        ctx.addIssue({
            code: "custom",
            path: ["rightOfUseAccountId"],
            message: "Map right-of-use cost, accumulated depreciation and depreciation expense.",
        });
});
export type InstrumentInput = z.infer<typeof instrumentSchema>;
export function halfAway(numerator: bigint, denominator: bigint) {
    if (denominator <= BigInt(0))
        throw new Error("Positive denominator required.");
    const sign = numerator < BigInt(0) ? -BigInt(1) : BigInt(1), absolute = numerator < BigInt(0) ? -numerator : numerator;
    return (sign * ((BigInt(2) * absolute + denominator) / (BigInt(2) * denominator)));
}
/** Every date is anchored to the original day, avoiding Jan 31 -> Feb 28 -> Mar 28 drift. */
export function monthlyDate(anchor: string, offset: number) {
    const [year, month, day] = anchor.split("-").map(Number), target = new Date(Date.UTC(year, month - 1 + offset, 1));
    const last = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate();
    return `${target.getUTCFullYear()}-${String(target.getUTCMonth() + 1).padStart(2, "0")}-${String(Math.min(day, last)).padStart(2, "0")}`;
}
/** Fixed monthly end-of-period payments, nominal annual rate / 12. Company-reviewed policy. */
export function instrumentSchedule(raw: InstrumentInput) {
    const input = instrumentSchema.parse(raw), payment = decimalToUnits(input.payment), rate = BigInt(input.annualRateBps), denominator = BigInt(120000);
    let principal: bigint;
    if (input.kind === "loan")
        principal = decimalToUnits(input.principal!);
    else if (rate === BigInt(0))
        principal = payment * BigInt(input.months);
    else {
        const base = denominator + rate, power = base ** BigInt(input.months), discount = denominator ** BigInt(input.months);
        principal = halfAway(payment * denominator * (power - discount), rate * power);
    }
    if (principal <= BigInt(0) || principal > BigInt(Number.MAX_SAFE_INTEGER))
        throw new Error("Instrument principal exceeds supported money precision.");
    let balance = principal, previousDepreciation = BigInt(0);
    const rows = [];
    for (let i = 1; i <= input.months; i++) {
        let interest = halfAway(balance * rate, denominator), cash = payment, capital = cash - interest;
        if (i === input.months) {
            if (input.kind === "lease") {
                capital = balance;
                interest = cash - capital;
                if (interest < BigInt(0))
                    throw new Error("Lease payment precision cannot settle the final period. Review the terms.");
            }
            else {
                capital = balance;
                cash = capital + interest;
            }
        }
        else if (capital <= BigInt(0) || capital >= balance)
            throw new Error("Fixed payments must amortize the loan throughout its selected term; the last payment settles the remaining principal.");
        const opening = balance;
        balance -= capital;
        const depreciationTarget = input.kind === "lease"
            ? (principal * BigInt(i)) / BigInt(input.months)
            : BigInt(0);
        rows.push({
            ordinal: i,
            dueDate: input.firstDueDate
                ? monthlyDate(input.firstDueDate, i - 1)
                : monthlyDate(input.startDate, i),
            opening: unitsToDecimal(opening),
            payment: unitsToDecimal(cash),
            interest: unitsToDecimal(interest),
            principal: unitsToDecimal(capital),
            closing: unitsToDecimal(balance),
            depreciation: unitsToDecimal(depreciationTarget - previousDepreciation),
        });
        previousDepreciation = depreciationTarget;
    }
    return {
        policy: "monthly-nominal-rate-half-away-v1",
        principal: unitsToDecimal(principal),
        rows,
    };
}
export const depreciationPolicySchema = z.discriminatedUnion("method", [
    z.object({ method: z.literal("straight_line") }),
    z.object({
        method: z.literal("declining_balance"),
        annualRateBps: z.number().int().positive().max(10000),
    }),
    z.object({
        method: z.literal("units_of_production"),
        totalUnits: z.number().int().positive().max(1000000000),
    }),
]);
export type DepreciationPolicy = z.infer<typeof depreciationPolicySchema>;
export function depreciationAmount(input: {
    cost: bigint;
    residual: bigint;
    life: number;
    ordinal: number;
    accumulated: bigint;
    policy: DepreciationPolicy;
    priorUnits?: number;
    units?: number;
    basisOrdinal?: number;
    basisAccumulated?: bigint;
    basisUnits?: number;
}) {
    const { cost, residual, life, ordinal, accumulated } = input, p = depreciationPolicySchema.parse(input.policy), remaining = cost - residual - accumulated, basisOrdinal = input.basisOrdinal ?? 0, basisAccumulated = input.basisAccumulated ?? BigInt(0), basisUnits = input.basisUnits ?? 0;
    if (cost <= BigInt(0) ||
        residual < BigInt(0) ||
        remaining < BigInt(0) ||
        life < 1 ||
        ordinal < 1 ||
        !Number.isInteger(basisOrdinal) ||
        basisOrdinal < 0 ||
        ordinal <= basisOrdinal ||
        basisAccumulated < BigInt(0) ||
        basisAccumulated > accumulated)
        throw new Error("Invalid depreciation carrying evidence.");
    let amount: bigint;
    if (p.method === "units_of_production") {
        const prior = input.priorUnits ?? 0, units = input.units ?? 0;
        if (!Number.isInteger(prior) ||
            !Number.isInteger(units) ||
            prior < 0 ||
            units < 0 ||
            prior + units > p.totalUnits ||
            basisUnits < 0 ||
            basisUnits > prior ||
            p.totalUnits <= basisUnits)
            throw new Error("Usage must be a nonnegative integer within the reviewed lifetime units.");
        amount =
            ((cost - residual - basisAccumulated) *
                BigInt(prior + units - basisUnits)) /
                BigInt(p.totalUnits - basisUnits) -
                (accumulated - basisAccumulated);
    }
    else if (remaining === BigInt(0))
        return BigInt(0);
    else if (ordinal > life)
        throw new Error("The depreciation term is complete.");
    else if (ordinal === life)
        amount = remaining;
    else if (p.method === "straight_line")
        amount =
            ((cost - residual - basisAccumulated) * BigInt(ordinal - basisOrdinal)) /
                BigInt(life - basisOrdinal) -
                (accumulated - basisAccumulated);
    else
        amount = halfAway((cost - accumulated) * BigInt(p.annualRateBps), BigInt(120000));
    if (amount < BigInt(0))
        throw new Error("Reviewed accumulated depreciation is ahead of the selected policy.");
    return amount > remaining ? remaining : amount;
}
