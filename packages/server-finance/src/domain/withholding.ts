import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import { z } from 'zod';
import { thaiFinanceTaxPack, type ThaiWithholdingCategory } from "@erpx/feature-finance/lib/thai-finance-tax-pack";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { TaxIdentity } from '@erpx/server-finance/domain/tax-identities';
export type Row = Record<string, string | number | null>;
export type WorkspaceInvoice = Row & {
    id: string;
    invoice_number: string;
    subtotal_minor: number;
    tax_minor: number;
    total_minor: number;
    outstanding_minor: number;
    line_revision: number;
};
const amount = z.string().regex(/^\d+(?:\.\d{1,2})?$/), evidence = z.string().trim().min(10).max(2000);
const categories = Object.keys(thaiFinanceTaxPack.rates) as [
    ThaiWithholdingCategory,
    ...ThaiWithholdingCategory[]
];
export const withholdingAssessmentSchema = z.object({
    expectedVersion: z.number().int().positive(), payerIdentityId: z.string().min(1), payeeIdentityId: z.string().min(1), liabilityAccountId: z.string().min(1),
    filingMode: z.literal("ordinary").default("ordinary"), rulePackVersion: z.literal(thaiFinanceTaxPack.version), classificationEvidence: evidence,
    allocations: z.array(z.object({ invoiceId: z.string().min(1), amount })).min(1).max(100),
    lines: z.array(z.object({ invoiceId: z.string().min(1), sourceLineId: z.string().min(1), category: z.enum(categories), contractRef: z.string().trim().min(2).max(200), contractTotal: amount })).min(1).max(1000),
});
export const withholdingDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "cancel"]), reason: evidence });
export type Allocation = {
    invoiceId: string;
    amount: string;
};
export type CapturedLine = {
    invoiceId: string;
    invoiceVersion: number;
    lineRevision: number;
    sourceLineId: string;
    description: string;
    category: ThaiWithholdingCategory;
    contractRef: string;
    contractTotalMinor: number;
    allocatedGrossMinor: number;
    priorPaidGrossMinor: number;
    originalIncomeMinor: number;
    incomeMinor: number;
    priorWithheldMinor: number;
    rateBps: number;
    taxMinor: number;
    source: unknown;
};
export type Source = {
    paymentId: string;
    paymentDate: string;
    grossMinor: number;
    currency: string;
    partyId: string;
    bankAccountId: string;
    bankAccountVersion: number;
    chartId: string;
    payerIdentityId: string;
    payerIdentityVersion: number;
    payeeIdentityId: string;
    payeeIdentityVersion: number;
    liabilityAccountId: string;
    liabilityAccountVersion: number;
    allocations: Allocation[];
    lines: CapturedLine[];
    rulePackVersion: string;
    rulePackDigest: string;
};
export type Assessment = Row & {
    id: string;
    payment_id: string;
    status: string;
    source_snapshot_json: string;
    source_fingerprint: string;
    version: number;
    tax_minor: number;
    income_minor: number;
    gross_minor: number;
};
export type WithholdingInput = z.infer<typeof withholdingAssessmentSchema>;
export type WithholdingDecision = z.infer<typeof withholdingDecisionSchema>;
export type InvoiceGuardFact = {
    invoiceId: string;
    version: string | number | null;
    outstandingMinor: string | number | null;
};
export type CapturedWithholding = {
    source: Source;
    payer: TaxIdentity;
    payee: TaxIdentity;
    incomeMinor: number;
    taxMinor: number;
    bank: Row;
    liability: Row;
    invoiceGuards: InvoiceGuardFact[];
};
export function sourceInput(source: Source, expectedVersion: number, classificationEvidence: string): z.infer<typeof withholdingAssessmentSchema> { return { expectedVersion, payerIdentityId: source.payerIdentityId, payeeIdentityId: source.payeeIdentityId, liabilityAccountId: source.liabilityAccountId, filingMode: "ordinary", rulePackVersion: thaiFinanceTaxPack.version, classificationEvidence, allocations: source.allocations, lines: source.lines.map(line => ({ invoiceId: line.invoiceId, sourceLineId: line.sourceLineId, category: line.category, contractRef: line.contractRef, contractTotal: minorToDecimal(line.contractTotalMinor) })) }; }
export function createWithholdingPolicy(error: FinanceError) {
    function safe(value: unknown) { const number = Number(value); if (!Number.isSafeInteger(number) || number < 0)
        throw error(422, "WITHHOLDING_AMOUNT_INVALID", "The source amount must use exact nonnegative Thai-baht minor units."); return number; }
    function portion(amountMinor: number, paidGross: number, gross: number) { return Number((BigInt(amountMinor) * BigInt(paidGross) + BigInt(Math.floor(gross / 2))) / BigInt(gross)); }
    function inPack(date: string) { if (date < thaiFinanceTaxPack.effectiveFrom || date > thaiFinanceTaxPack.effectiveTo)
        throw error(422, "WITHHOLDING_RULES_NOT_READY", "The ordinary Thai withholding rule pack must cover the actual payment date."); }
    return { safe, portion, inPack };
}
export type ReadyWithholding = Pick<CapturedWithholding, 'source' | 'payer' | 'payee' | 'incomeMinor' | 'taxMinor'> & {
    assessment: Assessment;
};
export type ConsumptionInput = {
    commandId: string;
    paymentId: string;
    bankTransactionId: string;
    bankDate: string;
    journalId: string;
};
