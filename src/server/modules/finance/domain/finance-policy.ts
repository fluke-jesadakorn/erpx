import { decimalToMinor as parseMinor } from "@/lib/finance/finance-money";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from './invoice-line-revision';
export function createFinancePolicy(error: FinanceError) {
    function decimalToMinor(value: string, scale = 2): number {
        try {
            return parseMinor(value, scale);
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : "Invalid money.";
            throw error(422, message.includes("decimal") ? "INVALID_MONEY" : message.includes("places") ? "MONEY_SCALE" : "MONEY_OVERFLOW", message);
        }
    }
    function fxMicros(currency: string, baseCurrency: string, value?: string) {
        const rate = currency === baseCurrency ? 1000000 : value ? decimalToMinor(value, 6) : 0;
        if (rate < 1 || rate > 1000000000000)
            throw error(422, "FX_RATE_REQUIRED", `Provide a positive ${baseCurrency} per ${currency} exchange rate.`);
        return rate;
    }
    function convertAtFx(amount: number, rate: number) {
        const product = BigInt(amount) * BigInt(rate), negative = product < BigInt(0), absolute = negative ? -product : product, converted = (absolute + BigInt(500000)) / BigInt(1000000), signed = negative ? -converted : converted;
        if (signed > BigInt(Number.MAX_SAFE_INTEGER) || signed < BigInt(Number.MIN_SAFE_INTEGER))
            throw error(422, "MONEY_OVERFLOW", "The converted amount exceeds supported precision.");
        return Number(signed);
    }
    function proportionalBase(baseOutstanding: number, transactionOutstanding: number, allocated: number) {
        if (allocated === transactionOutstanding)
            return baseOutstanding;
        return Number((BigInt(baseOutstanding) * BigInt(allocated) + BigInt(Math.floor(transactionOutstanding / 2))) / BigInt(transactionOutstanding));
    }
    function requireFinanceRole(actor: WorkContext, approver = false) {
        if (!hasPermission(actor, "finance.read"))
            throw error(403, "FINANCE_FORBIDDEN", "A finance assignment is required.");
        if (approver && !hasPermission(actor, "finance.approve"))
            throw error(403, "FINANCE_APPROVAL_REQUIRED", "An independent finance approver is required.");
    }
    return { decimalToMinor, fxMicros, convertAtFx, proportionalBase, requireFinanceRole };
}
