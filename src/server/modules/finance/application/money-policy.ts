import { currencyScale } from "@/lib/finance/company-money";
import type { FinanceError } from "../domain/invoice-line-revision";
export function createRequireFinanceMoneyScale(ports: {
    scale(companyId: string, currency: string): Promise<number | null>;
    error: FinanceError;
}) {
    return async (companyId: string, currency: string) => {
        const configured = await ports.scale(companyId, currency);
        let scale: number;
        try {
            scale = currencyScale(currency, configured === null ? undefined : Number(configured));
        }
        catch (e) {
            throw ports.error(422, "CURRENCY_SCALE_REQUIRED", e instanceof Error ? e.message : "Configure the currency's minor units.");
        }
        if (scale !== 2)
            throw ports.error(422, "FINANCE_CURRENCY_SCALE_UNSUPPORTED", "This Finance ledger currently accepts currencies with two minor-unit decimals. Historical amounts retain their recorded scale; use a supported currency for a new document.");
        return scale;
    };
}
