import { companyIdRowsSql } from './finance-read-tail-r56-sql';
import{companyRates}from '@erpx/finance-server/public/company-money-rates';
import { withFinanceConfigurationCommand } from "@erpx/finance-server/adapters/purchase-boundary";
import { nativeFinanceOwnerWriteSystem } from './native-finance-owner-write';
import { createCompanyMoney } from '@erpx/finance-server/application/company-money';
import type { CarrySource, SnapshotSource, SnapshotInput } from '@erpx/finance-server/domain/company-money';
import "server-only";
import { z } from "zod";

import { executeCommand } from "@erpx/workflow-server/public/commands";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
export { preferenceSchema, manualRateSchema, currencyScaleSchema, conversionSchema } from '@erpx/finance-server/domain/company-money';
import { preferenceSchema, manualRateSchema, currencyScaleSchema, conversionSchema } from '@erpx/finance-server/domain/company-money';

export async function quoteMoneyConversion(actor: WorkContext, input: z.infer<typeof conversionSchema>) { return moneyService().quoteMoneyConversion(actor, input); }
export async function moneyContext(actor: WorkContext) { return moneyService().moneyContext(actor); }
export async function saveCurrencyPreference(actor: WorkContext, input: z.infer<typeof preferenceSchema>, key: string) { return moneyService().saveCurrencyPreference(actor, input, key); }
export async function confirmManualRate(actor: WorkContext, input: z.infer<typeof manualRateSchema>, key: string) { return withFinanceConfigurationCommand(actor,{key,operation:"money.rate.confirm",purpose:"new-work"},tx=>moneyService(tx).confirmManualRate(actor,input,key)); }

export async function configureCurrencyScale(actor: WorkContext, input: z.infer<typeof currencyScaleSchema>, key: string) { return withFinanceConfigurationCommand(actor,{key,operation:"money.scale.configure",purpose:"new-work"},tx=>moneyService(tx).configureCurrencyScale(actor,input,key)); }
export async function snapshotMoney(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, input: {
    sourceType: string;
    sourceId: string;
    amountKind: string;
    transactionDate: string;
    currency: string;
    amountUnits: bigint | string;
    currencyScale?: number;
}) {
    const plan = await moneyService(db).snapshotMoney(actor, input);
    return {
        amountUnits: plan.amountUnits, usdUnits: plan.usdUnits, currencyScale: plan.currencyScale,
        rateId: plan.rateId,
        usedLatestFallback: plan.usedLatestFallback,
        statement: await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'money_snapshot_insert', [actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, input.currency, plan.currencyScale, plan.amountUnits, plan.usdUnits, plan.rateId, plan.usedLatestFallback, plan.capturedAt]),
    };
}
/** Prepare independent quote rows together; the caller retains its original owner transaction. */
export async function snapshotMoneyBatch(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, inputs: readonly SnapshotInput[]) {
    const plans = await moneyService(db).snapshotMoneyBatch(actor, inputs);
    const statements = [];
    // Bound parameters stay well below PostgreSQL's limit, including large group bookings.
    for (let offset = 0; offset < inputs.length; offset += 500) {
        const chunk = inputs.slice(offset, offset + 500);
        const values = chunk.flatMap((input, index) => {
            const plan = plans[offset + index];
            return [actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, input.currency,
                plan.currencyScale, plan.amountUnits, plan.usdUnits, plan.rateId, plan.usedLatestFallback, plan.capturedAt];
        });
        statements.push(await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'money_snapshot_batch_insert', [...values]));
    }
    return { plans, statements };
}
/** An internal handoff preserves the source currency/scale/quote at its new date. */
export async function snapshotMoneyCarry(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, input: {
    sourceType: string;
    sourceId: string;
    amountKind: string;
    transactionDate: string;
    amountUnits: bigint | string;
    allocationOffsetUnits?: bigint | string;
    original: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
    };
}) {
    const plan = await moneyService(db).snapshotMoneyCarry(actor, input);
    return { amountUnits: plan.amountUnits, usdUnits: plan.usdUnits, currencyScale: plan.currencyScale, rateId: plan.rateId, usedLatestFallback: plan.usedLatestFallback,
        statement: await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'money_snapshot_insert2', [actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, plan.currency, plan.currencyScale, plan.amountUnits, plan.usdUnits, plan.rateId, plan.usedLatestFallback, plan.capturedAt, input.original.sourceType, input.original.sourceId, input.original.amountKind, plan.allocationOffsetUnits])};
}
/** A return reverses the original evidence, including rounding, instead of quoting today's FX. */
export async function snapshotMoneyReversal(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, input: {
    sourceType: string;
    sourceId: string;
    amountKind: string;
    transactionDate: string;
    amountUnits: bigint | string;
    original: {
        sourceType: string;
        sourceId: string;
        amountKind: string;
    };
}) {
    const plan = await moneyService(db).snapshotMoneyReversal(actor, input); // The INSERT trigger locks the source and uses cumulative allocation. Returning
    // the original USD total exactly matters when many small partial returns round.
    return {
        currency: plan.currency, currencyScale: plan.currencyScale,
        rateId: plan.rateId, usedLatestFallback: plan.usedLatestFallback,
        amountUnits: plan.amountUnits,
        statement: await nativeFinanceOwnerWriteSystem(db, actor.companyId, 'money_snapshot_insert3', [actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, plan.currency, plan.currencyScale, plan.amountUnits, "0", plan.rateId, plan.usedLatestFallback, plan.capturedAt, input.original.sourceType, input.original.sourceId, input.original.amountKind]),
    };
}
/** Finance-internal port for an already-owned posting setup transaction.
 * The original command service retains its authority, input and native receipt.
 * Keep this private to Finance adapters; public setup ports own all boundaries. */
export function confirmPostingRateInTransaction(database:SqlDatabase,actor:WorkContext,input:z.infer<typeof manualRateSchema>,key:string,boundRequest:unknown){
    return moneyService(database,boundRequest).confirmManualRate(actor,input,key);
}
function moneyService(provided?: SqlDatabase,boundRequest?:unknown) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: Pick<WorkContext, 'companyId'> & Partial<Pick<WorkContext, 'holdingId'>>) => pending ??= holdingResources(actor.holdingId!).then(r => r.write);
    return createCompanyMoney({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, id: () => crypto.randomUUID(), rates: async (companyId) => companyRates(await pending!, companyId), scales: async (companyId, currencies) => currencies ? (await (await pending!).prepare(companyIdRowsSql('currency,scale','company_currency_scales',` AND currency IN (${currencies.map(() => '?').join(',')})`)).bind(companyId, ...currencies).all<{
            currency: string;
            scale: number;
        }>()).results : (await (await pending!).prepare(companyIdRowsSql('currency,scale','company_currency_scales','')).bind(companyId).all<{
            currency: string;
            scale: number;
        }>()).results, scale: async (companyId, currency) => (await pending!).prepare('SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?').bind(companyId, currency).first<{
            scale: number;
        }>(), settings: async (companyId) => (await pending!).prepare('SELECT default_currency FROM company_money_settings WHERE company_id=?').bind(companyId).first<{
            default_currency: string;
        }>(), preference: async (actor) => (await db(actor)).prepare('SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?').bind(actor.companyId, actor.userId).first<{
            display_currency: string;
        }>(), carrySource: async (actor, original) => (await db(actor)).prepare(`SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,original_source_type FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?`).bind(actor.companyId, original.sourceType, original.sourceId, original.amountKind).first<CarrySource>(), reversalSource: async (actor, original) => (await db(actor)).prepare(`SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?`).bind(actor.companyId, original.sourceType, original.sourceId, original.amountKind).first<SnapshotSource>(), commitPreference: async ({ actor, input, key, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "money.preference.set", input, `${actor.companyId}:${actor.userId}`, 1, { currency: input.currency }, [await nativeFinanceOwnerWriteSystem(write, actor.companyId, 'currency_preference_insert', [actor.companyId, actor.userId, input.currency, now])]);
        }, commitRate: async ({ actor, input, key, id, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "money.rate.confirm", boundRequest??input, id, 1, { id, currency: input.currency, effectiveDate: input.effectiveDate, currencyPerUsd: input.currencyPerUsd, confirmedAt: now }, [
                await nativeFinanceOwnerWriteSystem(write, actor.companyId, 'fx_rate_insert', [id, actor.companyId, input.currency, input.effectiveDate, input.currencyPerUsd, actor.userId, now, input.note]),
            ]);
        }, commitScale: async ({ actor, input, key, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "money.scale.configure", input, `${actor.companyId}:${input.currency}`, 1, { currency: input.currency, scale: input.scale }, [await nativeFinanceOwnerWriteSystem(write, actor.companyId, 'currency_scale_insert', [actor.companyId, input.currency, input.scale, actor.userId, now])]);
        } });
}
