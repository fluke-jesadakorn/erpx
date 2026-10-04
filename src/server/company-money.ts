import "server-only";
import { z } from "zod";
import { currencyDefinition, currencyDefinitions, currencyScale, convertWithRates, decimalUnits, unitsDecimal, type ManualRate } from "@/lib/company-money";
import { executeCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";

const code = z.string().regex(/^[A-Z]{3}$/).refine((value) => Boolean(currencyDefinition(value)), "Use a current ISO 4217 code.");
export const preferenceSchema = z.object({ currency: code });
export const manualRateSchema = z.object({
  currency: code.refine((value) => value !== "USD", "USD has a fixed rate of 1."),
  effectiveDate: z.iso.date(),
  currencyPerUsd: z.string().regex(/^\d{1,20}(?:\.\d{1,18})?$/, "Enter a positive decimal rate with at most 20 whole digits and 18 decimal places."),
  note: z.string().trim().max(500).default(""),
});
export const currencyScaleSchema = z.object({ currency: code, scale: z.number().int().min(0).max(12) });
export const conversionSchema = z.object({
  amount: z.string().regex(/^-?\d{1,26}(?:\.\d{1,12})?$/),
  currency: code,
  targetCurrency: code,
  transactionDate: z.iso.date(),
  requireRate: z.boolean().default(false),
});

type RateRow = { id: string; currency: string; effective_date: string; currency_per_usd: string; confirmed_at: number };
function rate(row: RateRow): ManualRate {
  const quote = String(row.currency_per_usd);
  return { id: row.id, currency: row.currency, effectiveDate: String(row.effective_date).slice(0, 10),
    currencyPerUsd: quote.includes(".") ? quote.replace(/0+$/, "").replace(/\.$/, "") : quote, confirmedAt: Number(row.confirmed_at) };
}

export async function companyRates(db: SqlDatabase, companyId: string): Promise<ManualRate[]> {
  return (await db.prepare("SELECT id,currency,effective_date,currency_per_usd::text AS currency_per_usd,confirmed_at FROM company_fx_rates WHERE company_id=? ORDER BY effective_date DESC,confirmed_at DESC,id DESC")
    .bind(companyId).all<RateRow>()).results.map(rate);
}

export async function quoteMoneyConversion(actor: WorkContext, input: z.infer<typeof conversionSchema>) {
  const { write } = await holdingResources(actor.holdingId);
  const specialCodes = [input.currency, input.targetCurrency].filter((value) => currencyDefinition(value)?.scale === null);
  const scaleRows = specialCodes.length ? (await write.prepare(`SELECT currency,scale FROM company_currency_scales WHERE company_id=? AND currency IN (${specialCodes.map(() => "?").join(",")})`)
    .bind(actor.companyId, ...specialCodes).all<{ currency: string; scale: number }>()).results : [];
  const scales = Object.fromEntries(scaleRows.map((row) => [row.currency, Number(row.scale)]));
  let sourceScale: number, targetScale: number;
  try { sourceScale = currencyScale(input.currency, scales[input.currency]); targetScale = currencyScale(input.targetCurrency, scales[input.targetCurrency]); }
  catch (cause) { throw new ApiError(422, "CURRENCY_SCALE_REQUIRED", cause instanceof Error ? cause.message : "Configure the currency scale."); }
  let amountUnits: bigint;
  try { amountUnits = decimalUnits(input.amount, sourceScale); }
  catch (cause) { throw new ApiError(422, "INVALID_MONEY_SCALE", cause instanceof Error ? cause.message : "Invalid money amount."); }
  if (input.currency === input.targetCurrency) return {
    original: { amount: unitsDecimal(amountUnits, sourceScale), currency: input.currency, scale: sourceScale },
    converted: { amount: unitsDecimal(amountUnits, targetScale), currency: input.targetCurrency, scale: targetScale,
      sourceRateVersion: null, targetRateVersion: null, usedLatestFallback: false, referenceOnly: !input.requireRate },
  };
  const converted = convertWithRates({ amountUnits, sourceCurrency: input.currency, sourceScale,
    targetCurrency: input.targetCurrency, targetScale, date: input.transactionDate, rates: await companyRates(write, actor.companyId) });
  if (!converted && input.requireRate) throw new ApiError(422, "MANUAL_RATE_REQUIRED", "A confirmed manual rate is required before this transaction can be recorded.");
  return {
    original: { amount: unitsDecimal(amountUnits, sourceScale), currency: input.currency, scale: sourceScale },
    converted: converted ? { amount: unitsDecimal(converted.amountUnits, targetScale), currency: input.targetCurrency, scale: targetScale,
      sourceRateVersion: converted.sourceRate.id, targetRateVersion: converted.targetRate.id,
      usedLatestFallback: converted.usedLatestFallback, referenceOnly: !input.requireRate } : null,
  };
}

export async function moneyContext(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  const [settings, preference, scales, rates] = await Promise.all([
    write.prepare("SELECT default_currency FROM company_money_settings WHERE company_id=?").bind(actor.companyId).first<{ default_currency: string }>(),
    write.prepare("SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?").bind(actor.companyId, actor.userId).first<{ display_currency: string }>(),
    write.prepare("SELECT currency,scale FROM company_currency_scales WHERE company_id=?").bind(actor.companyId).all<{ currency: string; scale: number }>(),
    companyRates(write, actor.companyId),
  ]);
  return {
    companyId: actor.companyId,
    defaultCurrency: settings?.default_currency ?? "USD",
    displayCurrency: preference?.display_currency ?? settings?.default_currency ?? "USD",
    scales: Object.fromEntries(scales.results.map((item) => [item.currency, Number(item.scale)])),
    rates,
    currencies: currencyDefinitions,
    canConfirmRate: !actor.branchId && !actor.departmentId && requirePermissionOrFalse(actor, "finance.approve"),
  };
}

function requirePermissionOrFalse(actor: WorkContext, permission: string) {
  try { requirePermission(actor, permission); return true; } catch { return false; }
}

export async function saveCurrencyPreference(actor: WorkContext, input: z.infer<typeof preferenceSchema>, key: string) {
  const { write } = await holdingResources(actor.holdingId), now = Date.now();
  return executeCommand(write, actor, key, "money.preference.set", input, `${actor.companyId}:${actor.userId}`, 1,
    { currency: input.currency }, [write.prepare(`INSERT INTO user_currency_preferences(company_id,user_id,display_currency,updated_at)
      VALUES(?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET display_currency=excluded.display_currency,version=user_currency_preferences.version+1,updated_at=excluded.updated_at`)
      .bind(actor.companyId, actor.userId, input.currency, now)]);
}

export async function confirmManualRate(actor: WorkContext, input: z.infer<typeof manualRateSchema>, key: string) {
  requirePermission(actor, "finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_FINANCE_REQUIRED", "A company-level Finance approver must confirm rates shared across the company.");
  if (decimalUnits(input.currencyPerUsd, 18) <= BigInt(0)) throw new ApiError(422, "INVALID_FX_RATE", "The manual rate must be positive.");
  const { write } = await holdingResources(actor.holdingId), id = `fx-${crypto.randomUUID()}`, now = Date.now();
  return executeCommand(write, actor, key, "money.rate.confirm", input, id, 1,
    { id, currency: input.currency, effectiveDate: input.effectiveDate, currencyPerUsd: input.currencyPerUsd, confirmedAt: now }, [
      write.prepare("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)")
        .bind(id, actor.companyId, input.currency, input.effectiveDate, input.currencyPerUsd, actor.userId, now, input.note),
    ]);
}

export async function configureCurrencyScale(actor: WorkContext, input: z.infer<typeof currencyScaleSchema>, key: string) {
  requirePermission(actor, "finance.approve");
  if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_FINANCE_REQUIRED", "A company-level Finance approver must configure currency precision.");
  const definition = currencyDefinition(input.currency)!;
  if (definition.scale !== null) throw new ApiError(422, "FIXED_CURRENCY_SCALE", "This ISO currency already has a fixed minor unit.");
  const { write } = await holdingResources(actor.holdingId), now = Date.now();
  const existing = await write.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?")
    .bind(actor.companyId, input.currency).first<{ scale: number }>();
  if (existing && Number(existing.scale) !== input.scale) throw new ApiError(409, "CURRENCY_SCALE_LOCKED", "Currency precision cannot change after setup.");
  return executeCommand(write, actor, key, "money.scale.configure", input, `${actor.companyId}:${input.currency}`, 1,
    { currency: input.currency, scale: input.scale }, [write.prepare("INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id,currency) DO NOTHING")
      .bind(actor.companyId, input.currency, input.scale, actor.userId, now)]);
}

const moneyIdentitySchema = z.object({
  sourceType: z.string().trim().min(1).max(100),
  sourceId: z.string().trim().min(1).max(200),
  amountKind: z.string().trim().min(1).max(100),
  transactionDate: z.iso.date(),
  amountUnits: z.string().regex(/^-?\d{1,38}$/),
});

export async function snapshotMoney(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, input: {
  sourceType: string; sourceId: string; amountKind: string; transactionDate: string;
  currency: string; amountUnits: bigint | string; currencyScale?: number;
}) {
  moneyIdentitySchema.extend({ currency: code }).parse({ ...input, amountUnits: String(input.amountUnits) });
  const customScale = await db.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?")
    .bind(actor.companyId, input.currency).first<{ scale: number }>();
  const configuredScale = currencyScale(input.currency, customScale?.scale);
  if (input.currencyScale !== undefined && input.currencyScale !== configuredScale)
    throw new ApiError(422, "CURRENCY_SCALE_MISMATCH", `A ${input.currency} transaction must use ${configuredScale} decimal places.`);
  const scale = configuredScale;
  const converted = convertWithRates({ amountUnits: input.amountUnits, sourceCurrency: input.currency, sourceScale: scale,
    targetCurrency: "USD", targetScale: 2, date: input.transactionDate, rates: await companyRates(db, actor.companyId) });
  if (!converted) throw new ApiError(422, "MANUAL_RATE_REQUIRED", `A confirmed manual ${input.currency}/USD rate is required.`);
  const now = Date.now();
  return {
    amountUnits: String(input.amountUnits), usdUnits: String(converted.amountUnits), currencyScale: scale,
    rateId: converted.sourceRate.id === "USD" ? null : converted.sourceRate.id,
    usedLatestFallback: converted.usedLatestFallback,
    statement: db.prepare(`INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate,
        input.currency, scale, String(input.amountUnits), String(converted.amountUnits),
        converted.sourceRate.id === "USD" ? null : converted.sourceRate.id, converted.usedLatestFallback, now),
  };
}

/** An internal handoff preserves the source currency/scale/quote at its new date. */
export async function snapshotMoneyCarry(db:SqlDatabase,actor:Pick<WorkContext,"companyId">,input:{
 sourceType:string;sourceId:string;amountKind:string;transactionDate:string;amountUnits:bigint|string;
 allocationOffsetUnits?:bigint|string;
 original:{sourceType:string;sourceId:string;amountKind:string};
}) {
 moneyIdentitySchema.parse({...input,amountUnits:String(input.amountUnits)});
 moneyIdentitySchema.pick({sourceType:true,sourceId:true,amountKind:true}).parse(input.original);
 const source=await db.prepare(`SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,original_source_type
   FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?`)
  .bind(actor.companyId,input.original.sourceType,input.original.sourceId,input.original.amountKind)
  .first<{currency:string;currency_scale:number;amount_units:string;usd_units:string;fx_rate_id:string|null;used_latest_fallback:boolean;original_source_type:string|null}>();
 const amount=BigInt(input.amountUnits);
 if(!source||BigInt(source.amount_units)<=BigInt(0)||source.original_source_type!==null||amount<BigInt(0)||amount>BigInt(source.amount_units))
  throw new ApiError(422,"MONEY_CARRY_SOURCE_INVALID","Choose a positive immutable monetary source in this Company for the handoff.");
 const offset=input.allocationOffsetUnits===undefined?null:
  BigInt(z.string().regex(/^\d{1,38}$/).parse(String(input.allocationOffsetUnits)));
 const sourceAmount=BigInt(source.amount_units),sourceUsd=BigInt(source.usd_units);
 if(offset!==null&&offset+amount>sourceAmount)
  throw new ApiError(422,"MONEY_CARRY_ALLOCATION_EXCEEDS_SOURCE","The handoff allocation exceeds its original monetary source.");
 const rounded=(units:bigint)=>(sourceUsd*units+sourceAmount/BigInt(2))/sourceAmount;
 const usd=offset===null?rounded(amount):rounded(offset+amount)-rounded(offset);
 return {amountUnits:String(amount),usdUnits:String(usd),currencyScale:Number(source.currency_scale),rateId:source.fx_rate_id,usedLatestFallback:source.used_latest_fallback,
  statement:db.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind,carried_allocation_offset_units)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(actor.companyId,input.sourceType,input.sourceId,input.amountKind,input.transactionDate,source.currency,source.currency_scale,String(amount),String(usd),source.fx_rate_id,source.used_latest_fallback,Date.now(),input.original.sourceType,input.original.sourceId,input.original.amountKind,offset===null?null:String(offset))};
}

/** A return reverses the original evidence, including rounding, instead of quoting today's FX. */
export async function snapshotMoneyReversal(db: SqlDatabase, actor: Pick<WorkContext, "companyId">, input: {
  sourceType: string; sourceId: string; amountKind: string; transactionDate: string;
  amountUnits: bigint | string;
  original: { sourceType: string; sourceId: string; amountKind: string };
}) {
  moneyIdentitySchema.extend({ amountUnits: z.string().regex(/^\d{1,38}$/) }).parse({ ...input, amountUnits: String(input.amountUnits) });
  const amount = BigInt(input.amountUnits);
  if (amount <= BigInt(0)) throw new ApiError(422, "INVALID_REVERSAL_AMOUNT", "A reversal must have a positive amount in original minor units.");
  const original = await db.prepare(`SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback
    FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?`)
    .bind(actor.companyId, input.original.sourceType, input.original.sourceId, input.original.amountKind)
    .first<{ currency: string; currency_scale: number; amount_units: string; usd_units: string; fx_rate_id: string | null; used_latest_fallback: boolean }>();
  if (!original) throw new ApiError(404, "MONEY_SOURCE_SNAPSHOT_NOT_FOUND", "The original money evidence is unavailable in this Company.");
  const originalUnits = BigInt(original.amount_units), originalUsd = BigInt(original.usd_units);
  if (originalUnits <= BigInt(0) || originalUsd < BigInt(0) || amount > originalUnits)
    throw new ApiError(422, "INVALID_REVERSAL_AMOUNT", "The reversal exceeds the positive original money evidence.");
  // The INSERT trigger locks the source and uses cumulative allocation. Returning
  // the original USD total exactly matters when many small partial returns round.
  return {
    currency: original.currency, currencyScale: Number(original.currency_scale),
    rateId: original.fx_rate_id, usedLatestFallback: original.used_latest_fallback,
    amountUnits: String(-amount),
    statement: db.prepare(`INSERT INTO money_transaction_snapshots
      (company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,
       original_source_type,original_source_id,original_amount_kind)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(actor.companyId, input.sourceType, input.sourceId, input.amountKind, input.transactionDate,
        original.currency, Number(original.currency_scale), String(-amount), "0", original.fx_rate_id,
        original.used_latest_fallback, Date.now(), input.original.sourceType, input.original.sourceId, input.original.amountKind),
  };
}
