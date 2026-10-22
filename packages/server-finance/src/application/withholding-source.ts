import { createWithholdingPolicy } from '@erpx/server-finance/domain/withholding';
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { thaiFinanceTaxPack, thaiWithholdingAmount } from "@erpx/feature-finance/lib/thai-finance-tax-pack";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { TaxIdentity } from '@erpx/server-finance/domain/tax-identities';
import type { PersistedInvoiceLine } from '@erpx/server-finance/domain/invoice-line-revision';
import type { Row, Source, CapturedLine, WithholdingInput, InvoiceGuardFact, CapturedWithholding } from '@erpx/server-finance/domain/withholding';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export interface WithholdingSourcePorts {
    error: FinanceError;
    digest(value: unknown): string;
    reviewedIdentity(actor: WorkContext, id: string, partyId: string | null, date: string): Promise<TaxIdentity>;
    bank(actor: WorkContext, id: string): Promise<Row | null>;
    liability(actor: WorkContext, id: string): Promise<Row | null>;
    invoice(actor: WorkContext, id: string): Promise<Row | null>;
    previousIncome(actor: WorkContext, id: string): Promise<Row[]>;
    invoiceLines(companyId: string, id: string, revision: number): Promise<PersistedInvoiceLine[]>;
}
export function createWithholdingSource(ports: WithholdingSourcePorts) {
    const { safe, portion, inPack } = createWithholdingPolicy(ports.error);
    async function captureSource(actor: WorkContext, payment: Row, input: WithholdingInput): Promise<CapturedWithholding> {
        const date = String(payment.payment_date);
        inPack(date);
        if (payment.kind !== "payment" || payment.currency !== "THB" || !payment.party_id || !payment.bank_account)
            throw ports.error(422, "WITHHOLDING_ADAPTER_UNSUPPORTED", "Use the ordinary domestic THB supplier payment adapter; other income, foreign payees and provider filing require their owner adapter.");
        const payer = await ports.reviewedIdentity(actor, input.payerIdentityId, null, date), payee = await ports.reviewedIdentity(actor, input.payeeIdentityId, String(payment.party_id), date);
        const bank = await ports.bank(actor, String(payment.bank_account));
        const liability = await ports.liability(actor, input.liabilityAccountId);
        if (!bank || bank.base_currency !== "THB" || !liability || liability.chart_id !== bank.chart_id || liability.id === bank.id)
            throw ports.error(422, "WITHHOLDING_LEDGER_INVALID", "Select a current THB withholding liability on the payment's functional THB ledger.");
        const ids = input.allocations.map(item => item.invoiceId), gross = safe(payment.amount_minor), allocations = input.allocations.map(item => ({ ...item, amount: minorToDecimal(decimalToMinor(item.amount)) }));
        if (new Set(ids).size !== ids.length || allocations.some(item => decimalToMinor(item.amount) <= 0) || allocations.reduce((sum, item) => sum + decimalToMinor(item.amount), 0) !== gross)
            throw ports.error(422, "WITHHOLDING_ALLOCATION_MISMATCH", "Allocate the complete gross supplier payment once across recognized invoices.");
        if (new Set(input.lines.map(line => `${line.invoiceId}:${line.sourceLineId}`)).size !== input.lines.length)
            throw ports.error(422, "WITHHOLDING_LINE_DUPLICATE", "Classify each immutable source line exactly once.");
        const lines: CapturedLine[] = [], invoiceGuards: InvoiceGuardFact[] = [];
        for (const allocation of allocations) {
            const invoice = await ports.invoice(actor, allocation.invoiceId), paid = decimalToMinor(allocation.amount);
            if (!invoice || invoice.kind !== "payable" || invoice.party_id !== payment.party_id || invoice.currency !== "THB" || invoice.chart_id !== bank.chart_id || !["posted", "partially_paid"].includes(String(invoice.status)) || invoice.journal_status !== "posted" || invoice.source_type === "expense_claim" || safe(invoice.outstanding_minor) < paid || safe(invoice.credit_total_minor ?? 0) > 0 || Number(invoice.fx_rate_micros) !== 1000000)
                throw ports.error(422, "WITHHOLDING_INVOICE_INVALID", "The gross payment must settle an open, recognized domestic supplier invoice without a pending owner credit adjustment.");
            const total = safe(invoice.total_minor), priorPaid = total - safe(invoice.outstanding_minor);
            const previous = await ports.previousIncome(actor, allocation.invoiceId);
            const priorGross = new Map<string, number>();
            for (const line of previous)
                priorGross.set(String(line.assessment_id), safe(line.allocated_gross_minor));
            if ([...priorGross.values()].reduce((sum, n) => sum + n, 0) !== priorPaid)
                throw ports.error(409, "WITHHOLDING_HISTORY_NOT_RECONCILED", "Reconcile prior supplier payments and their reviewed tax receipts before calculating a later installment.");
            let sourceLines = await ports.invoiceLines(actor.companyId, allocation.invoiceId, Number(invoice.line_revision));
            if (!sourceLines.length)
                sourceLines = [{ id: `${allocation.invoiceId}:header:${invoice.line_revision}`, description: String(invoice.invoice_number), subtotal_minor: safe(invoice.subtotal_minor), tax_minor: safe(invoice.tax_minor), source_snapshot_json: JSON.stringify({ invoiceId: invoice.id, invoiceVersion: invoice.version, journalId: invoice.journal_id }), source_version: Number(invoice.line_revision) }] as typeof sourceLines;
            if (!sourceLines.length || sourceLines.some(line => safe(line.subtotal_minor) <= 0))
                throw ports.error(422, "WITHHOLDING_SOURCE_LINES_INVALID", "Classify positive immutable income lines from the recognized source.");
            const classifications = input.lines.filter(line => line.invoiceId === allocation.invoiceId);
            if (classifications.length !== sourceLines.length || classifications.some(line => !sourceLines.some(source => source.id === line.sourceLineId)))
                throw ports.error(422, "WITHHOLDING_CLASSIFICATION_INCOMPLETE", "Every immutable source line needs its actual income category and contract evidence.");
            for (const [ordinal, line] of sourceLines.entries()) {
                const classification = classifications.find(item => item.sourceLineId === line.id)!, originalIncome = safe(line.subtotal_minor), contract = decimalToMinor(classification.contractTotal);
                if (contract < originalIncome || (classification.category === "non_life_insurance" && payee.entity_kind !== "domestic_company"))
                    throw ports.error(422, "WITHHOLDING_CONTRACT_INVALID", "The reviewed contract must cover the original income; insurance premiums require the supported domestic corporate payee.");
                const past = previous.filter(item => item.source_line_id === line.id);
                if (past.some(item => item.category !== classification.category || item.contract_ref !== classification.contractRef || safe(item.contract_total_minor) !== contract))
                    throw ports.error(409, "WITHHOLDING_CLASSIFICATION_CHANGED", "Later installments must retain the independently reviewed original income classification and contract.");
                const priorIncome = past.reduce((sum, item) => sum + safe(item.income_minor), 0), priorTax = past.reduce((sum, item) => sum + safe(item.tax_minor), 0), cumulative = portion(originalIncome, priorPaid + paid, total), expectedPrior = portion(originalIncome, priorPaid, total);
                if (priorIncome !== expectedPrior)
                    throw ports.error(409, "WITHHOLDING_HISTORY_NOT_RECONCILED", "Prior tax receipts must reconcile to the original invoice income carrying amount.");
                const calculation = thaiWithholdingAmount(cumulative, classification.category, contract), tax = calculation.taxMinor - priorTax;
                if (tax < 0 || past.some(item => Number(item.rate_bps) !== calculation.rateBps))
                    throw ports.error(409, "WITHHOLDING_HISTORY_NOT_RECONCILED", "Retain the original ordinary rate and reconcile prior withheld tax before another installment.");
                lines.push({ invoiceId: allocation.invoiceId, invoiceVersion: Number(invoice.version), lineRevision: Number(invoice.line_revision), sourceLineId: String(line.id), description: String(line.description), category: classification.category, contractRef: classification.contractRef, contractTotalMinor: contract, allocatedGrossMinor: paid, priorPaidGrossMinor: priorPaid, originalIncomeMinor: originalIncome, incomeMinor: cumulative - priorIncome, priorWithheldMinor: priorTax, rateBps: calculation.rateBps, taxMinor: tax, source: { invoiceNumber: invoice.invoice_number, journalId: invoice.journal_id, journalVersion: invoice.journal_version, ordinal, originalLine: JSON.parse(line.source_snapshot_json) } });
            }
            invoiceGuards.push({ invoiceId: allocation.invoiceId, version: invoice.version, outstandingMinor: invoice.outstanding_minor });
        }
        if (input.lines.some(line => !ids.includes(line.invoiceId)))
            throw ports.error(422, "WITHHOLDING_CLASSIFICATION_INCOMPLETE", "Classification lines must belong to these exact invoice allocations.");
        const source: Source = { paymentId: String(payment.id), paymentDate: date, grossMinor: gross, currency: "THB", partyId: String(payment.party_id), bankAccountId: String(bank.id), bankAccountVersion: Number(bank.version), chartId: String(bank.chart_id), payerIdentityId: payer.id, payerIdentityVersion: Number(payer.version), payeeIdentityId: payee.id, payeeIdentityVersion: Number(payee.version), liabilityAccountId: String(liability.id), liabilityAccountVersion: Number(liability.version), allocations, lines, rulePackVersion: thaiFinanceTaxPack.version, rulePackDigest: ports.digest(thaiFinanceTaxPack) };
        const incomeMinor = lines.reduce((sum, line) => sum + line.incomeMinor, 0), taxMinor = lines.reduce((sum, line) => sum + line.taxMinor, 0);
        if (!Number.isSafeInteger(incomeMinor) || incomeMinor <= 0 || !Number.isSafeInteger(taxMinor) || taxMinor >= gross)
            throw ports.error(422, "WITHHOLDING_AMOUNT_INVALID", "The recognized income and tax must fit the gross supplier payment.");
        return { source, payer, payee, incomeMinor, taxMinor, bank, liability, invoiceGuards };
    }
    return { captureSource, safe, inPack };
}
