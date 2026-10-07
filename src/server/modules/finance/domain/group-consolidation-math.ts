import {halfAway} from './finance-instrument-policy';import {BusinessRuleError} from '../../shared';
export function translationRate(currency: string, presentationCurrency: string, rates: Map<string, number>) {
        if (currency === presentationCurrency)
            return 1000000;
        const rate = rates.get(currency);
        if (rate === undefined)
            throw new BusinessRuleError(422, "CONSOLIDATION_RATE_REQUIRED", `Provide a presentation-currency rate for ${currency} before preparing this snapshot.`);
        if (rate <= 0)
            throw new BusinessRuleError(422, "CONSOLIDATION_RATE_INVALID", `The presentation-currency rate for ${currency} must be greater than zero.`);
        return rate;
    }
export function translateMinor(amount: number, rateMicros: number) {
        const product = BigInt(amount) * BigInt(rateMicros), negative = product < BigInt(0), absolute = negative ? -product : product;
        const converted = (absolute + BigInt(500000)) / BigInt(1000000), signed = negative ? -converted : converted;
        if (signed > BigInt(Number.MAX_SAFE_INTEGER) || signed < BigInt(Number.MIN_SAFE_INTEGER))
            throw new BusinessRuleError(422, "MONEY_OVERFLOW", "The translated consolidation balance exceeds supported precision.");
        return Number(signed);
    }
export function convert(amount: bigint, fx: bigint) {
        return halfAway(amount * fx, BigInt(1000000));
    }
