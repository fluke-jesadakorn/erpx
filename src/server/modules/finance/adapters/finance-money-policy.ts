import "server-only";
import { createRequireFinanceMoneyScale } from "../application/money-policy";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
/** Existing Finance ledger amounts retain their original two-decimal meaning. */
export async function requireFinanceMoneyScale(db: SqlDatabase, companyId: string, currency: string) {
    return createRequireFinanceMoneyScale({ error: (...args) => new ApiError(...args), scale: async (companyId, currency) => (await db.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?").bind(companyId, currency).first<{
            scale: number;
        }>())?.scale ?? null })(companyId, currency);
}
