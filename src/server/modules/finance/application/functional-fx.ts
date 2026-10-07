import { resolveRate, decimalUnits, type ManualRate } from '@/lib/company-money';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export function createFunctionalFx(ports: {
    rates(companyId: string): Promise<ManualRate[]>;
    error: FinanceError;
}) {
    async function confirmedFunctionalFx(actor: WorkContext, currency: string, baseCurrency: string, date: string) {
        if (currency === baseCurrency)
            return { micros: 1000000, sourceRateId: null as string | null, targetRateId: null as string | null, usedLatestFallback: false };
        const rates = await ports.rates(actor.companyId), source = resolveRate(rates, currency, date), target = resolveRate(rates, baseCurrency, date);
        if (!source || !target)
            throw ports.error(422, "MANUAL_RATE_REQUIRED", `Confirm company manual USD rates for ${currency} and ${baseCurrency} before posting.`);
        const sourceQuote = decimalUnits(source.rate.currencyPerUsd, 18), targetQuote = decimalUnits(target.rate.currencyPerUsd, 18);
        const micros = (targetQuote * BigInt(1000000) + sourceQuote / BigInt(2)) / sourceQuote;
        if (micros < BigInt(1) || micros > BigInt(1000000000000))
            throw ports.error(422, "FX_RATE_PRECISION", "The functional-currency rate is outside this ledger's supported six-decimal precision.");
        return { micros: Number(micros), sourceRateId: source.rate.id === 'USD' ? null : source.rate.id, targetRateId: target.rate.id === 'USD' ? null : target.rate.id, usedLatestFallback: source.usedLatestFallback || target.usedLatestFallback };
    }
    return confirmedFunctionalFx;
}
