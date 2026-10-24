import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{ManualRate}from '@erpx/feature-finance/lib/company-money';
type RateRow = {
    id: string;
    currency: string;
    effective_date: string;
    currency_per_usd: string;
    confirmed_at: number;
};
function rate(row: RateRow): ManualRate {
    const quote = String(row.currency_per_usd);
    return { id: row.id, currency: row.currency, effectiveDate: String(row.effective_date).slice(0, 10),
        currencyPerUsd: quote.includes(".") ? quote.replace(/0+$/, "").replace(/\.$/, "") : quote, confirmedAt: Number(row.confirmed_at) };
}
export async function companyRates(db: SqlDatabase, companyId: string): Promise<ManualRate[]> {
    return (await db.prepare("SELECT id,currency,effective_date,currency_per_usd::text AS currency_per_usd,confirmed_at FROM company_fx_rates WHERE company_id=? ORDER BY effective_date DESC,confirmed_at DESC,id DESC")
        .bind(companyId).all<RateRow>()).results.map(rate);
}
