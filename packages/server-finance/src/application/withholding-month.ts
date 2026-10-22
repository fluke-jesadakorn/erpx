import { z } from 'zod';
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { exportCsv } from '@erpx/server-finance/application/withholding-month-source';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { TaxPermission } from '@erpx/server-finance/domain/tax-identities';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import type { JournalInput } from '@erpx/server-finance/domain/finance';
import type { SnapshotInput, ReversalInput } from '@erpx/server-finance/domain/company-money';
import { withholdingPackageSchema, withholdingPackageDecisionSchema, withholdingRemittanceSchema, withholdingRemittanceReversalSchema, type Row, type Source, type SourceRow, type Package, type MonthlyInput, type MonthlyDecision, type RemittanceInput, type RecoveryInput, type BankInput, type BankEvidence } from '@erpx/server-finance/domain/withholding-month';
export type JournalSummary = {
    result: {
        id: string;
    };
};
export interface WithholdingMonthPorts {
    error: FinanceError;
    authorizeTax(actor: WorkContext, permission: TaxPermission): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    digest(value: unknown): string;
    sha256(value: string): Promise<string>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    capture(actor: WorkContext, month: string, formKind: 'PND3' | 'PND53'): Promise<Source>;
    package(actor: WorkContext, id: string): Promise<Package | null>;
    bankEvidence(actor: WorkContext, input: BankInput, amount: number, chartId: string, earliest: string, bankAccountId?: string): Promise<BankEvidence>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<JournalSummary>;
    prepareMoney(actor: WorkContext, input: SnapshotInput): Promise<void>;
    prepareMoneyReversal(actor: WorkContext, input: ReversalInput): Promise<void>;
    priorEdition(facts: {
        actor: WorkContext;
        input: MonthlyInput;
    }): Promise<{
        id: string;
        edition: number;
    } | null>;
    commitPackage(facts: {
        actor: WorkContext;
        key: string;
        input: MonthlyInput;
        id: string;
        source: Source;
        prior: {
            id: string;
            edition: number;
        } | null;
        csv: string;
        checksum: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitDecision(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: MonthlyDecision;
        id: string;
        status: string;
        pack: Package;
        source: Source | null;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitRemittance(facts: {
        actor: WorkContext;
        key: string;
        input: RemittanceInput;
        id: string;
        remittanceId: string;
        journal: JournalSummary;
        source: Source;
        pack: Package;
        due: SourceRow[];
        evidence: BankEvidence;
        liabilities: Map<string, number>;
        now: number;
    }): Promise<Record<string,unknown>>;
    remittance(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<Row | null>;
    commitRecovery(facts: {
        actor: WorkContext;
        key: string;
        input: RecoveryInput;
        id: string;
        journal: JournalSummary;
        remittance: Row;
        evidence: BankEvidence;
        now: number;
    }): Promise<Record<string,unknown>>;
    packageRows(facts: {
        actor: WorkContext;
    }): Promise<Row[]>;
    bankRows(facts: {
        actor: WorkContext;
    }): Promise<Row[]>;
}
export function createWithholdingMonth(ports: WithholdingMonthPorts) {
    async function packageById(actor: WorkContext, id: string) {
        const pack = await ports.package(actor, id);
        if (!pack)
            throw ports.error(404, "WITHHOLDING_PACKAGE_NOT_FOUND", "The Company monthly withholding workpaper was not found.");
        return pack;
    }
    async function freshSource(actor: WorkContext, pack: Package) {
        const source = await ports.capture(actor, pack.month, pack.form_kind);
        if (ports.digest(source) !== pack.source_fingerprint)
            throw ports.error(409, "WITHHOLDING_PACKAGE_STALE", "Payment, retained certificate or remittance evidence changed after preparation. Cancel the pending workpaper and prepare a fresh edition.");
        return source;
    }
    async function prepareWithholdingPackage(actor: WorkContext, raw: z.input<typeof withholdingPackageSchema>, key: string) {
        actor = ports.authorizeTax(actor, "finance.write");
        const input = withholdingPackageSchema.parse(raw);
        await ports.ready(actor);
        const id = `withholding-month-${key}`, previous = await ports.replay(actor, key, "finance.withholding.month.prepare", id, input);
        if (previous)
            return previous;
        const source = await ports.capture(actor, input.month, input.formKind), csv = exportCsv(source), checksum = await ports.sha256(csv), prior = await ports.priorEdition({ actor, input }), now = ports.now();
        return ports.commitPackage({ actor, key, input, id, source, prior, csv, checksum, now });
    }
    async function decideWithholdingPackage(actor: WorkContext, id: string, raw: z.input<typeof withholdingPackageDecisionSchema>, key: string) {
        actor = ports.authorizeTax(actor, raw.action === "cancel" ? "finance.write" : "finance.approve");
        const input = withholdingPackageDecisionSchema.parse(raw);
        await ports.ready(actor);
        const operation = `finance.withholding.month.${input.action}`, prior = await ports.replay(actor, key, operation, id, input);
        if (prior)
            return prior;
        const pack = await packageById(actor, id);
        if (pack.status !== "draft" || Number(pack.version) !== input.expectedVersion)
            throw ports.error(409, "WITHHOLDING_PACKAGE_CHANGED", "Reload the current pending monthly workpaper.");
        if (input.action === "cancel" ? !samePerson(actor, pack.prepared_by) : samePerson(actor, pack.prepared_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "Only the preparer cancels its draft; a different Finance reviewer approves the monthly sources.");
        const source = input.action === "approve" ? await freshSource(actor, pack) : null, status = input.action === "approve" ? "approved" : input.action === "cancel" ? "cancelled" : "rejected", now = ports.now();
        return ports.commitDecision({ actor, key, operation, input, id, status, pack, source, now });
    }
    async function remitWithholdingPackage(actor: WorkContext, id: string, raw: z.input<typeof withholdingRemittanceSchema>, key: string) {
        actor = ports.authorizeTax(actor, "finance.pay");
        const input = withholdingRemittanceSchema.parse(raw);
        await ports.ready(actor);
        const previous = await ports.replay(actor, key, "finance.withholding.month.remit", id, input);
        if (previous)
            return previous;
        const pack = await packageById(actor, id);
        if (pack.status !== "approved" || Number(pack.version) !== input.expectedVersion)
            throw ports.error(409, "WITHHOLDING_PACKAGE_CHANGED", "Reload the independently approved monthly workpaper.");
        if (samePerson(actor, pack.prepared_by) || samePerson(actor, pack.reviewed_by))
            throw ports.error(403, "INDEPENDENT_PAYER_REQUIRED", "A third Treasury person must match the tax remittance bank result.");
        const source = await freshSource(actor, pack), due = source.sources.filter(row => !row.alreadyRemitted), liabilities = new Map<string, number>();
        for (const row of due)
            liabilities.set(row.liabilityAccountId, (liabilities.get(row.liabilityAccountId) ?? 0) + row.taxMinor);
        const evidence = await ports.bankEvidence(actor, input, -source.taxMinor, source.chartId, due.reduce((date, row) => row.paymentDate > date ? row.paymentDate : date, `${pack.month}-01`), input.bankAccountId), remittanceId = `withholding-remit-${key}`, journal = await ports.prepareJournal(actor, { chartId: source.chartId, periodId: evidence.periodId, entryDate: String(evidence.row.transaction_date), currency: "THB", memo: `Manual ${pack.form_kind} monthly withholding remittance · ${pack.month}`, sourceType: "withholding_remittance", sourceId: remittanceId, isOpening: false, lines: [...Array.from(liabilities, ([accountId, total]) => ({ accountId, debit: minorToDecimal(total), credit: "0", description: "Ordinary supplier withholding liability remitted" })), { accountId: input.bankAccountId, debit: "0", credit: minorToDecimal(source.taxMinor), description: "Confirmed manual tax bank debit" }] }, `${key}-journal`, "posted");
        await ports.prepareMoney(actor, { sourceType: "finance_withholding_remittance", sourceId: remittanceId, amountKind: "paid", transactionDate: String(evidence.row.transaction_date), currency: "THB", currencyScale: 2, amountUnits: String(source.taxMinor) });
        const now = ports.now();
        return ports.commitRemittance({ actor, key, input, id, remittanceId, journal, source, pack, due, evidence, liabilities, now });
    }
    async function reverseWithholdingRemittance(actor: WorkContext, id: string, raw: z.input<typeof withholdingRemittanceReversalSchema>, key: string) {
        actor = ports.authorizeTax(actor, "finance.approve");
        const input = withholdingRemittanceReversalSchema.parse(raw);
        await ports.ready(actor);
        const previous = await ports.replay(actor, key, "finance.withholding.month.reverse", id, input);
        if (previous)
            return previous;
        const remittance = await ports.remittance({ id, actor });
        if (!remittance)
            throw ports.error(404, "WITHHOLDING_REMITTANCE_NOT_FOUND", "The Company tax remittance was not found.");
        if (remittance.status !== "confirmed" || Number(remittance.version) !== input.expectedVersion)
            throw ports.error(409, "WITHHOLDING_REMITTANCE_CHANGED", "Reload the confirmed remittance before recording actual recovery.");
        if (samePerson(actor, remittance.recorded_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "A different Finance reviewer records actual tax remittance recovery.");
        const evidence = await ports.bankEvidence(actor, input, Number(remittance.amount_minor), String(remittance.chart_id), String(remittance.entry_date), String(remittance.bank_account_id)), liabilities = JSON.parse(String(remittance.liability_accounts_json)) as Record<string, number>, journal = await ports.prepareJournal(actor, { chartId: String(remittance.chart_id), periodId: evidence.periodId, entryDate: String(evidence.row.transaction_date), currency: "THB", memo: `Confirmed manual tax remittance recovery · ${input.bankReference}`, sourceType: "withholding_remittance_reversal", sourceId: id, isOpening: false, lines: [{ accountId: String(remittance.bank_account_id), debit: minorToDecimal(Number(remittance.amount_minor)), credit: "0", description: "Confirmed tax recovery bank credit" }, ...Object.entries(liabilities).map(([accountId, amount]) => ({ accountId, debit: "0", credit: minorToDecimal(amount), description: "Restored original withholding liability" }))] }, `${key}-journal`, "posted");
        await ports.prepareMoneyReversal(actor, { sourceType: "finance_withholding_remittance_reversal", sourceId: id, amountKind: "returned", transactionDate: String(evidence.row.transaction_date), original: { sourceType: "finance_withholding_remittance", sourceId: id, amountKind: "paid" }, amountUnits: String(remittance.amount_minor) });
        const now = ports.now();
        return ports.commitRecovery({ actor, key, input, id, journal, remittance, evidence, now });
    }
    async function withholdingMonthlyWorkspace(actor: WorkContext) {
        actor = ports.authorizeTax(actor, "finance.read");
        await ports.ready(actor);
        return { packages: await ports.packageRows({ actor }), banks: await ports.bankRows({ actor }) };
    }
    async function withholdingPackageExport(actor: WorkContext, id: string) {
        actor = ports.authorizeTax(actor, "finance.read");
        await ports.ready(actor);
        const pack = await packageById(actor, id);
        if (await ports.sha256(pack.export_csv) !== pack.export_sha256)
            throw ports.error(409, "WITHHOLDING_EXPORT_CHECKSUM_INVALID", "The retained manual filing workpaper failed its source checksum.");
        return { content: pack.export_csv, sha256: pack.export_sha256, formKind: pack.form_kind, month: pack.month };
    }
    return { prepareWithholdingPackage, decideWithholdingPackage, remitWithholdingPackage, reverseWithholdingRemittance, withholdingMonthlyWorkspace, withholdingPackageExport };
}

