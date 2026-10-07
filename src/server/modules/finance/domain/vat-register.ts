import { z } from 'zod';
import type { FinanceError } from './invoice-line-revision';
export type TaxRow = Record<string, string | number | null>;
export const month = z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/);
const reason = z.string().trim().min(10).max(4000);
export const vatEntrySchema = z.object({
    sourceType: z.enum(["invoice", "commerce_return", "billing_credit"]), sourceId: z.string().min(1).max(128), expectedSourceVersion: z.number().int().positive(),
    taxPointDate: z.string().date(), claimMonth: month, documentReference: z.string().trim().min(1).max(200),
    treatment: z.enum(["standard", "zero", "exempt", "disallowed", "allocated"]), deductibleBps: z.number().int().min(0).max(10000),
    legalBasis: reason, taxPointEvidence: reason, documentEvidence: reason,
});
export const taxReviewSchema = z.object({ expectedVersion: z.number().int().positive(), reason });
export const vatCorrectionSchema = vatEntrySchema.omit({ sourceType: true, sourceId: true }).extend({ expectedEntryVersion: z.number().int().positive(), reason });
export const vatPackageSchema = z.object({ chartId: z.string().min(1), filingMonth: month, filingKind: z.enum(["original", "additional"]), originalPackageId: z.string().min(1).optional(), reason });
export const vatFilingSchema = taxReviewSchema.extend({ filingReference: z.string().trim().min(1).max(200), filedDate: z.string().date(), receiptEvidence: reason });
export function monthDates(value: string) {
    month.parse(value);
    const [year, n] = value.split("-").map(Number);
    return { start: `${value}-01`, end: `${value}-${new Date(Date.UTC(year, n, 0)).getUTCDate()}` };
}
export type VatEntryInput = z.infer<typeof vatEntrySchema>;
export type VatReviewInput = z.infer<typeof taxReviewSchema>;
export type VatCorrectionInput = z.infer<typeof vatCorrectionSchema>;
export type VatPackageInput = z.infer<typeof vatPackageSchema>;
export type VatFilingInput = z.infer<typeof vatFilingSchema>;
export type VatSourceType = VatEntryInput['sourceType'];
export type VatSource = {
    source: TaxRow;
    sourceHash: string;
    direction: string;
    sign: number;
    ledger: number;
};
export type VatRegister = {
    json: string;
    entries: TaxRow[];
    sourceHash: string;
};
export type VatReconciliation = {
    rows: TaxRow[];
    missing: number;
    pendingCorrections: number;
    ready: boolean;
};
export type VatFamily = {
    entries: TaxRow[];
    returns: Array<{
        id: string;
        status: string;
    }>;
    billingCredits: Array<{
        id: string;
        status: string;
    }>;
};
export type VatCorrectionMember = {
    previous: TaxRow;
    current: VatSource;
    facts: VatEntryInput;
    original: TaxRow | null;
    id: string;
};
export type VatResult = {
    outputVatMinor: number;
    inputVatMinor: number;
    deductibleInputMinor: number;
    disallowedInputMinor: number;
    netMinor: number;
    payableMinor: number;
    creditMinor: number;
};
export type VatSnapshot = {
    register: TaxRow[];
    reconciliation: VatReconciliation;
    profileId: TaxRow[string];
    result: VatResult;
    priorFiledResult: VatResult | null;
    preparationOnly: true;
    officialFormatValidated: false;
    priorCreditsIncluded: false;
};
export type VatCloseEvidence = {
    profileId: string;
    chartId: string;
    effectiveDate: string;
    endDate: string;
    rows: TaxRow[];
    missing: number;
    pendingCorrections: number;
    ready: boolean;
};
export type VatCorrectionDetail = {
    correction: TaxRow;
    entries: Array<{
        previous: TaxRow;
        replacement: TaxRow;
    }>;
};
export function createVatFactsPolicy(error: FinanceError) {
    function validateVatFacts(input: z.infer<typeof vatEntrySchema>, direction: string, ledger: number, source: TaxRow, original: TaxRow | null) {
        if (input.claimMonth < input.taxPointDate.slice(0, 7) || (direction === "output" && input.claimMonth !== input.taxPointDate.slice(0, 7)))
            throw error(422, "VAT_CLAIM_MONTH_INVALID", "Output VAT belongs to its tax-point month; input VAT cannot be claimed before that month.");
        if (direction === "output" && (["disallowed", "allocated"].includes(input.treatment) || input.deductibleBps !== 10000))
            throw error(422, "OUTPUT_VAT_TREATMENT_INVALID", "Input-tax deductions do not apply to output VAT.");
        if (["zero", "exempt"].includes(input.treatment) && ledger !== 0)
            throw error(422, "VAT_TREATMENT_MISMATCH", "A zero-rated or exempt document must have zero posted VAT.");
        const expectedDeduction = input.treatment === "disallowed" ? 0 : 10000;
        if (input.treatment !== "allocated" && input.deductibleBps !== expectedDeduction)
            throw error(422, "VAT_DEDUCTION_INVALID", "Use the treatment's full or prohibited deduction; proportional deduction requires allocated treatment.");
        if (input.sourceType !== "invoice") {
            if (!original)
                throw error(409, "ORIGINAL_TAX_ENTRY_REQUIRED", "Review the original invoice's tax entry before its credit/debit note.");
            if (input.taxPointDate < String(original.tax_point_date) || input.taxPointDate < String(source.document_date))
                throw error(422, "RETURN_TAX_DATE_INVALID", "A return tax adjustment cannot precede its original VAT point or return document date.");
            if (original.treatment !== input.treatment || original.deductible_bps !== input.deductibleBps)
                throw error(422, "RETURN_VAT_POLICY_MISMATCH", "The return must retain the original document's reviewed deduction policy.");
        }
    }
    return { validateVatFacts };
}
export type VatCommandResult = {
    id: string;
    status: string;
    version: number;
    taxMinor?: number;
    deductibleMinor?: number;
    entryCount?: number;
    result?: VatResult;
    filename?: string;
    content?: string;
    exportHash?: string;
};
export type VatReviewResult = {
    id: string;
    status: string;
    version: number;
};
export type VatEntryResult = VatReviewResult & {
    taxMinor: number;
    deductibleMinor: number;
};
export type VatCorrectionResult = VatReviewResult & {
    entryCount: number;
};
export type VatPackageResult = VatReviewResult & {
    result: VatResult;
};
export type VatExportResult = VatReviewResult & {
    filename: string;
    content: string;
    exportHash: string;
};
export type VatTaxEntryPlan = {
    id: string;
    chartId: TaxRow[string];
    financialEventId: TaxRow[string];
    taxAccountId: TaxRow[string];
    sourceType: VatSourceType;
    sourceId: string;
    sourceVersion: TaxRow[string];
    originalEntryId: TaxRow[string];
    direction: string;
    documentDate: TaxRow[string];
    accountingDate: TaxRow[string];
    taxPointDate: string;
    claimMonth: string;
    documentReference: string;
    treatment: VatEntryInput['treatment'];
    deductibleBps: number;
    taxableMinor: number;
    taxMinor: number;
    deductibleMinor: number;
    sourceHash: string;
    evidenceJson: string;
    createdAt: number;
    replacesEntryId: string | null;
    correctionId: string | null;
};
export function createVatTaxEntryPlan(id: string, input: VatEntryInput, current: VatSource, original: TaxRow | null, createdAt: number, correction?: {
    id: string;
    replacesId: string;
    reason: string;
}): VatTaxEntryPlan { const { source, sourceHash, sign, ledger, direction } = current; return { id, chartId: source.chart_id, financialEventId: source.financial_event_id, taxAccountId: source.tax_account_id, sourceType: input.sourceType, sourceId: input.sourceId, sourceVersion: source.version, originalEntryId: original?.id ?? null, direction, documentDate: source.document_date, accountingDate: source.accounting_date, taxPointDate: input.taxPointDate, claimMonth: input.claimMonth, documentReference: input.documentReference, treatment: input.treatment, deductibleBps: input.deductibleBps, taxableMinor: sign * Number(source.subtotal_minor), taxMinor: ledger, deductibleMinor: sign * Math.round(Math.abs(ledger) * input.deductibleBps / 10000), sourceHash, evidenceJson: JSON.stringify({ facts: input, source, originalDocumentReference: original?.document_reference ?? null, correction: correction ?? null }), createdAt, replacesEntryId: correction?.replacesId ?? null, correctionId: correction?.id ?? null }; }
