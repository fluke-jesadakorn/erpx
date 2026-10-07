import { z } from 'zod';
import { createAccountSchema, type FinanceRow } from '../domain/finance';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export type AccountImportRow = {
    code: string;
    name: string;
    category: z.infer<typeof createAccountSchema>['category'];
    currency?: string;
    controlKind?: 'none' | 'receivable' | 'payable' | 'bank';
};
export type AccountImportPlan = {
    id: string;
    index: number;
    row: AccountImportRow;
    existing: FinanceRow | null;
    currency: string;
    controlKind: string;
    structuralChange: boolean;
};
export interface AccountImportPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    id(): string;
    prior<Result=Record<string,unknown>>(actor: WorkContext, key: string, input: unknown): Promise<Result|null>;
    chart(actor: WorkContext, chartId: string): Promise<{
        base_currency: string;
    } | null>;
    existing(actor: WorkContext, chartId: string, code: string): Promise<FinanceRow | null>;
    unused(id: string | number | null): Promise<boolean>;
    commit(facts: {
        actor: WorkContext;
        key: string;
        input: {
            chartId: string;
            rows: AccountImportRow[];
        };
        plans: AccountImportPlan[];
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createAccountImport(ports: AccountImportPorts) { return async (actor: WorkContext, chartId: string, rows: AccountImportRow[], key: string) => { actor = ports.authorize(actor, 'finance.write'); await ports.ready(actor); const input = { chartId, rows }, previous = await ports.prior<Record<string,unknown>>(actor, key, input); if (previous)
    return previous; const chart = await ports.chart(actor, chartId); if (!chart)
    throw ports.error(404, 'CHART_NOT_FOUND', 'Chart not found.'); if (!rows.length || rows.length > 1000)
    throw ports.error(422, 'IMPORT_SIZE', 'Import between 1 and 1,000 accounts.'); if (new Set(rows.map(row => row.code.trim())).size !== rows.length)
    throw ports.error(422, 'DUPLICATE_ACCOUNT_CODE', 'Each account code must appear once.'); const now = ports.now(), plans: AccountImportPlan[] = []; for (const [index, row] of rows.entries()) {
    const existing = await ports.existing(actor, chartId, row.code), currency = (row.currency ?? String(existing?.currency ?? chart.base_currency)).toUpperCase(), controlKind = row.controlKind ?? String(existing?.control_kind ?? 'none'), structuralChange = !!existing && (existing.name !== row.name || existing.category !== row.category || existing.currency !== currency || existing.control_kind !== controlKind);
    if (structuralChange && !await ports.unused(existing!.id))
        throw ports.error(409, 'ACCOUNT_HISTORY_IMMUTABLE', `Account ${row.code} is already referenced. Create a new account to change its accounting meaning.`);
    plans.push({ id: `acct-${ports.id()}`, index, row, existing, currency, controlKind, structuralChange });
} return ports.commit({ actor, key, input, plans, now }); }; }

