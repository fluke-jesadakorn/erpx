import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { createFinancePolicy } from '../domain/finance-policy';
import type { FinanceError } from '../domain/invoice-line-revision';
export type BankImportInput = {
    settlementPaymentId?: string;
    bankAccount: string;
    statementNumber: string;
    statementDate: string;
    opening: string;
    closing: string;
    closingFxRate?: string;
    sourceHash: string;
    rows: Array<{
        externalId: string;
        transactionDate: string;
        description?: string;
        amount: string;
        currency: string;
    }>;
};
export type BankReconciliationInput = {
    expectedVersion: number;
    matchedType: string;
    matchedId: string;
    proof: string;
    attachmentSetIds: string[];
};
export interface FinanceBankPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    digest(value: unknown): Promise<string>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    moneyScale(companyId: string, currency: string): Promise<number>;
    account(actor: WorkContext, id: string): Promise<{
        currency: string;
        control_kind: string;
        base_currency: string;
    } | null>;
    functionalFx(actor: WorkContext, currency: string, baseCurrency: string, date: string): Promise<number>;
    commitImport(facts: {
        actor: WorkContext;
        input: BankImportInput;
        key: string;
        id: string;
        opening: number;
        closing: number;
        amounts: number[];
        closingFxRate: number;
        baseClosing: number;
        digest: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitReconciliation(facts: {
        actor: WorkContext;
        transactionId: string;
        input: BankReconciliationInput;
        key: string;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createFinanceBank(ports: FinanceBankPorts) {
    const { decimalToMinor, fxMicros, convertAtFx, requireFinanceRole } = createFinancePolicy(ports.error);
    async function importBankStatement(actor: WorkContext, input: {
        settlementPaymentId?: string;
        bankAccount: string;
        statementNumber: string;
        statementDate: string;
        opening: string;
        closing: string;
        closingFxRate?: string;
        sourceHash: string;
        rows: Array<{
            externalId: string;
            transactionDate: string;
            description?: string;
            amount: string;
            currency: string;
        }>;
    }, key: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const id = `statement-${key}`;
        const previous = await ports.replay(actor, key, "finance.bank.import", id, input);
        if (previous)
            return previous;
        if (input.rows.length > 500)
            throw ports.error(422, "STATEMENT_SIZE", "Import at most 500 statement rows.");
        const account = await ports.account(actor, input.bankAccount);
        if (!account)
            throw ports.error(422, "BANK_ACCOUNT_INVALID", "Select an active cash or bank account from the primary book. Secondary books use the same business statement.");
        await ports.moneyScale(actor.companyId, account.currency);
        await ports.moneyScale(actor.companyId, account.base_currency);
        const opening = decimalToMinor(input.opening), closing = decimalToMinor(input.closing), amounts = input.rows.map(r => decimalToMinor(r.amount));
        if (BigInt(opening) + amounts.reduce((n, a) => n + BigInt(a), BigInt(0)) !== BigInt(closing))
            throw ports.error(422, "STATEMENT_UNBALANCED", "Opening balance plus movements must equal the closing balance.");
        if (new Set(input.rows.map(r => r.externalId)).size !== input.rows.length)
            throw ports.error(422, "DUPLICATE_TRANSACTION", "Each bank transaction needs a unique external reference.");
        if (input.rows.some(r => r.currency.toUpperCase() !== account.currency || r.transactionDate > input.statementDate))
            throw ports.error(422, "STATEMENT_ROW_INVALID", "Rows must use the bank account currency and cannot follow the statement date.");
        const closingFxRate = input.closingFxRate ? fxMicros(account.currency, account.base_currency, input.closingFxRate) : await ports.functionalFx(actor, account.currency, account.base_currency, input.statementDate), baseClosing = convertAtFx(closing, closingFxRate), digest = await ports.digest({ bankAccount: input.bankAccount, statementNumber: input.statementNumber, opening, closing, closingFxRate, rows: input.rows });
        return ports.commitImport({ actor, input, key, id, opening, closing, amounts, closingFxRate, baseClosing, digest, now: ports.now() });
    }
    async function reconcileBankTransaction(actor: WorkContext, transactionId: string, input: {
        expectedVersion: number;
        matchedType: string;
        matchedId: string;
        proof: string;
        attachmentSetIds: string[];
    }, key: string) {
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        await ports.ready(actor);
        if (input.matchedType !== "payment" || !input.proof.trim())
            throw ports.error(422, "MATCH_INPUT_INVALID", "Select a posted payment and provide review evidence.");
        const previous = await ports.replay(actor, key, "finance.bank.reconcile", transactionId, input);
        if (previous)
            return previous;
        return ports.commitReconciliation({ actor, transactionId, input, key, now: ports.now() });
    }
    return { importBankStatement, reconcileBankTransaction };
}

