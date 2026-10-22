import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export type PosRow = Record<string, string | number | boolean | null>;
export const num = (row: PosRow, key: string) => Number(row[key] ?? 0);
export const str = (row: PosRow, key: string) => String(row[key] ?? '');
export function json<T>(value: unknown): T { return JSON.parse(String(value)) as T; }
export const pair = (debit: string, credit: string, amount: number) => [{ account: debit, debit: amount, credit: 0 }, { account: credit, debit: 0, credit: amount }];
export function scopeRow(actor: WorkContext, row: PosRow | null, error: (status: number, code: string, message: string) => Error): PosRow {
    if (!row || row.company_id !== actor.companyId || (actor.branchId && row.branch_id !== actor.branchId) || actor.departmentId)
        throw error(404, 'POS_NOT_FOUND', 'Record not found in this workspace');
    return row;
}
