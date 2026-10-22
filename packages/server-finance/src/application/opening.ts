import { z } from 'zod';
import { openingTrialBalanceSchema, createJournalSchema, type FinanceRow, type JournalInput } from '@erpx/server-finance/domain/finance';
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import type { JournalPlan } from '@erpx/server-finance/application/journal';
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export type OpeningInput = z.infer<typeof openingTrialBalanceSchema>;
export type JournalResult = {
    id: string;
    number: string;
    status: string;
    version: number;
    debit: string;
    credit: string;
};
export type OpeningResult = JournalResult & {
    subledgerCount: number;
};
export interface OpeningPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext, readOnly?: boolean): Promise<void>;
    now(): number;
    digest(value: unknown): Promise<string>;
    period(actor: WorkContext, periodId: string, date: string): Promise<FinanceRow>;
    prepare(actor: WorkContext, input: JournalInput, key: string): Promise<JournalPlan>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<JournalResult | null>;
    replayOpening(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<OpeningResult | null>;
    commitJournal(facts: {
        actor: WorkContext;
        input: JournalInput;
        id: string;
        journalId: string;
        plan: JournalPlan;
    }): Promise<JournalResult>;
    commitOpening(facts: {
        actor: WorkContext;
        commandId: string;
        journalId: string;
        commandInput: {
            journal: JournalInput;
            subledgerItems: OpeningInput['subledgerItems'];
        };
        journalInput: JournalInput;
        definition: OpeningInput;
        journal: JournalPlan;
        now: number;
    }): Promise<OpeningResult>;
    openingChart(facts: {
        input: OpeningInput;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    openingAccounts(facts: {
        accountIds: string[];
        actor: WorkContext;
        input: OpeningInput;
    }): Promise<FinanceRow[]>;
    openingParties(facts: {
        partyIds: string[];
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    openingReference(facts: {
        references: Set<string>;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    openingExisting(facts: {
        actor: WorkContext;
        input: OpeningInput;
    }): Promise<boolean>;
}
export function createOpeningService(ports: OpeningPorts) {
    const { decimalToMinor } = createFinancePolicy(ports.error);
    async function createJournal(actor: WorkContext, input: z.infer<typeof createJournalSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        if (input.sourceType === "fiscal_close")
            throw ports.error(422, "FISCAL_CLOSING_OWNER_REQUIRED", "Prepare and independently review a fiscal closing through the Finance year-closing workflow.");
        if (input.sourceType.startsWith("book_"))
            throw ports.error(422, "BOOK_OWNER_REQUIRED", "Book openings, projections and adjustments require their owning review workflow.");
        if (["business_recognition", "business_billing_credit", "instrument_activation", "instrument_payment", "asset_policy_depreciation"].includes(input.sourceType))
            throw ports.error(422, "FINANCE_SOURCE_OWNER_REQUIRED", "Prepare this journal through its originating Finance workflow.");
        await ports.ready(actor);
        const journalId = `journal-${id}`;
        const previous = await ports.replay(actor, id, "finance.journal.create", journalId, input);
        if (previous)
            return previous;
        const plan = await ports.prepare(actor, input, id);
        return ports.commitJournal({ actor, input, id, journalId, plan });
    }
    async function previewOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema>) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor, true);
        const chart = await ports.openingChart({ input, actor });
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Active chart not found.");
        if (input.currency !== chart.base_currency)
            throw ports.error(422, "OPENING_CURRENCY_MISMATCH", "Opening balances must use the chart's functional currency.");
        const period = await ports.period(actor, input.periodId, input.entryDate);
        const lineAccountIds = new Set(input.lines.map(line => line.accountId));
        if (lineAccountIds.size !== input.lines.length)
            throw ports.error(422, "DUPLICATE_OPENING_ACCOUNT", "Each account may appear once in an opening balance import.");
        const accountIds = [...new Set([...lineAccountIds, ...input.subledgerItems.map(item => item.offsetAccountId)])];
        if (input.subledgerItems.some(item => !lineAccountIds.has(item.controlAccountId)))
            throw ports.error(422, "OPENING_CONTROL_LINE_REQUIRED", "Every subledger item must identify its matching opening-balance control account.");
        const accounts = await ports.openingAccounts({ accountIds, actor, input });
        if (accounts.length !== accountIds.length || accounts.some(account => account.status !== "active"))
            throw ports.error(422, "ACCOUNT_INVALID", "Every opening balance account must be active and belong to this chart.");
        let debit = BigInt(0), credit = BigInt(0);
        const accountById = new Map(accounts.map(account => [String(account.id), account]));
        const lines = input.lines.map(line => {
            const debitMinor = decimalToMinor(line.debit), creditMinor = decimalToMinor(line.credit);
            if (debitMinor < 0 || creditMinor < 0 || (debitMinor > 0) === (creditMinor > 0))
                throw ports.error(422, "INVALID_OPENING_LINE", "Each opening line requires one positive debit or credit.");
            debit += BigInt(debitMinor);
            credit += BigInt(creditMinor);
            const account = accountById.get(line.accountId)!;
            if (["receivable", "payable"].includes(String(account.control_kind)) && line.subledgerKind !== account.control_kind)
                throw ports.error(422, "OPENING_CONTROL_SCHEDULE_REQUIRED", `${account.code} ${account.name} requires a ${account.control_kind} schedule.`);
            if (line.subledgerKind !== "none") {
                if (Number(account.is_control) !== 1)
                    throw ports.error(422, "OPENING_CONTROL_ACCOUNT_REQUIRED", "Receivable and payable opening lines must use accounts marked as control accounts.");
                if (line.subledgerKind === "receivable" && debitMinor <= 0)
                    throw ports.error(422, "OPENING_RECEIVABLE_DIRECTION", "Opening receivables must be a debit balance.");
                if (line.subledgerKind === "payable" && creditMinor <= 0)
                    throw ports.error(422, "OPENING_PAYABLE_DIRECTION", "Opening payables must be a credit balance.");
            }
            return { ...line, debit: minorToDecimal(debitMinor), credit: minorToDecimal(creditMinor), accountCode: String(account.code), accountName: String(account.name) };
        });
        if (debit !== credit)
            throw ports.error(422, "UNBALANCED_OPENING_BALANCE", "Opening debits and credits must balance.");
        if (debit > BigInt(Number.MAX_SAFE_INTEGER))
            throw ports.error(422, "MONEY_OVERFLOW", "The opening balance total exceeds supported precision.");
        const references = new Set<string>();
        const partyIds = [...new Set(input.subledgerItems.map(item => item.partyId))];
        const parties = partyIds.length ? await ports.openingParties({ partyIds, actor }) : [] as FinanceRow[];
        const partyById = new Map(parties.map(party => [String(party.id), party]));
        const scheduleByAccount = new Map<string, bigint>();
        const subledgerItems = input.subledgerItems.map(item => {
            const amountMinor = decimalToMinor(item.amount);
            if (amountMinor <= 0)
                throw ports.error(422, "INVALID_OPENING_SUBLEDGER_AMOUNT", "Each opening subledger amount must be greater than zero.");
            const party = partyById.get(item.partyId);
            if (!party || party.status !== "active" || party.kind !== (item.kind === "receivable" ? "customer" : "supplier"))
                throw ports.error(422, "OPENING_PARTY_INVALID", `Opening ${item.kind} ${item.reference} requires an active ${item.kind === "receivable" ? "customer" : "supplier"}.`);
            if (party.currency !== input.currency)
                throw ports.error(422, "OPENING_PARTY_CURRENCY_MISMATCH", `Opening ${item.reference} must use a party with currency ${input.currency}.`);
            const controlLine = input.lines.find(line => line.accountId === item.controlAccountId);
            if (!controlLine || controlLine.subledgerKind !== item.kind)
                throw ports.error(422, "OPENING_CONTROL_KIND_MISMATCH", `Opening ${item.reference} does not match the selected control-account type.`);
            const offset = accountById.get(item.offsetAccountId);
            if (!offset || offset.status !== "active" || item.offsetAccountId === item.controlAccountId)
                throw ports.error(422, "OPENING_OFFSET_ACCOUNT_INVALID", `Opening ${item.reference} requires a different active offset account in this chart.`);
            const referenceKey = `${item.kind}:${item.reference.toLocaleLowerCase("en-US")}`;
            if (references.has(referenceKey))
                throw ports.error(422, "DUPLICATE_OPENING_REFERENCE", "Opening subledger references must be unique for each kind.");
            references.add(referenceKey);
            scheduleByAccount.set(item.controlAccountId, (scheduleByAccount.get(item.controlAccountId) ?? BigInt(0)) + BigInt(amountMinor));
            return { ...item, amount: minorToDecimal(amountMinor), partyKind: String(party.kind) };
        });
        for (const line of lines.filter(line => line.subledgerKind !== "none")) {
            const expected = BigInt(decimalToMinor(line.subledgerKind === "receivable" ? line.debit : line.credit));
            const scheduled = scheduleByAccount.get(line.accountId) ?? BigInt(0);
            if (expected !== scheduled)
                throw ports.error(422, "OPENING_SUBLEDGER_MISMATCH", `${line.accountCode} ${line.accountName} is ${minorToDecimal(Number(expected))}, but its ${line.subledgerKind} schedule totals ${minorToDecimal(Number(scheduled))}.`);
        }
        if (references.size) {
            const existing = await ports.openingReference({ references, actor });
            if (existing)
                throw ports.error(409, "OPENING_REFERENCE_EXISTS", `Invoice ${existing.invoice_number} already exists for ${existing.kind}.`);
        }
        const existing = await ports.openingExisting({ actor, input });
        if (existing)
            throw ports.error(409, "OPENING_BALANCE_EXISTS", "Reverse or finish the existing opening balance before importing another one.");
        const previewToken = await ports.digest({
            companyId: actor.companyId,
            input: {
                ...input,
                lines: input.lines.map(line => ({
                    ...line,
                    debit: minorToDecimal(decimalToMinor(line.debit)),
                    credit: minorToDecimal(decimalToMinor(line.credit)),
                })),
            },
            chart: { id: chart.id, currency: chart.base_currency, updatedAt: chart.updated_at },
            period: { id: period.id, state: period.state, version: period.close_version },
            accounts: accounts
                .map(account => ({ id: account.id, status: account.status, isControl: account.is_control, controlKind: account.control_kind, updatedAt: account.updated_at }))
                .sort((a, b) => String(a.id).localeCompare(String(b.id))),
            parties: parties.map(party => ({ id: party.id, kind: party.kind, currency: party.currency, status: party.status, version: party.version, updatedAt: party.updated_at })).sort((a, b) => String(a.id).localeCompare(String(b.id))),
        });
        return { previewToken, chartId: input.chartId, periodId: input.periodId, entryDate: input.entryDate, currency: input.currency, debit: minorToDecimal(Number(debit)), credit: minorToDecimal(Number(credit)), lines, subledgerItems, subledgerCount: subledgerItems.length };
    }
    async function importOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema> & {
        previewToken: string;
    }, id: string) {
        const { previewToken, ...definition } = input;
        actor = ports.authorize(actor, "finance.write");
        const journalInput: z.infer<typeof createJournalSchema> = { chartId: definition.chartId, periodId: definition.periodId, entryDate: definition.entryDate, currency: definition.currency, memo: "Verified opening trial balance and subledger schedules", sourceType: "opening_import", sourceId: definition.chartId, isOpening: true, lines: definition.lines.map((line) => ({ accountId: line.accountId, debit: line.debit, credit: line.credit, description: line.description })) };
        // Keep the record identifier path-safe and consistent with other journal
        // commands. The idempotency key already scopes this operation, so adding a
        // second suffix here only creates an unnecessary colon in the URL segment.
        await ports.ready(actor);
        const commandId = id, journalId = `journal-${commandId}`;
        const commandInput = { journal: journalInput, subledgerItems: definition.subledgerItems };
        const replay = await ports.replayOpening(actor, commandId, "finance.journal.create", journalId, commandInput);
        if (replay)
            return replay;
        const preview = await previewOpeningTrialBalance(actor, definition);
        if (preview.previewToken !== previewToken)
            throw ports.error(409, "OPENING_PREVIEW_STALE", "The chart, period, accounts or opening balances changed. Preview the import again.");
        const journal = await ports.prepare(actor, journalInput, commandId), now = ports.now();
        return ports.commitOpening({ actor, commandId, journalId, commandInput, journalInput, definition, journal, now });
    }
    return { createJournal, previewOpeningTrialBalance, importOpeningTrialBalance };
}
