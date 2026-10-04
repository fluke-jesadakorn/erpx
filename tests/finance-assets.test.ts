import { beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { activateAsset,assetActivationSchema,assetDecisionSchema,assetDepreciationSchema,assetDetail,assetDisposalSchema,assetEditSchema,assetSchema,cancelDraftAsset,createAsset,decideAssetDisposal,depreciateAsset,editAsset,prepareAssetDisposal } from "../src/server/finance-assets";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,mapped:{cost:string;accumulated:string;expense:string;gain:string;loss:string};
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();mapped={cost:`asset-cost-${key()}`,accumulated:`asset-accum-${key()}`,expense:`asset-expense-${key()}`,gain:`asset-gain-${key()}`,loss:`asset-loss-${key()}`};
  for(const [name,id] of Object.entries(mapped))await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(id,f.chart,f.company,`${name}-${key()}`,name,["cost","accumulated"].includes(name)?"asset":name==="gain"?"other_income":"expense").run();
});
function input(cost="1200.01",life=3){return assetSchema.parse({assetNumber:`FA-${key()}`,name:"Synthetic oven",accountId:mapped.cost,depreciationAccountId:mapped.accumulated,depreciationExpenseAccountId:mapped.expense,currency:"THB",cost,residual:"0",usefulLifeMonths:life,inServiceDate:"2026-09-01"});}
async function recognized(){
  const created=await createAsset(f.reviewer,input(),key());
  const activated=await activateAsset(f.manager,String(created.id),assetActivationSchema.parse({expectedVersion:1,periodId:f.period,entryDate:"2026-09-01",offsetAccountId:f.accounts.cash,evidenceRef:"Synthetic acquisition invoice confirmed independently"}),key());return activated;
}
describe("atomic Finance fixed asset lifecycle",()=>{
  it("recognizes cost then posts consecutive straight-line months to expense and accumulated depreciation with exact residual rounding",async()=>{
    const a=await recognized(),assetId=String(a.id),firstInput=assetDepreciationSchema.parse({expectedVersion:2,periodId:f.period,entryDate:"2026-09-30"}),firstKey=key(),first=await depreciateAsset(f.reviewer,assetId,firstInput,firstKey);
    expect(first.amount).toBe("400.00");expect(await depreciateAsset(f.reviewer,assetId,firstInput,firstKey)).toEqual(first);
    await expect(depreciateAsset(f.reviewer,assetId,{...firstInput,expectedVersion:3},key())).rejects.toMatchObject({code:"ASSET_DEPRECIATION_SEQUENCE"});
    const october=`asset-oct-${key()}`,november=`asset-nov-${key()}`;
    await db.batch([db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES(?,?,'Oct 2026','2026-10-01','2026-10-31','open',1)").bind(october,f.company),db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES(?,?,'Nov 2026','2026-11-01','2026-11-30','open',1)").bind(november,f.company)]);
    const second=await depreciateAsset(f.reviewer,assetId,{expectedVersion:3,periodId:october,entryDate:"2026-10-31"},key()),third=await depreciateAsset(f.reviewer,assetId,{expectedVersion:4,periodId:november,entryDate:"2026-11-30"},key());
    expect([first.amount,second.amount,third.amount]).toEqual(["400.00","400.00","400.01"]);
    const fresh=await assetDetail(f.owner,assetId);expect(fresh.asset).toMatchObject({status:"in_service",cost_minor:120001,accumulated_depreciation_minor:120001,version:5});expect(fresh.depreciations).toHaveLength(3);
    expect(await db.prepare("SELECT account_id,SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id IN (SELECT journal_id FROM finance_asset_depreciations WHERE asset_id=?) GROUP BY account_id ORDER BY account_id").bind(assetId).all()).toMatchObject({results:expect.arrayContaining([{account_id:mapped.expense,debit:120001,credit:0},{account_id:mapped.accumulated,debit:0,credit:120001}])});
    expect(await db.prepare("SELECT SUM(debit_minor-credit_minor) net FROM finance_journal_lines WHERE account_id=?").bind(mapped.cost).first("net")).toBe(120001);
    await expect(db.prepare("UPDATE finance_assets SET cost_minor=1 WHERE id=?").bind(assetId).run()).rejects.toThrow("ACTIVATED_ASSET_SOURCE_IMMUTABLE");
    await expect(db.prepare("DELETE FROM finance_asset_depreciations WHERE asset_id=?").bind(assetId).run()).rejects.toThrow("ASSET_EVIDENCE_IMMUTABLE");
  });
  it("posts reviewed disposal gain or loss, reverses asset and accumulated balances, and retains the immutable receipt after reload",async()=>{
    const a=await recognized(),assetId=String(a.id);await depreciateAsset(f.reviewer,assetId,{expectedVersion:2,periodId:f.period,entryDate:"2026-09-20"},key());
    const prepared=await prepareAssetDisposal(f.reviewer,assetId,assetDisposalSchema.parse({expectedVersion:3,periodId:f.period,entryDate:"2026-09-30",proceeds:"500",proceedsAccountId:f.accounts.cash,gainAccountId:mapped.gain,lossAccountId:mapped.loss,evidenceRef:"Synthetic disposal sale and collection evidence"}),key()),disposalId=String(prepared.id);
    const preparerWithApproval={...f.reviewer,role:"finance_approver",assignmentId:await f.grant(f.reviewer,"finance_approver")};
    await expect(decideAssetDisposal(preparerWithApproval,disposalId,"post",{expectedVersion:1,reason:"I prepared this disposal"},key())).rejects.toMatchObject({code:"SELF_APPROVAL"});
    const decision={expectedVersion:1,reason:"Independent Finance reviewer checked proceeds and carrying value"},postKey=key(),posted=await decideAssetDisposal(f.manager,disposalId,"post",decision,postKey);expect(await decideAssetDisposal(f.manager,disposalId,"post",decision,postKey)).toEqual(posted);
    expect(await db.prepare("SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).all()).toMatchObject({results:expect.arrayContaining([{account_id:mapped.cost,debit_minor:0,credit_minor:120001},{account_id:mapped.accumulated,debit_minor:40000,credit_minor:0},{account_id:f.accounts.cash,debit_minor:50000,credit_minor:0},{account_id:mapped.loss,debit_minor:30001,credit_minor:0}])});
    const fresh=await assetDetail(f.owner,assetId);expect(fresh.asset).toMatchObject({status:"retired",version:4});expect(fresh.disposals).toMatchObject([{id:disposalId,status:"posted",journal_id:posted.journalId}]);expect(fresh.events.map(e=>e.event_type)).toEqual(["created","activated","depreciated","disposal_prepared","disposed"]);
    await expect(depreciateAsset(f.reviewer,assetId,{expectedVersion:4,periodId:f.period,entryDate:"2026-09-30"},key())).rejects.toMatchObject({code:"ASSET_NOT_RECOGNIZED"});
    await expect(db.prepare("UPDATE finance_asset_disposals SET proceeds_minor=0 WHERE id=?").bind(disposalId).run()).rejects.toThrow("ASSET_DISPOSAL_REVISION_IMMUTABLE");
    const other=await expenseFixture();await expect(assetDetail(other.owner,assetId)).rejects.toMatchObject({code:"ASSET_NOT_FOUND"});
  });
  it("guards stale versions, cancels drafts and disposal proposals, and rolls back journal writes when the posting period closes",async()=>{
    const data=input(),created=await createAsset(f.reviewer,data,key()),id=String(created.id);
    const edited=await editAsset(f.reviewer,id,assetEditSchema.parse({...data,name:"Revised oven",expectedVersion:1,reason:"Corrected before recognition"}),key());expect(edited.version).toBe(2);
    await expect(cancelDraftAsset(f.reviewer,id,{expectedVersion:1,reason:"Stale request"},key())).rejects.toMatchObject({code:"ASSET_CHANGED"});
    const cancelled=await cancelDraftAsset(f.reviewer,id,assetDecisionSchema.parse({expectedVersion:2,reason:"Cancelled duplicate acquisition entry"}),key());expect(cancelled.cancelled).toBe(true);expect((await assetDetail(f.owner,id)).events.map(e=>e.event_type)).toEqual(["created","edited","cancelled"]);
    const a=await recognized(),assetId=String(a.id),proposal=await prepareAssetDisposal(f.reviewer,assetId,assetDisposalSchema.parse({expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",gainAccountId:mapped.gain,lossAccountId:mapped.loss,evidenceRef:"Synthetic scrapping proposal"}),key());
    await expect(depreciateAsset(f.reviewer,assetId,{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30"},key())).rejects.toMatchObject({code:"ASSET_DISPOSAL_PENDING"});
    await decideAssetDisposal(f.reviewer,String(proposal.id),"cancel",{expectedVersion:1,reason:"Continue operating the oven"},key());
    await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();
    await expect(depreciateAsset(f.reviewer,assetId,{expectedVersion:2,periodId:f.period,entryDate:"2026-09-30"},key())).rejects.toMatchObject({code:"PERIOD_CLOSED"});
    expect((await assetDetail(f.owner,assetId)).asset).toMatchObject({version:2,accumulated_depreciation_minor:0,status:"in_service"});expect(await db.prepare("SELECT COUNT(*) n FROM finance_asset_depreciations WHERE asset_id=?").bind(assetId).first("n")).toBe(0);
  });
});
