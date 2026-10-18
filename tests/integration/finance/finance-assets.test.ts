import {createProductWorkspace} from "../../../src/server/modules/identity_access/application/product-workspace";
import {productWorkspaceTarget} from "../../../src/server/modules/identity_access/domain/product-workspace";
import {workspaceGrantsForAccount,type AccountAccessContext} from "../../../src/server/modules/identity_access/public/authorization";
import {accountWorkModuleAccess} from "../../../src/server/modules/identity_access/public/work-module-access";
import {initializeWorkSelection,readWorkSelection,selectWorkRole} from "../../../src/server/modules/work_projection/public/work-role-store";
import {readCompanyPurchaseFacts} from "../../../src/server/modules/commercial/public/package-grants";
import {financeProductDocumentEntry} from "../../../src/server/modules/finance/public/document-purchases";
import {financeSourceDocumentBackdrop,financeDocumentPresentationAccess,financeDocumentFlow} from "../../../src/server/modules/finance/public/record-presentation";
import {ApiError} from "../../../src/server/modules/runtime/public/http";
import {journalDetail,ledgerWorkspace} from "../../../src/server/modules/finance/public/ledger";
import {journalRecordWorkspace} from "../../../src/server/modules/finance/public/record-presentation";
import {createJournal,createJournalSchema} from "../../../src/server/modules/finance/public/finance";
import {enrichFlowWork} from "../../../src/server/modules/workflow/public/flow-work";
import {resolveFlow} from "../../../src/server/modules/workflow/public/flow-routing";
import {withFinanceFlowAssignment} from "../../../src/server/modules/finance/public/document-purchases";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet,attachmentContent,linkExistingAttachment} from "../../../src/server/modules/files/public/attachments/service";
import {assetPage,assetOptions} from "../../../src/server/modules/finance/public/finance-assets";
import {createAccrualSchedule,accrualScheduleSchema} from "../../../src/server/modules/finance/public/finance-accruals";
import {afterEach} from "vitest";
import {reviewedFinanceGrant} from "../../support/helpers/finance-purchase-fixture";
import { beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { activateAsset,assetActivationSchema,assetDecisionSchema,assetDepreciationSchema,assetDetail,assetDisposalSchema,assetEditSchema,assetSchema,cancelDraftAsset,createAsset,decideAssetDisposal,depreciateAsset,editAsset,prepareAssetDisposal } from "../../../src/server/modules/finance/public/finance-assets";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,mapped:{cost:string;accumulated:string;expense:string;gain:string;loss:string};
beforeAll(async()=>{await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);});
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
  it("rolls back disposal journals carrying events and receipts after final outbox failure, then retries and replays once",async()=>{
    const a=await recognized(),assetId=String(a.id),prepared=await prepareAssetDisposal(f.reviewer,assetId,assetDisposalSchema.parse({expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",proceeds:"500",proceedsAccountId:f.accounts.cash,gainAccountId:mapped.gain,lossAccountId:mapped.loss,evidenceRef:"Retained disposal sale evidence"}),key()),disposalId=String(prepared.id),command=key(),decision={expectedVersion:1,reason:"Independent disposal carrying and actual proceeds review"},fault=`asset_disposal_fault_${crypto.randomUUID().replaceAll("-","")}`;
    const retained=await assetDetail(f.owner,assetId);
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'ASSET_DISPOSAL_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.asset.disposal.post') EXECUTE FUNCTION ${fault}();`);
    try{
      await expect(decideAssetDisposal(f.manager,disposalId,"post",decision,command)).rejects.toThrow();
      expect(await assetDetail(f.owner,assetId)).toEqual(retained);
      for(const table of ["command_receipts","audit_events","outbox_events","decision_evidence"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
      expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type='asset_disposal' AND source_id=?").bind(disposalId).first("n"))).toBe(0);
      expect(Number(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='asset_disposal' AND source_id=?").bind(f.company,disposalId).first("n"))).toBe(0);
      expect(Number(await db.prepare("SELECT SUM(debit_minor-credit_minor) n FROM finance_journal_lines WHERE account_id=?").bind(mapped.cost).first("n"))).toBe(120001);
    }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
    const posted=await decideAssetDisposal(f.manager,disposalId,"post",decision,command);expect(await decideAssetDisposal(f.manager,disposalId,"post",decision,command)).toEqual(posted);
    expect(posted).toMatchObject({status:"posted",version:2,assetVersion:3});
    for(const table of ["command_receipts","audit_events","outbox_events","decision_evidence"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
    expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type='asset_disposal' AND source_id=?").bind(disposalId).first("n"))).toBe(1);
    expect(Number(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='asset_disposal' AND source_id=?").bind(f.company,disposalId).first("n"))).toBe(1);
    expect(Number(await db.prepare("SELECT SUM(debit_minor-credit_minor) n FROM finance_journal_lines WHERE account_id=?").bind(mapped.cost).first("n"))).toBe(0);
    expect((await assetDetail(f.owner,assetId)).events.filter(e=>e.command_id===command)).toHaveLength(1);
  });

});

afterEach(()=>vi.unstubAllEnvs());

it("refuses unpurchased fixed asset data and creation while shadow retains the original owner workflow",async()=>{
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const command=key(),data=input();
 await expect(createAsset(f.reviewer,data,command)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(assetPage(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(assetOptions(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(Number(await db.prepare("SELECT COUNT(*) n FROM command_receipts WHERE command_id=?").bind(command).first("n"))).toBe(0);
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","shadow");const created=await createAsset(f.reviewer,data,command);expect((await assetDetail(f.owner,String(created.id))).asset.status).toBe("draft");
});
it("Assets-only recognition and disposal settle retained acquisitions while general Finance stays unavailable",async()=>{
 await reviewedFinanceGrant(db,f.company,"assets");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const data=input(),command=key(),created=await createAsset(f.reviewer,data,command),id=String(created.id);
 await reviewedFinanceGrant(db,f.company,"assets","draining");expect(await createAsset(f.reviewer,data,command)).toEqual(created);
 await expect(createAsset(f.reviewer,input(),key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});
 vi.stubEnv("ERPX_ATTACHMENTS_V2","true");vi.stubEnv("ERPX_ATTACHMENT_UPLOAD_FREEZE","false");
 const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64"),files=await createAttachmentSet(db,f.reviewer,{targetType:"finance_asset",targetId:id,componentKind:"record",purposeKey:"asset_support",expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(db,f.reviewer,files.id,{filename:"reviewed-acquisition.png",mediaType:"image/png",bytes:png.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,f.reviewer,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
 expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);await bindRecordAttachmentSet(db,f.reviewer,files.id,1,key());
 await activateAsset(f.manager,id,assetActivationSchema.parse({expectedVersion:2,periodId:f.period,entryDate:"2026-09-01",offsetAccountId:f.accounts.cash,evidenceRef:"Reviewed Assets-only source acquisition"}),key());
 await expect(depreciateAsset(f.reviewer,id,{expectedVersion:3,periodId:f.period,entryDate:"2026-09-30"},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(createAccrualSchedule(f.reviewer,accrualScheduleSchema.parse({description:"Cannot acquire GL from Assets",debitAccountId:mapped.expense,creditAccountId:f.accounts.payable,currency:"THB",total:"10",startDate:"2026-09-01",endDate:"2026-09-30",frequency:"monthly"}),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 const proposal=await prepareAssetDisposal(f.reviewer,id,assetDisposalSchema.parse({expectedVersion:3,periodId:f.period,entryDate:"2026-09-30",proceeds:"500",proceedsAccountId:f.accounts.cash,gainAccountId:mapped.gain,lossAccountId:mapped.loss,evidenceRef:"Retained Assets disposal evidence"}),key()),decision={expectedVersion:1,reason:"Independent Assets-only source carrying review"},postKey=key(),posted=await decideAssetDisposal(f.manager,String(proposal.id),"post",decision,postKey);
 expect(await decideAssetDisposal(f.manager,String(proposal.id),"post",decision,postKey)).toEqual(posted);expect((await assetDetail(f.owner,id)).asset).toMatchObject({status:"retired",cost_minor:120001,version:4});
 expect(Number(await db.prepare("SELECT SUM(debit_minor-credit_minor) n FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first("n"))).toBe(0);
 expect((await assetPage(f.owner)).some(row=>row.id===id)).toBe(true);
 const original=await attachmentContent(db,env.H001_FILES,f.reviewer,reserved.attachmentLinkId,"original");expect(Buffer.from(await original.object.arrayBuffer())).toEqual(png);
 const journalId=String((await assetDetail(f.owner,id)).asset.activation_journal_id),journalFiles=await createAttachmentSet(db,f.reviewer,{targetType:"finance_journal",targetId:journalId,componentKind:"record",purposeKey:"journal_support",expectedTargetVersion:1},key()),linked=await linkExistingAttachment(db,f.reviewer,journalFiles.id,{sourceLinkId:reserved.attachmentLinkId},key());await bindRecordAttachmentSet(db,f.reviewer,journalFiles.id,1,key());expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,f.reviewer,linked.attachmentLinkId,"original")).object.arrayBuffer())).toEqual(png);


});

it("late purchased disposal reviewer revocation rolls back all source carrying journals evidence and receipts before an exact retry",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const a=await recognized(),id=String(a.id);
 const prepared=await prepareAssetDisposal(f.reviewer,id,assetDisposalSchema.parse({expectedVersion:2,periodId:f.period,entryDate:"2026-09-30",proceeds:"500",proceedsAccountId:f.accounts.cash,gainAccountId:mapped.gain,lossAccountId:mapped.loss,evidenceRef:"Retained purchased asset disposal evidence"}),key()),decision={expectedVersion:1,reason:"Independent exact carrying value review"},command=key(),fault=`asset_purchase_${key().replaceAll("-","")}`;
 const snapshot=async()=>Promise.all(["finance_assets","finance_asset_disposals","finance_asset_events","finance_journals","finance_journal_lines","money_transaction_snapshots","command_receipts","audit_events","outbox_events","decision_evidence"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(decideAssetDisposal(f.manager,String(prepared.id),"post",decision,command)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await snapshot()).toEqual(before);expect(await db.prepare("SELECT revoked_at FROM assignments WHERE id=?").bind(f.manager.assignmentId).first("revoked_at")).toBeNull();}
 finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 const posted=await decideAssetDisposal(f.manager,String(prepared.id),"post",decision,command);expect(await decideAssetDisposal(f.manager,String(prepared.id),"post",decision,command)).toEqual(posted);expect((await assetDetail(f.owner,id)).asset.status).toBe("retired");
});

it("included Assets reads only its actual journal ownership and cannot open a label-forged GL or acquire generic Finance actions",async()=>{
 const asset=await recognized(),id=String(asset.id),journal=String((await assetDetail(f.owner,id)).asset.activation_journal_id),forged=await createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-01",currency:"THB",sourceType:"asset_acquisition",sourceId:id,memo:"Unowned label cannot become source authority",lines:[{accountId:mapped.cost,debit:"1",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"1"}]}),key());
 await reviewedFinanceGrant(db,f.company,"assets");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const detail=await journalRecordWorkspace(f.owner,journal);expect(detail.journal.id).toBe(journal);expect(detail.accounts.every(account=>account.chart_id===f.chart)).toBe(true);expect(detail).not.toHaveProperty("report");expect(detail).not.toHaveProperty("journals");await expect(journalDetail(f.owner,String(forged.id),db)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(ledgerWorkspace(f.owner,f.chart)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 const flow=await resolveFlow(db,f.owner,"journal",journal),item={id:journal,type:"journal" as const,title:"Owned acquisition source",detail:"Source history",href:`/finance/journals/${journal}`,status:"posted",updatedAt:1,queue:"todo" as const,canAct:true,flow:{...flow,canAct:true,canManage:true}},visible=await enrichFlowWork(f.owner,[item]);expect(visible).toMatchObject([{id:journal,canAct:false,actionKind:"work",flow:{canAct:false,canManage:false}}]);await expect(withFinanceFlowAssignment(f.owner,key(),async()=>{throw Error("Unpurchased route mutation reached effects");})).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
});

it("enters only an actual Assets journal through normal product role selection without a full Finance purchase or assignment changes",async()=>{
 const asset=await recognized(),id=String((await assetDetail(f.owner,String(asset.id))).asset.activation_journal_id),foreign=await createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-01",currency:"THB",sourceType:"asset_acquisition",sourceId:asset.id,memo:"Unowned source label",lines:[{accountId:mapped.cost,debit:"1",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"1"}]}),key());
 await reviewedFinanceGrant(db,f.company,"assets");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const salesId=key(),sessionId=key();await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,valid_from,valid_until,scope_kind) VALUES(?,?,'sales_operator',?,?,0,9999999999999,'holding')").bind(salesId,f.owner.userId,f.company,JSON.stringify(['sales'])).run();
 const account=async():Promise<AccountAccessContext>=>{const {rows,roles}=await workspaceGrantsForAccount(db,f.company,f.owner.userId);return {scope:f.owner.scope,sessionId,userId:f.owner.userId,name:f.owner.name,email:f.owner.email,expiresAt:Date.now()+600000,requestHeaders:new Headers(),assignments:rows.filter(row=>roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row=>({id:row.id,role:row.role,companyId:row.company_id,branchId:row.branch_id,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,modules:JSON.parse(row.modules_json),version:row.version,validUntil:row.valid_until}))};};
 const service=createProductWorkspace({access:account,modules:accountWorkModuleAccess,purchaseFacts:access=>readCompanyPurchaseFacts(db,access.scope.companyId),ready:async()=>true,financeDocumentEntry:(access,assignment,target)=>financeProductDocumentEntry(db,{companyId:f.company,branchId:null,departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),selection:(access,applicable)=>initializeWorkSelection(db,{sessionId,userId:access.userId,companyId:f.company,expiresAt:access.expiresAt},access.assignments,applicable),select:(access,option,revision)=>selectWorkRole(db,{sessionId,userId:access.userId,companyId:f.company,expiresAt:access.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 await selectWorkRole(db,{sessionId,userId:f.owner.userId,companyId:f.company,expiresAt:Date.now()+600000,assignmentId:salesId,assignmentVersion:1,revision:0});
 const snapshot=async()=>(await db.prepare("SELECT id,role,version,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results,before=await snapshot(),input=productWorkspaceTarget("finance",f.company,`/holdings/test/finance/accounting/journals/${id}/overview?chart=retained#history`)!;
 const state=await service.read("finance",input);expect(state.target).toBeNull();expect(state.options.map(option=>option.assignmentId)).not.toContain(salesId);expect(state.options.find(option=>option.assignmentId===f.owner.assignmentId)).toMatchObject({sourceRecord:true,purchasePurpose:"history",target:input.target});expect((await readWorkSelection(db,sessionId,f.owner.userId,f.company))?.assignment_id).toBe(salesId);
 await service.choose("finance",input,f.owner.assignmentId,state.selection!.revision,1);expect(await service.read("finance",input)).toMatchObject({target:input.target,sourceRecord:true});expect(await snapshot()).toEqual(before);
 expect(await financeSourceDocumentBackdrop(f.owner,"journal",id)).toMatchObject({id,status:"posted"});expect(await financeDocumentPresentationAccess(f.owner,"journal",id)).toEqual({sourceOnly:true,historyOnly:true});expect(await financeDocumentFlow(f.owner,"journal",id)).toMatchObject({canAct:false,canManage:false});
 await expect(service.read("finance",productWorkspaceTarget("finance",f.company,"/holdings/test/finance/accounting/journals")!)).rejects.toMatchObject({code:"PRODUCT_PACKAGE_REQUIRED"});await expect(service.read("finance",productWorkspaceTarget("finance",f.company,`/holdings/test/finance/accounting/journals/${foreign.id}/overview`)!)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});
 await reviewedFinanceGrant(db,f.company,"assets","draining");expect((await service.read("finance",input)).target).toBe(input.target);
 const scope={companyId:f.company,branchId:null,departmentId:null,userId:f.owner.userId,assignmentId:f.owner.assignmentId,assignmentVersion:1,role:f.owner.role};expect(await financeProductDocumentEntry(db,{...scope,assignmentVersion:2},{kind:"journal",id})).toBeNull();
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id})).toBeNull();await expect(service.read("finance",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});
});
it("keeps product source entry strict in every rollout mode while preserving Console and full Finance backgrounds",async()=>{
 const asset=await recognized(),id=String((await assetDetail(f.owner,String(asset.id))).asset.activation_journal_id);await reviewedFinanceGrant(db,f.company,"assets");
 const scope={companyId:f.company,branchId:null,departmentId:null,userId:f.owner.userId,assignmentId:f.owner.assignmentId,assignmentVersion:1,role:f.owner.role};
 // Product entry always checks actual source history; legacy Console/API rollout remains independent.
 for(const mode of ["disabled","shadow"]){vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT",mode);expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id})).toMatchObject({capability:"finance.asset-recognition",purchasePurpose:"history"});expect(await financeSourceDocumentBackdrop(f.owner,"journal",id)).toBeNull();expect(await financeSourceDocumentBackdrop(f.owner,"journal",id,{strictProduct:true})).toMatchObject({id,status:"posted"});}
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id})).toMatchObject({capability:"finance.asset-recognition",purchasePurpose:"history"});await reviewedFinanceGrant(db,f.company,"finance");expect(await financeSourceDocumentBackdrop(f.owner,"journal",id)).toBeNull();expect(await financeDocumentPresentationAccess(f.owner,"journal",id)).toEqual({sourceOnly:false,historyOnly:false});
});
