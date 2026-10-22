import { z } from 'zod';
import { currencyDefinition, currencyDefinitions, currencyScale, convertWithRates, decimalUnits, unitsDecimal, type ManualRate } from "@erpx/feature-finance/lib/company-money";
import { preferenceSchema, manualRateSchema, currencyScaleSchema, conversionSchema, moneyIdentitySchema, type SnapshotInput, type CarryInput, type ReversalInput, type SnapshotSource, type CarrySource, type SnapshotPlan, type CarryPlan } from '@erpx/server-finance/domain/company-money';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export type MoneyActor = Pick<WorkContext, 'companyId'>;
export type PreferenceInput = z.infer<typeof preferenceSchema>;
export type RateInput = z.infer<typeof manualRateSchema>;
export type ScaleInput = z.infer<typeof currencyScaleSchema>;
export interface CompanyMoneyPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    id(): string;
    rates(companyId: string): Promise<ManualRate[]>;
    scales(companyId: string, currencies?: string[]): Promise<Array<{
        currency: string;
        scale: number;
    }>>;
    scale(companyId: string, currency: string): Promise<{
        scale: number;
    } | null>;
    settings(companyId: string): Promise<{
        default_currency: string;
    } | null>;
    preference(actor: WorkContext): Promise<{
        display_currency: string;
    } | null>;
    carrySource(actor: MoneyActor, original: CarryInput['original']): Promise<CarrySource | null>;
    reversalSource(actor: MoneyActor, original: ReversalInput['original']): Promise<SnapshotSource | null>;
    commitPreference(facts: {
        actor: WorkContext;
        input: PreferenceInput;
        key: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitRate(facts: {
        actor: WorkContext;
        input: RateInput;
        key: string;
        id: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitScale(facts: {
        actor: WorkContext;
        input: ScaleInput;
        key: string;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createCompanyMoney(ports: CompanyMoneyPorts) {
    async function quoteMoneyConversion(actor: WorkContext, input: z.infer<typeof conversionSchema>) {
        await ports.ready(actor);
        const specialCodes = [input.currency, input.targetCurrency].filter((value) => currencyDefinition(value)?.scale === null);
        const scaleRows = specialCodes.length ? await ports.scales(actor.companyId, specialCodes) : [];
        const scales = Object.fromEntries(scaleRows.map((row) => [row.currency, Number(row.scale)]));
        let sourceScale: number, targetScale: number;
        try {
            sourceScale = currencyScale(input.currency, scales[input.currency]);
            targetScale = currencyScale(input.targetCurrency, scales[input.targetCurrency]);
        }
        catch (cause) {
            throw ports.error(422, "CURRENCY_SCALE_REQUIRED", cause instanceof Error ? cause.message : "Configure the currency scale.");
        }
        let amountUnits: bigint;
        try {
            amountUnits = decimalUnits(input.amount, sourceScale);
        }
        catch (cause) {
            throw ports.error(422, "INVALID_MONEY_SCALE", cause instanceof Error ? cause.message : "Invalid money amount.");
        }
        if (input.currency === input.targetCurrency)
            return {
                original: { amount: unitsDecimal(amountUnits, sourceScale), currency: input.currency, scale: sourceScale },
                converted: { amount: unitsDecimal(amountUnits, targetScale), currency: input.targetCurrency, scale: targetScale,
                    sourceRateVersion: null, targetRateVersion: null, usedLatestFallback: false, referenceOnly: !input.requireRate },
            };
        const converted = convertWithRates({ amountUnits, sourceCurrency: input.currency, sourceScale,
            targetCurrency: input.targetCurrency, targetScale, date: input.transactionDate, rates: await ports.rates(actor.companyId) });
        if (!converted && input.requireRate)
            throw ports.error(422, "MANUAL_RATE_REQUIRED", "A confirmed manual rate is required before this transaction can be recorded.");
        return {
            original: { amount: unitsDecimal(amountUnits, sourceScale), currency: input.currency, scale: sourceScale },
            converted: converted ? { amount: unitsDecimal(converted.amountUnits, targetScale), currency: input.targetCurrency, scale: targetScale,
                sourceRateVersion: converted.sourceRate.id, targetRateVersion: converted.targetRate.id,
                usedLatestFallback: converted.usedLatestFallback, referenceOnly: !input.requireRate } : null,
        };
    }
    async function moneyContext(actor: WorkContext) {
        await ports.ready(actor);
        const [settings, preference, scales, rates] = await Promise.all([
            ports.settings(actor.companyId),
            ports.preference(actor),
            ports.scales(actor.companyId),
            ports.rates(actor.companyId),
        ]);
        return {
            companyId: actor.companyId,
            defaultCurrency: settings?.default_currency ?? "USD",
            displayCurrency: preference?.display_currency ?? settings?.default_currency ?? "USD",
            scales: Object.fromEntries(scales.map((item) => [item.currency, Number(item.scale)])),
            rates,
            currencies: currencyDefinitions,
            canConfirmRate: !actor.branchId && !actor.departmentId && requirePermissionOrFalse(actor, "finance.approve"),
        };
    }
    function requirePermissionOrFalse(actor: WorkContext, permission: string) {
        try {
            ports.authorize(actor, permission);
            return true;
        }
        catch {
            return false;
        }
    }
    async function saveCurrencyPreference(actor: WorkContext, input: z.infer<typeof preferenceSchema>, key: string) {
        await ports.ready(actor);
        const now = ports.now();
        return ports.commitPreference({ actor, input, key, now });
    }
    async function confirmManualRate(actor: WorkContext, input: z.infer<typeof manualRateSchema>, key: string) {
        ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "A company-level Finance approver must confirm rates shared across the company.");
        if (decimalUnits(input.currencyPerUsd, 18) <= BigInt(0))
            throw ports.error(422, "INVALID_FX_RATE", "The manual rate must be positive.");
        await ports.ready(actor);
        const id = `fx-${ports.id()}`, now = ports.now();
        return ports.commitRate({ actor, input, key, id, now });
    }
    async function configureCurrencyScale(actor: WorkContext, input: z.infer<typeof currencyScaleSchema>, key: string) {
        ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "A company-level Finance approver must configure currency precision.");
        const definition = currencyDefinition(input.currency)!;
        if (definition.scale !== null)
            throw ports.error(422, "FIXED_CURRENCY_SCALE", "This ISO currency already has a fixed minor unit.");
        await ports.ready(actor);
        const now = ports.now();
        const existing = await ports.scale(actor.companyId, input.currency);
        if (existing && Number(existing.scale) !== input.scale)
            throw ports.error(409, "CURRENCY_SCALE_LOCKED", "Currency precision cannot change after setup.");
        return ports.commitScale({ actor, input, key, now });
    }
    function planSnapshot(input: SnapshotInput, customScale: number | undefined, rates: ManualRate[]): SnapshotPlan {
        moneyIdentitySchema.extend({ currency: conversionSchema.shape.currency }).parse({ ...input, amountUnits: String(input.amountUnits) });
        const configuredScale = currencyScale(input.currency, customScale);
        if (input.currencyScale !== undefined && input.currencyScale !== configuredScale)
            throw ports.error(422, "CURRENCY_SCALE_MISMATCH", `A ${input.currency} transaction must use ${configuredScale} decimal places.`);
        const converted = convertWithRates({ amountUnits: input.amountUnits, sourceCurrency: input.currency, sourceScale: configuredScale,
            targetCurrency: "USD", targetScale: 2, date: input.transactionDate, rates });
        if (!converted)
            throw ports.error(422, "MANUAL_RATE_REQUIRED", `A confirmed manual ${input.currency}/USD rate is required.`);
        return { currency: input.currency, amountUnits: String(input.amountUnits), usdUnits: String(converted.amountUnits), currencyScale: configuredScale, rateId: converted.sourceRate.id === 'USD' ? null : converted.sourceRate.id, usedLatestFallback: converted.usedLatestFallback, capturedAt: ports.now() };
    }
    async function snapshotMoney(actor: MoneyActor, input: SnapshotInput): Promise<SnapshotPlan> {
        moneyIdentitySchema.extend({ currency: conversionSchema.shape.currency }).parse({ ...input, amountUnits: String(input.amountUnits) });
        const customScale = await ports.scale(actor.companyId, input.currency);
        // Preserve validation before the rate read for existing single-snapshot callers.
        const configuredScale = currencyScale(input.currency, customScale?.scale);
        if (input.currencyScale !== undefined && input.currencyScale !== configuredScale)
            throw ports.error(422, "CURRENCY_SCALE_MISMATCH", `A ${input.currency} transaction must use ${configuredScale} decimal places.`);
        return planSnapshot(input, customScale?.scale, await ports.rates(actor.companyId));
    }
    /** Fresh Company facts are shared only by this explicit batch, never another invocation. */
    async function snapshotMoneyBatch(actor: MoneyActor, inputs: readonly SnapshotInput[]): Promise<SnapshotPlan[]> {
        if (!inputs.length) return [];
        for (const input of inputs)
            moneyIdentitySchema.extend({ currency: conversionSchema.shape.currency }).parse({ ...input, amountUnits: String(input.amountUnits) });
        const scales = await ports.scales(actor.companyId, [...new Set(inputs.map(input => input.currency))]);
        const rates = await ports.rates(actor.companyId);
        return inputs.map(input => planSnapshot(input, scales.find(row => row.currency === input.currency)?.scale, rates));
    }
    async function snapshotMoneyCarry(actor: MoneyActor, input: CarryInput): Promise<CarryPlan> {
        moneyIdentitySchema.parse({ ...input, amountUnits: String(input.amountUnits) });
        moneyIdentitySchema.pick({ sourceType: true, sourceId: true, amountKind: true }).parse(input.original);
        const source = await ports.carrySource(actor, input.original);
        const amount = BigInt(input.amountUnits);
        if (!source || BigInt(source.amount_units) <= BigInt(0) || source.original_source_type !== null || amount < BigInt(0) || amount > BigInt(source.amount_units))
            throw ports.error(422, "MONEY_CARRY_SOURCE_INVALID", "Choose a positive immutable monetary source in this Company for the handoff.");
        const offset = input.allocationOffsetUnits === undefined ? null :
            BigInt(z.string().regex(/^\d{1,38}$/).parse(String(input.allocationOffsetUnits)));
        const sourceAmount = BigInt(source.amount_units), sourceUsd = BigInt(source.usd_units);
        if (offset !== null && offset + amount > sourceAmount)
            throw ports.error(422, "MONEY_CARRY_ALLOCATION_EXCEEDS_SOURCE", "The handoff allocation exceeds its original monetary source.");
        const rounded = (units: bigint) => (sourceUsd * units + sourceAmount / BigInt(2)) / sourceAmount;
        const usd = offset === null ? rounded(amount) : rounded(offset + amount) - rounded(offset);
        return { currency: source.currency, amountUnits: String(amount), usdUnits: String(usd), currencyScale: Number(source.currency_scale), rateId: source.fx_rate_id, usedLatestFallback: source.used_latest_fallback, capturedAt: ports.now(), allocationOffsetUnits: offset === null ? null : String(offset) };
    }
    async function snapshotMoneyReversal(actor: MoneyActor, input: ReversalInput): Promise<SnapshotPlan> {
        moneyIdentitySchema.extend({ amountUnits: z.string().regex(/^\d{1,38}$/) }).parse({ ...input, amountUnits: String(input.amountUnits) });
        const amount = BigInt(input.amountUnits);
        if (amount <= BigInt(0))
            throw ports.error(422, "INVALID_REVERSAL_AMOUNT", "A reversal must have a positive amount in original minor units.");
        const original = await ports.reversalSource(actor, input.original);
        if (!original)
            throw ports.error(404, "MONEY_SOURCE_SNAPSHOT_NOT_FOUND", "The original money evidence is unavailable in this Company.");
        const originalUnits = BigInt(original.amount_units), originalUsd = BigInt(original.usd_units);
        if (originalUnits <= BigInt(0) || originalUsd < BigInt(0) || amount > originalUnits)
            throw ports.error(422, "INVALID_REVERSAL_AMOUNT", "The reversal exceeds the positive original money evidence.");
        return { currency: original.currency, currencyScale: Number(original.currency_scale), rateId: original.fx_rate_id, usedLatestFallback: original.used_latest_fallback, amountUnits: String(-amount), usdUnits: '0', capturedAt: ports.now() };
    }
    return { quoteMoneyConversion, moneyContext, saveCurrencyPreference, confirmManualRate, configureCurrencyScale, snapshotMoney, snapshotMoneyBatch, snapshotMoneyCarry, snapshotMoneyReversal };
}

