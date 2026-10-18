import { withFinanceConfigurationCommand } from "./purchase-boundary";
import { createCompanyMoney } from '../application/company-money';
import type { CarrySource, SnapshotSource, SnapshotInput } from '../domain/company-money';
import "server-only";
import { z } from "zod";
import { type ManualRate } from "@/lib/finance/company-money";
import { executeCommand } from "@/server/modules/workflow/public/commands";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
export { preferenceSchema, manualRateSchema, currencyScaleSchema, conversionSchema } from '../domain/company-money';
import { preferenceSchema, manualRateSchema, currencyScaleSchema, conversionSchema } from '../domain/company-money';
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
        statement: db.prepare(`INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
            .bind(actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, input.currency, plan.currencyScale, plan.amountUnits, plan.usdUnits, plan.rateId, plan.usedLatestFallback, plan.capturedAt),
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
        statements.push(db.prepare(`INSERT INTO money_transaction_snapshots
            (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
            VALUES ${chunk.map(() => '(?,?,?,?,?,?,?,?,?,?,?,?)').join(',')}`).bind(...values));
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
        statement: db.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind,carried_allocation_offset_units)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, plan.currency, plan.currencyScale, plan.amountUnits, plan.usdUnits, plan.rateId, plan.usedLatestFallback, plan.capturedAt, input.original.sourceType, input.original.sourceId, input.original.amountKind, plan.allocationOffsetUnits) };
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
        statement: db.prepare(`INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,
       original_source_type,original_source_id,original_amount_kind)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
            .bind(actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate, plan.currency, plan.currencyScale, plan.amountUnits, "0", plan.rateId, plan.usedLatestFallback, plan.capturedAt, input.original.sourceType, input.original.sourceId, input.original.amountKind),
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
    return createCompanyMoney({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, id: () => crypto.randomUUID(), rates: async (companyId) => companyRates(await pending!, companyId), scales: async (companyId, currencies) => currencies ? (await (await pending!).prepare(`SELECT currency,scale FROM company_currency_scales WHERE company_id=? AND currency IN (${currencies.map(() => '?').join(',')})`).bind(companyId, ...currencies).all<{
            currency: string;
            scale: number;
        }>()).results : (await (await pending!).prepare('SELECT currency,scale FROM company_currency_scales WHERE company_id=?').bind(companyId).all<{
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
            return executeCommand(write, actor, key, "money.preference.set", input, `${actor.companyId}:${actor.userId}`, 1, { currency: input.currency }, [write.prepare(`INSERT INTO user_currency_preferences(company_id,user_id,display_currency,updated_at)
      VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET display_currency=excluded.display_currency,version=user_currency_preferences.version+1,updated_at=excluded.updated_at`)
                    .bind(actor.companyId, actor.userId, input.currency, now)]);
        }, commitRate: async ({ actor, input, key, id, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "money.rate.confirm", boundRequest??input, id, 1, { id, currency: input.currency, effectiveDate: input.effectiveDate, currencyPerUsd: input.currencyPerUsd, confirmedAt: now }, [
                write.prepare("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)")
                    .bind(id, actor.companyId, input.currency, input.effectiveDate, input.currencyPerUsd, actor.userId, now, input.note),
            ]);
        }, commitScale: async ({ actor, input, key, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "money.scale.configure", input, `${actor.companyId}:${input.currency}`, 1, { currency: input.currency, scale: input.scale }, [write.prepare("INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,currency) DO NOTHING")
                    .bind(actor.companyId, input.currency, input.scale, actor.userId, now)]);
        } });
}












