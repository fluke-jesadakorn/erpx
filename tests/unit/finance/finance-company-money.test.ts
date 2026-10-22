import { describe, expect, it, vi } from 'vitest';
import { createCompanyMoney, type CompanyMoneyPorts } from "@erpx/server-finance/application/company-money";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
const actor = { holdingId: 'h001', companyId: 'company', userId: 'user', assignmentId: 'assignment', role: 'finance_approver', branchId: null, departmentId: null } as WorkContext;
const original = { sourceType: 'booked_price', sourceId: 'price', amountKind: 'total' };
const source = { currency: 'THB', currency_scale: 2, amount_units: '42800', usd_units: '1189', fx_rate_id: 'original-rate', used_latest_fallback: false, original_source_type: null };
const input = { sourceType: 'charge', sourceId: 'split', amountKind: 'total', transactionDate: '2026-07-02', amountUnits: '21400', original };
function service(extra: Partial<CompanyMoneyPorts> = {}) { const ports: CompanyMoneyPorts = { error: (status, code, message) => Object.assign(new Error(message), { status, code }), authorize: actor => actor, ready: async () => { }, now: () => 123, id: () => 'new', rates: async () => [{ id: 'original-rate', currency: 'THB', effectiveDate: '2026-07-01', currencyPerUsd: '36', confirmedAt: 1 }], scales: async () => [], scale: async () => null, settings: async () => null, preference: async () => null, carrySource: async () => source, reversalSource: async () => source, commitPreference: vi.fn(async (facts) => ({ currency: facts.input.currency })), commitRate: vi.fn(async (facts) => ({ id: facts.id })), commitScale: vi.fn(async (facts) => ({ currency: facts.input.currency })), ...extra }; return { ports, run: createCompanyMoney(ports) }; }
describe('Company money semantic service', () => {
    it('preserves original rate and exactly distributes rounding over disjoint handoffs', async () => { const c = service(); const first = await c.run.snapshotMoneyCarry(actor, { ...input, allocationOffsetUnits: '0' }), last = await c.run.snapshotMoneyCarry(actor, { ...input, allocationOffsetUnits: '21400' }); expect(first.usdUnits).toBe('595'); expect(last.usdUnits).toBe('594'); expect(BigInt(first.usdUnits) + BigInt(last.usdUnits)).toBe(1189n); expect(last).toMatchObject({ rateId: 'original-rate', capturedAt: 123, allocationOffsetUnits: '21400' }); });
    it('rejects a reversal source as an immutable handoff source', async () => { const c = service({ carrySource: async () => ({ ...source, original_source_type: 'prior_source' }) }); await expect(c.run.snapshotMoneyCarry(actor, input)).rejects.toMatchObject({ code: 'MONEY_CARRY_SOURCE_INVALID' }); });
    it('rejects handoff offset extending past source before producing a persistence plan', async () => { await expect(service().run.snapshotMoneyCarry(actor, { ...input, allocationOffsetUnits: '21401' })).rejects.toMatchObject({ code: 'MONEY_CARRY_ALLOCATION_EXCEEDS_SOURCE' }); });
    it('uses the exact source currency and leaves cumulative reversal rounding to the native atomic allocator', async () => { const plan = await service().run.snapshotMoneyReversal(actor, { ...input, amountUnits: '42800' }); expect(plan).toEqual({ currency: 'THB', currencyScale: 2, rateId: 'original-rate', usedLatestFallback: false, amountUnits: '-42800', usdUnits: '0', capturedAt: 123 }); expect(plan).not.toHaveProperty('statement'); });
    it('rejects excessive reversals and a missing company source', async () => { await expect(service().run.snapshotMoneyReversal(actor, { ...input, amountUnits: '42801' })).rejects.toMatchObject({ code: 'INVALID_REVERSAL_AMOUNT' }); await expect(service({ reversalSource: async () => null }).run.snapshotMoneyReversal(actor, input)).rejects.toMatchObject({ code: 'MONEY_SOURCE_SNAPSHOT_NOT_FOUND' }); });
    it('rejects missing manual quote and mismatched variable scale', async () => { await expect(service({ rates: async () => [] }).run.snapshotMoney(actor, { ...input, currency: 'THB' })).rejects.toMatchObject({ code: 'MANUAL_RATE_REQUIRED' }); await expect(service({ scale: async () => ({ scale: 3 }) }).run.snapshotMoney(actor, { ...input, currency: 'XAU', currencyScale: 2 })).rejects.toMatchObject({ code: 'CURRENCY_SCALE_MISMATCH' }); });
    it('returns same-currency reference quote without asking for manual rate', async () => { const rates = vi.fn(async () => []); const quote = await service({ rates }).run.quoteMoneyConversion(actor, { amount: '10.01', currency: 'THB', targetCurrency: 'THB', transactionDate: '2026-07-01', requireRate: true }); expect(quote.converted).toMatchObject({ amount: '10.01', sourceRateVersion: null, targetRateVersion: null, referenceOnly: false }); expect(rates).not.toHaveBeenCalled(); });
    it('keeps company authority and positive manual rate checks ahead of native commit', async () => { const c = service(); await expect(c.run.confirmManualRate({ ...actor, branchId: 'branch' }, { currency: 'THB', effectiveDate: '2026-07-01', currencyPerUsd: '36', note: '' }, 'key')).rejects.toMatchObject({ code: 'COMPANY_FINANCE_REQUIRED' }); await expect(c.run.confirmManualRate(actor, { currency: 'THB', effectiveDate: '2026-07-01', currencyPerUsd: '0', note: '' }, 'key')).rejects.toMatchObject({ code: 'INVALID_FX_RATE' }); expect(c.ports.commitRate).not.toHaveBeenCalled(); });
    it('locks configured variable precision and rejects fixed ISO precision changes', async () => { const c = service({ scale: async () => ({ scale: 3 }) }); await expect(c.run.configureCurrencyScale(actor, { currency: 'XAU', scale: 2 }, 'key')).rejects.toMatchObject({ code: 'CURRENCY_SCALE_LOCKED' }); await expect(c.run.configureCurrencyScale(actor, { currency: 'THB', scale: 3 }, 'key')).rejects.toMatchObject({ code: 'FIXED_CURRENCY_SCALE' }); expect(c.ports.commitScale).not.toHaveBeenCalled(); });
    it('uses company/default preferences and injected permission facts', async () => { const c = service({ settings: async () => ({ default_currency: 'THB' }), preference: async () => ({ display_currency: 'USD' }), authorize: () => { throw new Error('denied'); } }); expect(await c.run.moneyContext(actor)).toMatchObject({ companyId: 'company', defaultCurrency: 'THB', displayCurrency: 'USD', canConfirmRate: false }); });
    it('shares fresh Company facts only inside each explicit batch and preserves per-date quotes', async () => {
        let denominator = '36';
        const rates = vi.fn(async (companyId: string) => [{ id: companyId + ':' + denominator, currency: 'THB', effectiveDate: '2026-07-01', currencyPerUsd: denominator, confirmedAt: 1 }]);
        const scales = vi.fn(async () => []);
        const c = service({ rates, scales });
        const inputs = [1, 2].map(n => ({ ...input, sourceId: 'night:' + n, currency: 'THB', amountUnits: '10700' }));
        const first = await c.run.snapshotMoneyBatch(actor, inputs);
        expect(first.map(p => p.usdUnits)).toEqual(['297', '297']);
        expect(rates).toHaveBeenCalledTimes(1); expect(scales).toHaveBeenCalledWith('company', ['THB']);
        denominator = '40';
        const next = await c.run.snapshotMoneyBatch({ companyId: 'other' }, inputs);
        expect(next.map(p => p.usdUnits)).toEqual(['268', '268']);
        expect(next.every(p => p.rateId === 'other:40')).toBe(true);
        expect(rates).toHaveBeenCalledTimes(2);
    });
    it('rejects invalid members before returning any batch plans and leaves an empty batch read-free', async () => {
        const scales = vi.fn(async () => [{ currency: 'XAU', scale: 3 }]);
        const rates = vi.fn(async () => []);
        const c = service({ scales, rates });
        expect(await c.run.snapshotMoneyBatch(actor, [])).toEqual([]); expect(scales).not.toHaveBeenCalled();
        await expect(c.run.snapshotMoneyBatch(actor, [{ ...input, currency: 'USD', amountUnits: '100' }, { ...input, currency: 'XAU', currencyScale: 2 }])).rejects.toMatchObject({ code: 'CURRENCY_SCALE_MISMATCH' });
        await expect(c.run.snapshotMoneyBatch(actor, [{ ...input, currency: 'THB', amountUnits: '1.5' }])).rejects.toThrow();
        expect(scales).toHaveBeenCalledTimes(1);
    });

});
