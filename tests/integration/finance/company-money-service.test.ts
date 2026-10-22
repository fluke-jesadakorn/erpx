import { beforeAll, describe, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { companyRates, configureCurrencyScale, confirmManualRate, quoteMoneyConversion, saveCurrencyPreference, snapshotMoney, snapshotMoneyBatch, snapshotMoneyCarry, snapshotMoneyReversal } from "@erpx/server-finance/public/company-money";
import { executeCommand } from "@erpx/server-workflow/public/commands";

vi.mock("@erpx/server-runtime/adapters/runtime", async () => {
  const { env } = await import("../../support/runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});

const db = env.H001_DB;
const approver = { holdingId: "h001", companyId: "money-company-a", userId: "money-approver", assignmentId: "money-assignment-a", role: "finance_approver", branchId: null, departmentId: null } as WorkContext;
const otherCompany = { ...approver, companyId: "money-company-b", assignmentId: "money-assignment-b" } as WorkContext;
const preparer = { ...approver, userId: "money-preparer", assignmentId: "money-assignment-preparer", role: "finance_preparer" } as WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await db.exec(`
    INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES
      ('money-company-a','company','money-company-a','Money A','money-company-a'),
      ('money-company-b','company','money-company-b','Money B','money-company-b');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES
      ('money-assignment-a','money-approver','finance_approver','["finance"]','money-company-a','holding',0,9999999999999),
      ('money-assignment-b','money-approver','finance_approver','["finance"]','money-company-b','holding',0,9999999999999),
      ('money-assignment-preparer','money-preparer','finance_preparer','["finance"]','money-company-a','holding',0,9999999999999);
  `);
});

describe("company manual currency persistence", () => {
  it("conserves the original USD cent across disjoint split handoffs and rejects concurrent overlap", async () => {
    const original={sourceType:"test_provider_payment",sourceId:"split-payment",amountKind:"requested"};
    const rate=await confirmManualRate(approver,{currency:"THB",effectiveDate:"2026-07-01",currencyPerUsd:"36",note:"Reviewed provider allocation"},"money-split-original-rate");
    await db.batch([(await snapshotMoney(db,approver,{...original,transactionDate:"2026-07-01",currency:"THB",amountUnits:"42800"})).statement]);
    await confirmManualRate(approver,{currency:"THB",effectiveDate:"2026-07-02",currencyPerUsd:"40",note:"Later rate"},"money-split-later-rate");
    const first={sourceType:"test_payment_allocation",sourceId:"split-first",amountKind:"total",transactionDate:"2026-07-02",amountUnits:"21400",allocationOffsetUnits:"0",original};
    const competing=await Promise.all(["split-first","split-overlap"].map(sourceId=>snapshotMoneyCarry(db,approver,{...first,sourceId})));
    const attempts=await Promise.allSettled(competing.map(value=>db.batch([value.statement])));
    expect(attempts.filter(value=>value.status==='fulfilled')).toHaveLength(1);
    expect(attempts.filter(value=>value.status==='rejected').map(value=>(value as PromiseRejectedResult).reason.message).join()).toContain("MONEY_CARRY_ALLOCATION_OVERLAP");
    const final={...first,sourceId:"split-final",allocationOffsetUnits:"21400"};
    const snapshot=await snapshotMoneyCarry(db,approver,final);
    expect(snapshot.usdUnits).toBe("594");
    const run=()=>executeCommand(db,approver,"money-split-lost-response","test.payment.allocate",final,final.sourceId,1,{id:final.sourceId},[snapshot.statement]);
    const committed=await run();expect(await run()).toEqual(committed);
    expect(await db.prepare("SELECT SUM(amount_units)::text amount,SUM(usd_units)::text usd,COUNT(*)::int n FROM money_transaction_snapshots WHERE company_id=? AND carried_source_type=? AND carried_source_id=?").bind(approver.companyId,original.sourceType,original.sourceId).first()).toEqual({amount:"42800",usd:"1189",n:2});
    const rows=(await db.prepare("SELECT source_type,source_id,amount_kind,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND carried_source_id=? ORDER BY carried_allocation_offset_units").bind(approver.companyId,original.sourceId).all<{source_type:string;source_id:string;amount_kind:string;fx_rate_id:string}>()).results;
    expect(rows.every(row=>row.fx_rate_id===rate.id)).toBe(true);
    for(const row of rows)await db.batch([(await snapshotMoneyReversal(db,approver,{sourceType:"test_allocation_refund",sourceId:`refund-${row.source_id}`,amountKind:"total",transactionDate:"2026-07-02",amountUnits:"21400",original:{sourceType:row.source_type,sourceId:row.source_id,amountKind:row.amount_kind}})).statement]);
    expect(await db.prepare("SELECT SUM(amount_units)::text amount,SUM(usd_units)::text usd FROM money_transaction_snapshots WHERE company_id=? AND source_type='test_allocation_refund'").bind(approver.companyId).first()).toEqual({amount:"-42800",usd:"-1189"});
    await expect(snapshotMoneyCarry(db,approver,{...final,sourceId:"split-excess",allocationOffsetUnits:"21401"})).rejects.toMatchObject({code:"MONEY_CARRY_ALLOCATION_EXCEEDS_SOURCE"});
    await expect(snapshotMoneyCarry(db,otherCompany,first)).rejects.toMatchObject({code:"MONEY_CARRY_SOURCE_INVALID"});
    await expect(db.prepare("UPDATE money_transaction_snapshots SET carried_allocation_offset_units=1 WHERE company_id=? AND source_id=?").bind(approver.companyId,final.sourceId).run()).rejects.toMatchObject({code:"23514"});
  });
  it("carries reviewed prices into a dated handoff without replacing its quote or consuming the return allowance", async () => {
    const rate = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-08-01", currencyPerUsd: "36", note: "Original reviewed price" }, "money-carry-original-rate");
    const original = { sourceType: "test_reviewed_price", sourceId: "booked-price", amountKind: "night_total" };
    await db.batch([(await snapshotMoney(db, approver, { ...original, transactionDate: "2026-08-01", currency: "THB", amountUnits: "10700" })).statement]);
    await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-08-02", currencyPerUsd: "40", note: "Later quote must not reprice a handoff" }, "money-carry-later-rate");
    const input = { sourceType: "test_charge", sourceId: "night-posting", amountKind: "total", transactionDate: "2026-08-02", amountUnits: "10700", original };
    await db.batch([(await snapshotMoneyCarry(db, approver, input)).statement]);
    expect(await db.prepare("SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,transaction_date::text,carried_source_id FROM money_transaction_snapshots WHERE company_id=? AND source_id=?").bind(approver.companyId, input.sourceId).first())
      .toEqual({ currency: "THB", currency_scale: 2, amount_units: "10700", usd_units: "297", fx_rate_id: rate.id, transaction_date: "2026-08-02", carried_source_id: "booked-price" });
    await expect(snapshotMoneyCarry(db, otherCompany, input)).rejects.toMatchObject({ code: "MONEY_CARRY_SOURCE_INVALID" });
    await expect(snapshotMoneyCarry(db, approver, { ...input, amountUnits: "10701" })).rejects.toMatchObject({ code: "MONEY_CARRY_SOURCE_INVALID" });
    const charge = { sourceType: input.sourceType, sourceId: input.sourceId, amountKind: input.amountKind };
    await db.batch([(await snapshotMoneyReversal(db, approver, { ...input, sourceId: "voided-night", amountUnits: "10700", original: charge })).statement]);
    expect(await db.prepare("SELECT SUM(amount_units)::text amount,SUM(usd_units)::text usd FROM money_transaction_snapshots WHERE source_id=? OR original_source_id=?").bind(input.sourceId, input.sourceId).first()).toEqual({ amount: "0", usd: "0" });
    const raw = { sourceType: "test_charge", sourceId: "tampered-handoff", amountKind: "total", transactionDate: "2026-08-02", currency: "USD", amountUnits: "100" };
    const tampered = await snapshotMoney(db, approver, raw);
    // Native SQL constraints enforce provenance even when a client bypasses the helper.
    await expect(db.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind)
      VALUES(?,?,?,?,?,'USD',2,100,100,NULL,false,1,?,?,?)`).bind(approver.companyId, raw.sourceType, raw.sourceId, raw.amountKind, raw.transactionDate, original.sourceType, original.sourceId, original.amountKind).run()).rejects.toThrow("MONEY_CARRY_EVIDENCE_MISMATCH");
    expect(tampered.amountUnits).toBe("100");
    await expect(db.prepare("UPDATE money_transaction_snapshots SET carried_source_id='changed' WHERE company_id=? AND source_id=?").bind(approver.companyId, input.sourceId).run()).rejects.toMatchObject({ code: "23514" });
  });
  it("rejects fractional minor units and invalid business dates before taking money evidence", async () => {
    const input = { sourceType: "test_sale", sourceId: "invalid-units", amountKind: "total", transactionDate: "2026-10-02", currency: "USD", amountUnits: "1.5" };
    await expect(snapshotMoney(db, approver, input)).rejects.toThrow();
    await expect(snapshotMoney(db, approver, { ...input, amountUnits: "100", transactionDate: "2026-02-30" })).rejects.toThrow();
    expect(await db.prepare("SELECT 1 FROM money_transaction_snapshots WHERE source_id='invalid-units'").first()).toBeNull();
  });

  it("scopes preferences and the custom ISO precision to one company", async () => {
    expect(await db.prepare("SELECT default_currency FROM company_money_settings WHERE company_id=?").bind(approver.companyId).first()).toEqual({ default_currency: "USD" });
    await saveCurrencyPreference(approver, { currency: "KWD" }, "money-pref-001");
    await configureCurrencyScale(approver, { currency: "XAU", scale: 6 }, "money-scale-001");
    expect(await db.prepare("SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?").bind(approver.companyId, approver.userId).first()).toEqual({ display_currency: "KWD" });
    expect(await db.prepare("SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?").bind(otherCompany.companyId, approver.userId).first()).toBeNull();
    expect(await db.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency='XAU'").bind(otherCompany.companyId).first()).toBeNull();
    await expect(configureCurrencyScale(approver, { currency: "XAU", scale: 2 }, "money-scale-002")).rejects.toMatchObject({ code: "CURRENCY_SCALE_LOCKED" });
    await expect(configureCurrencyScale(preparer, { currency: "XAU", scale: 6 }, "money-scale-003")).rejects.toMatchObject({ status: 403 });
    await expect(configureCurrencyScale({ ...approver, branchId: "branch-1" }, { currency: "XTS", scale: 2 }, "money-scale-branch")).rejects.toMatchObject({ code: "COMPANY_FINANCE_REQUIRED" });
  });

  it("keeps rate revisions and transaction snapshots immutable across a later correction", async () => {
    await expect(confirmManualRate(preparer, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "unauthorized" }, "money-rate-denied")).rejects.toMatchObject({ status: 403 });
    await expect(confirmManualRate({ ...approver, branchId: "branch-1" }, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "branch" }, "money-rate-branch")).rejects.toMatchObject({ code: "COMPANY_FINANCE_REQUIRED" });
    const first = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "manual rate" }, "money-rate-001");
    expect((await companyRates(db, approver.companyId))[0].id).toBe(first.id);
    const quote = await quoteMoneyConversion(approver, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true });
    expect(quote.converted).toMatchObject({ amount: "3600.00", currency: "THB", targetRateVersion: first.id });
    expect((await quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: false })).converted).toBeNull();
    expect((await quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "THB", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true })).converted).toMatchObject({ amount: "100.00", sourceRateVersion: null });
    await expect(quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true })).rejects.toMatchObject({ code: "MANUAL_RATE_REQUIRED" });

    const snapshot = await snapshotMoney(db, approver, { sourceType: "test_sale", sourceId: "sale-1", amountKind: "total", transactionDate: "2026-09-29", currency: "THB", amountUnits: "360000" });
    await db.batch([snapshot.statement]);
    const second = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "37", note: "corrected" }, "money-rate-002");
    expect(second.id).not.toBe(first.id);
    const recorded = await db.prepare("SELECT currency,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_id='sale-1'").bind(approver.companyId).first();
    expect(recorded).toEqual({ currency: "THB", amount_units: "360000", usd_units: "10000", fx_rate_id: first.id });
    await expect(db.prepare("UPDATE company_fx_rates SET currency_per_usd=38 WHERE id=?").bind(first.id).run()).rejects.toMatchObject({ code: "23514" });
    await expect(db.prepare("UPDATE money_transaction_snapshots SET usd_units=1 WHERE source_id='sale-1'").run()).rejects.toMatchObject({ code: "23514" });
  });

  it("reverses partial returns at the original FX and conserves rounded units after a rate change", async () => {
    const initial = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-10-01", currencyPerUsd: "3", note: "Small-unit rounding fixture" }, "money-return-rate-1");
    const source = { sourceType: "test_sale", sourceId: "partial-sale", amountKind: "total" };
    await db.batch([(await snapshotMoney(db, approver, { ...source, transactionDate: "2026-10-01", currency: "THB", amountUnits: "3" })).statement]);
    await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-10-02", currencyPerUsd: "100", note: "Must not revalue the return" }, "money-return-rate-2");
    for (let i = 1; i <= 3; i++) {
      const reversal = await snapshotMoneyReversal(db, approver, { sourceType: "test_return", sourceId: `partial-return-${i}`, amountKind: "refund", transactionDate: "2026-10-02", amountUnits: "1", original: source });
      await db.batch([reversal.statement]);
    }
    const rows = (await db.prepare("SELECT currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,original_source_id FROM money_transaction_snapshots WHERE original_source_id=? ORDER BY source_id").bind(source.sourceId).all()).results;
    expect(rows).toEqual([-0, -1, -0].map(usd => ({ currency: "THB", currency_scale: 2, amount_units: "-1", usd_units: String(usd), fx_rate_id: initial.id, original_source_id: source.sourceId })));
    expect(await db.prepare("SELECT SUM(amount_units)::text original_units,SUM(usd_units)::text usd_units FROM money_transaction_snapshots WHERE source_id=? OR original_source_id=?").bind(source.sourceId, source.sourceId).first()).toEqual({ original_units: "0", usd_units: "0" });
    await expect(snapshotMoneyReversal(db, otherCompany, { sourceType: "test_return", sourceId: "cross-company", amountKind: "refund", transactionDate: "2026-10-02", amountUnits: "1", original: source })).rejects.toMatchObject({ code: "MONEY_SOURCE_SNAPSHOT_NOT_FOUND" });
    const excess = await snapshotMoneyReversal(db, approver, { sourceType: "test_return", sourceId: "excess-return", amountKind: "refund", transactionDate: "2026-10-02", amountUnits: "1", original: source });
    await expect(db.batch([excess.statement])).rejects.toThrow("MONEY_REVERSAL_EXCEEDS_SOURCE");
    await expect(db.prepare("UPDATE money_transaction_snapshots SET original_source_id='changed' WHERE source_id='partial-return-1'").run()).rejects.toMatchObject({ code: "23514" });
  });

  it("serializes simultaneous reversals and replays a committed command after its response is lost", async () => {
    const original = { sourceType: "test_sale", sourceId: "concurrent-sale", amountKind: "total" };
    await db.batch([(await snapshotMoney(db, approver, { ...original, transactionDate: "2026-10-02", currency: "USD", amountUnits: "100" })).statement]);
    const candidates = await Promise.all([1, 2].map(i => snapshotMoneyReversal(db, approver, { sourceType: "test_return", sourceId: `concurrent-return-${i}`, amountKind: "refund", transactionDate: "2026-10-02", amountUnits: "75", original })));
    const attempts = await Promise.allSettled(candidates.map(candidate => db.batch([candidate.statement])));
    expect(attempts.filter(result => result.status === "fulfilled")).toHaveLength(1);
    expect(await db.prepare("SELECT SUM(amount_units)::text amount FROM money_transaction_snapshots WHERE original_source_id=?").bind(original.sourceId).first()).toEqual({ amount: "-75" });
    const finalInput = { sourceType: "test_return", sourceId: "lost-response-return", amountKind: "refund", transactionDate: "2026-10-02", amountUnits: "25", original };
    const final = await snapshotMoneyReversal(db, approver, finalInput);
    const run = () => executeCommand(db, approver, "money-return-lost-response", "test.return.confirm", finalInput, finalInput.sourceId, 1, { id: finalInput.sourceId, status: "confirmed" }, [final.statement]);
    const committed = await run();
    expect(await run()).toEqual(committed);
    expect(await db.prepare("SELECT COUNT(*)::int n FROM money_transaction_snapshots WHERE source_id=?").bind(finalInput.sourceId).first()).toEqual({ n: 1 });
    expect(await db.prepare("SELECT SUM(amount_units)::text original_units,SUM(usd_units)::text usd_units FROM money_transaction_snapshots WHERE source_id=? OR original_source_id=?").bind(original.sourceId, original.sourceId).first()).toEqual({ original_units: "0", usd_units: "0" });
  });
});


describe("explicit money snapshot batches", () => {
  it("preserves individual date quotes, rounding and Company isolation with fresh facts on the next batch", async () => {
    const a = await confirmManualRate(approver, { currency: 'THB', effectiveDate: '2026-11-01', currencyPerUsd: '36', note: 'Batch reviewed rate' }, 'money-batch-a');
    const b = await confirmManualRate(approver, { currency: 'THB', effectiveDate: '2026-11-02', currencyPerUsd: '40', note: 'Batch next-day rate' }, 'money-batch-b');
    const inputs = ['2026-11-01', '2026-11-02'].map((date, i) => ({ sourceType: 'test_batch', sourceId: 'batch-night-' + i, amountKind: 'total', currency: 'THB', currencyScale: 2, transactionDate: date, amountUnits: '10700' }));
    const individual = await Promise.all(inputs.map(input => snapshotMoney(db, approver, input)));
    const prepared = await snapshotMoneyBatch(db, approver, inputs);
    expect(prepared.statements).toHaveLength(1);
    expect(prepared.plans.map(p => [p.usdUnits, p.rateId, p.usedLatestFallback])).toEqual(individual.map(p => [p.usdUnits, p.rateId, p.usedLatestFallback]));
    expect(prepared.plans.map(p => p.rateId)).toEqual([a.id, b.id]);
    await db.batch(prepared.statements);
    expect((await db.prepare("SELECT source_id,amount_units::text amount,usd_units::text usd,transaction_date::text date FROM money_transaction_snapshots WHERE company_id=? AND source_type='test_batch' ORDER BY source_id").bind(approver.companyId).all()).results).toEqual([{source_id:'batch-night-0',amount:'10700',usd:'297',date:'2026-11-01'},{source_id:'batch-night-1',amount:'10700',usd:'268',date:'2026-11-02'}]);
    await expect(snapshotMoneyBatch(db, otherCompany, inputs)).rejects.toMatchObject({code:'MANUAL_RATE_REQUIRED'});
    const c = await confirmManualRate(approver, {currency:'THB',effectiveDate:'2026-11-03',currencyPerUsd:'50',note:'Fresh next batch'}, 'money-batch-c');
    const fresh = await snapshotMoneyBatch(db, approver, [{...inputs[0],sourceId:'fresh-night',transactionDate:'2026-11-03'}]);
    expect(fresh.plans[0]).toMatchObject({rateId:c.id,usdUnits:'214'});
  });
  it("validates every member before persistence and rolls all quote rows back with their owner transaction", async () => {
    const input = {sourceType:'test_batch_rollback',sourceId:'valid',amountKind:'total',transactionDate:'2026-11-01',currency:'USD',currencyScale:2,amountUnits:'100'};
    await expect(snapshotMoneyBatch(db, approver, [input,{...input,sourceId:'invalid',amountUnits:'1.5'}])).rejects.toThrow();
    await expect(db.transaction(async tx => {const batch=await snapshotMoneyBatch(tx, approver, [input,{...input,sourceId:'second'}]);await tx.batch(batch.statements);throw new Error('owner rollback');})).rejects.toThrow('owner rollback');
    expect(await db.prepare("SELECT count(*)::int n FROM money_transaction_snapshots WHERE source_type='test_batch_rollback'").first()).toEqual({n:0});
  });
  it("chunks a large group quote below the parameter ceiling while retaining every independent row", async () => {
    const inputs = Array.from({length:501},(_,i)=>({sourceType:'test_large_batch',sourceId:'row-'+i,amountKind:'total',transactionDate:'2026-11-01',currency:'USD',currencyScale:2,amountUnits:String(i+1)}));
    const prepared=await snapshotMoneyBatch(db, approver, inputs);expect(prepared.statements).toHaveLength(2);expect(prepared.plans).toHaveLength(501);
    await db.batch(prepared.statements);
    expect(await db.prepare("SELECT count(*)::int n,sum(amount_units)::text amount,sum(usd_units)::text usd FROM money_transaction_snapshots WHERE company_id=? AND source_type='test_large_batch'").bind(approver.companyId).first()).toEqual({n:501,amount:'125751',usd:'125751'});
  });
});
