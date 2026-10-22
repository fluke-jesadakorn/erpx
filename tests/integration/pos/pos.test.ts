import {financeProductDocumentEntry} from "@erpx/server-finance/public/document-purchases";
import {financeSourceDocumentBackdrop} from "@erpx/server-finance/public/record-presentation";
import {getInvoice,getPayment} from "@erpx/server-finance/public/finance";
import {journalDetail} from "@erpx/server-finance/public/ledger";
import { reviewedSecondaryBook } from "../../support/helpers/finance-book-fixture";
import { canonicalGuestAccount, legacyGuestAccount } from "@erpx/feature-identity_access/lib/guest-account-compat";
import { beforeAll,beforeEach,afterEach,describe,it,expect,vi } from 'vitest';
import { createHash, createHmac, randomUUID } from 'node:crypto';
import { packageGrantStore, readCompanyPackageGrants, packageGrantRevision } from "@erpx/server-commercial/public/package-grants";
import { withOwnerPackageObligations, readOwnerPackageObligations, ownerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { posPackageObligations, lockPosPackageObligationSources } from "@erpx/server-pos/public/package-obligations";
import { importBankStatement } from "@erpx/server-finance/public/finance";
import { setFinanceCompanyTool } from "@erpx/server-finance/public/finance-company-tools";
import { bankAutomationWorkspace,proposeBankMatchGroup,decideBankMatchGroup } from "@erpx/server-finance/public/finance-bank-automation";
import { env,applyMigrations } from "../../support/runtime-env";
import {posWorkspace,posReportReceipts,posWork} from "@erpx/server-pos/public/pos";
import {posProductImage,uploadPosProductImage} from "@erpx/server-pos/public/pos-images";
import {readableRecordSourceIds} from "@erpx/server-pos/public/records-source-facts";
import {visibleSourceRecord} from "@erpx/server-records/public/source-record-policy";
import {readExecutiveMetricProjection as posMetric} from "@erpx/server-pos/public/executive-metric-projections";
import {executiveEvidenceSources as posMetricEvidence} from "@erpx/server-pos/public/executive-evidence-sources";
import {metricCatalogFor} from "@erpx/server-runtime/public/executive/metrics";
import { posCommand,one,all,run } from "@erpx/server-pos/public/pos-core";
import { accountKeys,saleAmounts,portion,recipeQuantities,formatPosMoney,businessDate } from "@erpx/server-pos/public/rules/pos";
import { defaultConfig,pmsAccountKeys } from "@erpx/server-pms/public/rules/pms";
import { drainPos,posStripeWebhook,verifyPosStripeSignature } from "@erpx/server-pos/public/pos-stripe";
import type { PermissionActor } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {posCustomers,posDayReport} from "@erpx/server-pos/public/pos-daily";
import { attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet } from "../../support/helpers/attachment-v2";
vi.mock("@erpx/server-runtime/adapters/runtime",async()=>{const {env}=await import("../../support/runtime-env");return {runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB})};});
const db=env.H001_DB;
afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();vi.unstubAllEnvs();});
const actor={userId:'cashier',assignmentId:'pos-cashier',role:'holding_owner',companyId:'pos-company',branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'pos-test',name:'Cashier',email:'cashier@example.test',assignmentScope:'holding',scope:{kind:'holding',holdingId:'h001',holdingSlug:'pos-test',holdingName:'POS',companyId:'pos-company'},modules:['pos'],assignments:[],workspaces:[],inherited:false,holdingName:'POS'} as WorkContext;
const manager={...actor,userId:'manager',assignmentId:'pos-manager'};
const hotelManager={...actor,userId:'hotel-manager',assignmentId:'hotel-manager'};
let serial=0;
const command=(op:string,input:unknown,who=actor,key=`pos-test-command-${++serial}`)=>posCommand(db,who,op,input,key);
async function change(op:string,saleId:string,extra:Record<string,unknown>={},who=actor){const row=await one(db,'SELECT * FROM pos_sales WHERE id=?',saleId);return command(op,{id:saleId,expectedVersion:row!.version,...extra},who);}
async function createSale(mode='retail'){
 const sale=await command('sale.create',{shiftId:mode==='retail'?'shift-retail':'shift-kitchen',serviceMode:mode==='retail'?'retail':'dine_in'});return String(sale.id);
}
async function add(sale:string,product='tea',qty=1000){await change('sale.add',sale,{productId:product,quantityMilli:qty,modifierIds:[],discountMinor:0,note:''});}
async function pay(sale:string,amount=10700){await change('payment.create',sale,{method:'cash',amountMinor:amount,tenderedMinor:amount,reference:''});}
async function kitchen(sale:string,action:string){const f=(await one(db,'SELECT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?',sale))!;return command(`kitchen.${action}`,{id:f.id,expectedVersion:f.version,reason:'customer changed order'},action==='cancel'?manager:actor);}
async function expectLatePosAuditRollback(operation:string,request:unknown,who:WorkContext,key:string){
 const tables=['pos_room_charge_reversal_requests','pos_sales','pos_lines','pos_reservations','pos_stock_moves','pos_fulfillments','pos_payments','pos_postings','pos_receipts','pos_audit','pos_jobs','pms_properties','pms_records','pms_sources','pms_postings','finance_journals','finance_journal_lines','finance_invoices','finance_payments','finance_allocations','money_transaction_snapshots','commerce_stock','attachment_sets','attachment_links','attachment_events','command_assertions'];
 const snapshot=async()=>Promise.all(tables.map(table=>all(db,`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`)));
 const before=await snapshot();
 await db.exec("CREATE FUNCTION pos_final_audit_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation=TG_ARGV[0] THEN RAISE EXCEPTION 'forced late POS audit failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$");
 await db.exec(`CREATE TRIGGER pos_final_audit_fault AFTER INSERT ON pos_audit FOR EACH ROW EXECUTE FUNCTION pos_final_audit_fault('${operation}')`);
 try{await expect(command(operation,request,who,key)).rejects.toMatchObject({code:'POS_CONFLICT'});expect(await snapshot()).toEqual(before);expect(await one(db,'SELECT id FROM pos_receipts WHERE id=?',key)).toBeNull();}
 finally{await db.exec('DROP TRIGGER pos_final_audit_fault ON pos_audit; DROP FUNCTION pos_final_audit_fault()');}
}
async function clearPosSetup(){await run(db,'DELETE FROM pos_shifts');await run(db,'DELETE FROM pos_tables');await run(db,'DELETE FROM pos_registers');await run(db,'DELETE FROM pos_settings');}
async function packageGrant(packageId="pos",state:"active"|"draining"="active",endsAt:number|null=null){
 const current=(await readCompanyPackageGrants(db,actor.companyId)).find(grant=>grant.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:"isolated-pos-package-fixture"}),obligations:async()=>{throw Error("No test settlement-withdrawal witness");}}).apply({
  expectedVersion:current?.version??0,idempotencyKey:randomUUID(),grant:{companyId:actor.companyId,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(current?.version??0)+1,
   reason:"Native POS package-only command review",evidence:"test:pos-package-command"}});
}
beforeAll(async()=>{await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);enableAttachmentV2ForTest();},120000);
beforeEach(async context=>{
 await db.exec(`DELETE FROM command_assertions;
 TRUNCATE company_package_review_evidence,company_package_change_sets,company_package_history_rights,company_package_grant_events,company_package_grants,company_package_revisions CASCADE;
 TRUNCATE money_transaction_snapshots,company_fx_rates,pos_day_closes,pos_sale_documents,pos_product_images,pos_register_catalog,hospitality_tax_documents,company_feature_rollouts,pms_sources,pms_postings,pms_records,pms_properties,pos_receipts,pos_audit,pos_jobs,pos_stripe_events,pos_settlements,pos_oauth_states,pos_connections,pos_connection_history,pos_cash_movements,pos_refund_payments,pos_refunds,pos_payments,pos_lines,pos_reservations,pos_stock_moves,pos_fulfillments,pos_sales,pos_tables,pos_shifts,pos_registers,pos_catalog,pos_settings,pos_postings,finance_bank_transactions,finance_bank_statements,finance_allocations,finance_invoices,finance_payments,finance_journal_lines,finance_journals,finance_parties,commerce_stock,commerce_products,commerce_warehouses,commerce_settings,finance_accounts,finance_periods,finance_charts,assignments CASCADE;
 INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('pos-company','company','pos-company','POS test','pos-company') ON CONFLICT DO NOTHING;
 INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,parent_id,branch_id) VALUES('pos-branch','branch','pos-branch','Branch','pos-company','pos-company','pos-branch') ON CONFLICT DO NOTHING;
 INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES('pos-cashier','cashier','holding_owner','["pos"]','pos-company','holding',0,9999999999999),('pos-manager','manager','holding_owner','["pos"]','pos-company','holding',0,9999999999999),('hotel-manager','hotel-manager','holding_owner','["pos","pms"]','pos-company','holding',0,9999999999999);
 INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','pos-company','POS test','THB','system',0,0);
 INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('period','pos-company','Open','2020-01-01','2099-12-31',0);
 INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','pos-company','pos-branch','POS','POS warehouse');
 INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES('tea','pos-company','TEA','Tea','each',10000,700),('milk','pos-company','MILK','Milk','litre',5000,700),('latte','pos-company','LATTE','Latte','cup',12000,700);
 INSERT INTO commerce_stock VALUES('warehouse','tea',10000,30000,1),('warehouse','milk',10000,50000,1);
 INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode) VALUES('retail','pos-company','pos-branch','warehouse','Shop','retail'),('restaurant','pos-company','pos-branch','warehouse','Cafe','restaurant');
 INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor) VALUES('shift-retail','retail','pos-company','pos-branch','cashier',0,10000),('shift-kitchen','restaurant','pos-company','pos-branch','cashier',0,10000);
 `);
 for(const [i,k] of accountKeys.entries())await run(db,'INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES(?,?,?,?,?,?,0,0)',k,'chart','pos-company',String(1000+i),k,k==='revenue'?'revenue':['advance','refund','outputTax'].includes(k)?'liability':['cogs','fees','waste','cashDifference'].includes(k)?'expense':'asset');
 await run(db,'INSERT INTO commerce_settings VALUES(?,?,?,?,?,?,?,?,?,?,?,?)','pos-company','chart','THB','receivable','refund','revenue','inventory','advance','cogs','outputTax','outputTax','cash');
 await command('settings.save',{enabled:true,currency:'THB',timezone:'Asia/Bangkok',cashierDiscountBps:0,accounts:{chartId:'chart',...Object.fromEntries(accountKeys.map(k=>[k,k]))},expectedVersion:0});
 await command('accounting.approve',{expectedVersion:1},manager);
 await command('catalog.save',{productId:'latte',barcode:'8850001',category:'Coffee',kind:'recipe',station:'Bar',recipe:[{productId:'milk',quantityMilli:200}],modifiers:[],taxInclusive:false,expectedVersion:0});
 await command('catalog.save',{productId:'tea',barcode:'8850002',category:'Retail',kind:'stock',station:'Shop',recipe:[],modifiers:[],taxInclusive:false,registerIds:['retail','restaurant'],expectedVersion:0});
 await run(db,"DELETE FROM company_fx_rates WHERE company_id='pos-company'");
 if(!context.task.name.startsWith("does not silently open"))await run(db,"INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)",`pos-fixture-rate-${serial}`,'pos-company','THB','2020-01-01','36','manager',Date.now(),'Declared synthetic fixture quote; no provider or market observation.');
});
describe('POS committed business flows',()=>{
 const settlementInput=()=>({companyId:actor.companyId,packageId:'pos',packageVersion:1});
 const settlementStore=()=>packageGrantStore(db,{authorize:async()=>({userId:'isolated-pos-settlement-provider'}),obligations:(tx,company,packageId)=>ownerPackageObligations(db,tx,company,packageId)});
 const settlement=()=>withOwnerPackageObligations(db,settlementInput(),tx=>readOwnerPackageObligations(db,tx,{...settlementInput(),now:Date.now()}));
 async function inactiveBatch(){const grants=await readCompanyPackageGrants(db,actor.companyId),current=grants.find(grant=>grant.packageId==='pos')!;return {companyId:actor.companyId,expectedRevision:packageGrantRevision(actor.companyId,grants),idempotencyKey:randomUUID(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Native POS owner settlement reviewed',evidence:'test:pos-concrete-settlement'}}]};}
 const previewInactive=async(request:Awaited<ReturnType<typeof inactiveBatch>>,apply=false)=>withOwnerPackageObligations(db,settlementInput(),tx=>apply?settlementStore().batch.applyIn(tx,request,async()=>{}):settlementStore().batch.previewIn(tx,request));
 async function closePosDay(){
  for(const id of ['shift-retail','shift-kitchen']){
   let shift=(await one(db,'SELECT * FROM pos_shifts WHERE id=?',id))!;
   if(shift.state==='open'){const cash=(await one(db,"SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=? AND p.method='cash' AND p.state='succeeded'",id))!;await command('shift.close',{id,expectedVersion:shift.version,countedMinor:10000+Number(cash.n)});shift=(await one(db,'SELECT * FROM pos_shifts WHERE id=?',id))!;}
   if(shift.state==='closed')await command('shift.review',{id,expectedVersion:shift.version,reason:'Independent settlement close review'},manager);
  }
  const close=await command('day.prepare',{branchId:'pos-branch',businessDate:businessDate()},manager);await command('day.confirm',{id:close.id,expectedVersion:1});
 }
 it('package POS settlement reads all three actual owners, completes normal close and preserves inactive history',async()=>{
  const secondary=await reviewedSecondaryBook(db,actor,manager,'chart','period',businessDate());
  await command('shift.close',{id:'shift-kitchen',expectedVersion:1,countedMinor:10000});await command('shift.review',{id:'shift-kitchen',expectedVersion:2,reason:'Close unused historical Restaurant fixture'},manager);
  await packageGrant();vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const sale=await createSale();await add(sale);await pay(sale);
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_book_postings p JOIN finance_journals j ON j.id=p.secondary_journal_id WHERE p.company_id=? AND j.chart_id=?').bind(actor.companyId,secondary.chartId).first('n'))).toBeGreaterThan(0);
  const open=await settlement();expect(open.observations.map(row=>row.moduleId)).toEqual(['pos','finance','inventory']);expect(open.observations.every(row=>row.outstanding>0)).toBe(true);
  expect(JSON.stringify(open)).not.toMatch(/Cashier|Tea|cashier@example|accounts_json|userId/);
  await packageGrant('pos','draining');const request=await inactiveBatch();await expect(previewInactive(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(previewInactive(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
  await change('sale.complete',sale);const paid=await settlement();expect(paid.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(0);expect(paid.observations.find(row=>row.moduleId==='inventory')?.outstanding).toBe(0);expect(paid.total).toBeGreaterThan(0);
  await closePosDay();expect((await settlement()).total).toBe(0);
  const tables=['pos_sales','pos_sale_documents','pos_payments','pos_postings','pos_stock_moves','commerce_stock','finance_journals','finance_journal_lines','finance_payments','finance_invoices'];
  const history=()=>Promise.all(tables.map(table=>all(db,`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`))),before=await history();
  expect((await previewInactive(request)).after.find(row=>row.packageId==='pos')?.state).toBe('inactive');
  const applied=await previewInactive(request,true);expect((await readCompanyPackageGrants(db,actor.companyId))[0]).toMatchObject({state:'inactive',version:3});expect(await history()).toEqual(before);
  expect(await previewInactive(request,true)).toEqual(applied);expect((await settlement()).total).toBe(0);
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');expect((await posWorkspace(actor,sale,'','history')).purchases?.['pos.sales']).toMatchObject({newWork:false,settleExisting:false,history:true});
  await expect(createSale()).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
 });
 it('package POS settlement refuses pooled, forged, wrong-Company and pre-existing source snapshots',async()=>{
  const input={...settlementInput(),moduleId:'pos',now:Date.now()};await expect(posPackageObligations(db,db,input)).rejects.toThrow('TRANSACTION_NOT_ACTIVE_OR_OWNED');
  await expect(lockPosPackageObligationSources(db,{} as Parameters<typeof lockPosPackageObligationSources>[1])).rejects.toThrow('PACKAGE_SETTLEMENT_SOURCE_LOCK_REQUIRED');
  await db.transaction(async tx=>{await one(tx,'SELECT count(*) n FROM pos_sales');await expect(posPackageObligations(db,tx,input)).rejects.toThrow('POS_PACKAGE_OBLIGATION_SOURCE_REQUIRED');});
  await withOwnerPackageObligations(db,settlementInput(),async tx=>{await expect(posPackageObligations(db,tx,{...input,companyId:'other-company'})).rejects.toThrow('POS_PACKAGE_OBLIGATION_SOURCE_REQUIRED');});
  expect((await withOwnerPackageObligations(db,{...settlementInput(),companyId:'other-company'},tx=>readOwnerPackageObligations(db,tx,{...input,companyId:'other-company'}))).total).toBe(0);
 });
 it('package POS settlement rolls a deferred new callback obligation and inactivation back together',async()=>{
  await packageGrant();const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);await closePosDay();await packageGrant('pos','draining');expect((await settlement()).total).toBe(0);
  const request=await inactiveBatch(),job=randomUUID(),trigger=`pos_withdrawal_${randomUUID().replaceAll('-','')}`;
  await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('${job}','pos-company','event','late-provider-event',0);RETURN NEW;END $$;
   CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.company_id='pos-company') EXECUTE FUNCTION ${trigger}()`);
  try{await expect(previewInactive(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await one(db,'SELECT id FROM pos_jobs WHERE id=?',job)).toBeNull();expect(await one(db,'SELECT id FROM company_package_change_sets WHERE id=?',request.idempotencyKey)).toBeNull();expect((await readCompanyPackageGrants(db,actor.companyId))[0]).toMatchObject({state:'draining',version:2});}
  finally{await db.exec(`DROP TRIGGER ${trigger} ON company_package_change_sets;DROP FUNCTION ${trigger}()`);}
 });
 it('package POS settlement fences absent POS, Finance and Stock rows before the snapshot',async()=>{
  await withOwnerPackageObligations(db,settlementInput(),async tx=>{
   expect((await readOwnerPackageObligations(db,tx,{...settlementInput(),now:Date.now()})).observations).toHaveLength(3);
   for(const [table,column] of [['company_package_grants','version'],['pos_sales','version'],['pos_refunds','version'],['pos_reservations','quantity_milli'],['pos_jobs','attempts'],['pos_stripe_events','received_at'],['pos_settings','version'],['pos_day_closes','version'],['pos_connections','version'],['finance_invoices','version'],['finance_payments','version'],['finance_journals','version'],['finance_journal_lines','ordinal'],['commerce_stock','version'],['inventory_counts','version'],['inventory_transfers','version'],['inventory_transit','version'],['inventory_landed_costs','version']]){
    await expect(db.transaction(async competing=>{await competing.prepare("SET LOCAL lock_timeout='100ms'").run();await competing.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();})).rejects.toMatchObject({code:'55P03'});
   }
  });
 });
 it('package POS metrics and evidence keep purchased history and reject a revoked cached assignment',async()=>{
  const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);
  const date=businessDate(),filters={from:date,to:date,currency:'THB'},evidence=posMetricEvidence(db);
  const read=()=>posMetric(db,actor,'pos-sales',filters,'status'),search=()=>evidence.searchRows0(actor,{metric:'pos-sales',filters,query:'',limit:25},null);
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
  expect((await metricCatalogFor(db,actor)).find(row=>row.id==='pos-sales')?.available).toBe(false);
  await expect(read()).rejects.toMatchObject({status:403,code:'METRIC_SOURCE_UNAVAILABLE'});expect(await search()).toEqual([]);
  await packageGrant();expect((await metricCatalogFor(db,actor)).find(row=>row.id==='pos-sales')?.available).toBe(true);
  const snapshot=await read();expect(snapshot.raw).toEqual([{label:'completed',value:'10700'}]);expect((await search()).map(row=>row.sale_id)).toEqual([sale]);
  await packageGrant('pos','draining');expect((await read()).raw).toEqual(snapshot.raw);expect((await search()).map(row=>row.sale_id)).toEqual([sale]);
  await run(db,"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='pos-cashier'");
  await expect(read()).rejects.toMatchObject({code:'METRIC_SOURCE_UNAVAILABLE'});expect(await search()).toEqual([]);
  await expect(snapshot.source({label:'completed',page:1})).rejects.toMatchObject({code:'METRIC_SOURCE_UNAVAILABLE'});
 });
 it('package POS source identifiers match record visibility without granting cached or revoked authority',async()=>{
  const sale=await createSale(),row=(await one(db,'SELECT version FROM pos_sales WHERE id=?',sale))!;
  const ids=async()=>{const projection=await readableRecordSourceIds(actor);return(await db.prepare(projection.sql).bind(...projection.bindings).all<{id:string}>()).results.map(item=>item.id);};
  const visible=()=>visibleSourceRecord(actor,{kind:'pos_sale',id:sale,version:Number(row.version)},db);
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');expect(await visible()).toBeNull();expect(await ids()).not.toContain(sale);
  await packageGrant();expect(await visible()).toMatchObject({id:sale,kind:'pos_sale'});expect(await ids()).toContain(sale);
  await packageGrant('pos','draining');expect(await visible()).toMatchObject({id:sale});expect(await ids()).toContain(sale);
  await run(db,"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='pos-cashier'");expect(await visible()).toBeNull();expect(await ids()).not.toContain(sale);
 });
 it('package enforcement refuses an unpurchased sale and shadow mode observes without granting extra authority',async()=>{
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
  await expect(createSale()).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(posWorkspace(actor,undefined,'','sale')).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(posCustomers(db,actor,'private customer')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(posDayReport(db,manager,'pos-branch',businessDate())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(posProductImage(actor,'private-image')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(uploadPosProductImage(new Request('https://pos-dev-erpx.jesadakorn.com/api/images/upload',{method:'POST',body:'not-read'}),actor)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  expect(await posWork(manager)).toEqual([]);
  expect(await one(db,'SELECT count(*) n FROM pos_sales')).toEqual({n:0});
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','shadow');const telemetry=vi.spyOn(console,'info').mockImplementation(()=>{});
  try{
   const sale=await createSale();expect(await one(db,'SELECT id FROM pos_sales WHERE id=?',sale)).toEqual({id:sale});
   expect(telemetry.mock.calls.some(args=>String(args[0]).includes('commercial.purchase.shadow')&&String(args[0]).includes('PACKAGE_REQUIRED'))).toBe(true);
   await run(db,"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='pos-cashier'");
   await expect(createSale()).rejects.toMatchObject({code:'POS_ACCESS_CHANGED'});
  }finally{telemetry.mockRestore();}
 });
 it('package-only POS posts stock and Finance, drains refunds and retains receipt replay under current authority',async()=>{
  await packageGrant();vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  const input={shiftId:'shift-retail',serviceMode:'retail'},key='pos-package-original-sale';
  const opened=await command('sale.create',input,actor,key),sale=String(opened.id);await add(sale);await pay(sale);await change('sale.complete',sale);
  const active=await posWorkspace(actor,sale,'','history');expect(active.purchases?.['finance.pos-posting'].newWork).toBe(true);expect(active.purchases?.['finance.accounting'].history).toBe(false);expect(active.roomChargeEnabled).toBe(false);
  expect((await readCompanyPackageGrants(db,actor.companyId)).map(grant=>grant.packageId)).toEqual(['pos']);
  expect((await all(db,"SELECT split_part(source_key,':',1) kind FROM pos_postings ORDER BY kind")).map(row=>row.kind)).toEqual(['allocate','consume','receipt','sale']);
  expect(await one(db,"SELECT SUM(l.base_debit_minor-l.base_credit_minor) net FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.status='posted'")).toEqual({net:0});
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:9000});
  await packageGrant('pos','draining');
  expect((await posWorkspace(actor,sale,'','history')).purchases?.['pos.sales']).toMatchObject({newWork:false,settleExisting:true,history:true});
  await expect(createSale()).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'Original sale returned while package drains',payoutShiftId:'shift-retail',lines:[{lineId:line.id,quantityMilli:1000,restock:true}]});
  await command('refund.approve',{id:refund.id,expectedVersion:1,reason:'Independent return review during draining'},manager);
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:10000});
  const invoiceId=String((await one(db,'SELECT invoice_id FROM pos_sales WHERE id=?',sale))!.invoice_id);expect((await getInvoice(actor,invoiceId,db)).invoice.id).toBe(invoiceId);
  for(const row of await all(db,'SELECT journal_id FROM pos_postings WHERE company_id=?',actor.companyId))expect((await journalDetail(actor,String(row.journal_id),db)).journal.id).toBe(row.journal_id);
  for(const row of await all(db,'SELECT id FROM finance_payments WHERE company_id=?',actor.companyId)){const detail=await getPayment(actor,String(row.id));expect(detail.payment.id).toBe(row.id);expect(detail.bankTransactions).toEqual([]);expect(detail.invoices).toEqual([]);}
  const scope={companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:1,role:actor.role};
  for(const [kind,ids] of [["invoice",[invoiceId]],["journal",(await all(db,'SELECT journal_id id FROM pos_postings WHERE company_id=?',actor.companyId)).map(row=>String(row.id))],["payment",(await all(db,'SELECT id FROM finance_payments WHERE company_id=?',actor.companyId)).map(row=>String(row.id))]] as const)for(const id of ids){expect(await financeProductDocumentEntry(db,scope,{kind,id})).toMatchObject({capability:"finance.pos-posting",purchasePurpose:"history"});const background=await financeSourceDocumentBackdrop(actor,kind,id);expect(background).toMatchObject({id});expect(typeof background?.date).toBe("string");}
  expect(await command('sale.create',input,actor,key)).toEqual(opened);
  await run(db,"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='pos-cashier'");
  await expect(command('sale.create',input,actor,key)).rejects.toMatchObject({code:'POS_ACCESS_CHANGED'});
  expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
 });
 it('package restaurant access follows the stored register and preserves existing kitchen settlement while draining',async()=>{
  await packageGrant();vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
  await expect(command('sale.create',{shiftId:'shift-kitchen',serviceMode:'retail'})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await expect(posWorkspace(actor,undefined,'','kitchen')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const retail=await createSale();await expect(add(retail,'latte')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await packageGrant('pos-restaurant');const sale=await createSale('restaurant');await add(sale,'latte');
  await packageGrant('pos-restaurant','draining');
  await expect(createSale('restaurant')).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
  await kitchen(sale,'send');await kitchen(sale,'start');await kitchen(sale,'ready');await kitchen(sale,'deliver');
  expect((await one(db,'SELECT state FROM pos_fulfillments WHERE id=(SELECT fulfillment_id FROM pos_lines WHERE sale_id=? LIMIT 1)',sale))!.state).toBe('delivered');
 });
 it('package expiry after a POS receipt and audit rolls back the sale and money snapshot before commit',async()=>{
  await packageGrant('pos','active',Date.now()+1500);vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
  const tables=['pos_sales','pos_receipts','pos_audit','pos_jobs','pos_reservations','pos_stock_moves','finance_journals','finance_journal_lines','money_transaction_snapshots','command_assertions'];
  const snapshot=()=>Promise.all(tables.map(table=>all(db,`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`)));const before=await snapshot();
  await db.exec("CREATE FUNCTION pos_package_delayed_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='sale.create' THEN PERFORM pg_sleep(1.6); END IF; RETURN NEW; END $$");
  await db.exec('CREATE TRIGGER pos_package_delayed_audit AFTER INSERT ON pos_audit FOR EACH ROW EXECUTE FUNCTION pos_package_delayed_audit()');
  try{await expect(createSale()).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});expect(await snapshot()).toEqual(before);}
  finally{await db.exec('DROP TRIGGER pos_package_delayed_audit ON pos_audit; DROP FUNCTION pos_package_delayed_audit()');}
 });
 it('retains original sale and payment FX through concurrent partial refunds and allocates the final USD cent exactly',async()=>{
  vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(new Date('2026-09-30T09:00:00Z'));
  await run(db,"UPDATE pos_shifts SET business_date='2026-09-30' WHERE id IN ('shift-retail','shift-kitchen')");
  const sale=await createSale();await add(sale,'tea',3000);await pay(sale,32100);await change('sale.complete',sale);
  const payment=(await one(db,'SELECT id FROM pos_payments WHERE sale_id=?',sale))!;
  const source=await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_sale' AND source_id=? AND amount_kind='total'",sale);
  expect(source).toMatchObject({amount_units:'32100',usd_units:'892'});
  expect(await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_payment' AND source_id=? AND amount_kind='received'",payment.id)).toEqual(source);
  expect(await one(db,"SELECT COUNT(*) n FROM money_transaction_snapshots WHERE source_type='pos_payment' AND source_id=?",payment.id)).toEqual({n:2});
  vi.setSystemTime(new Date('2026-10-01T09:00:00Z'));
  await run(db,"INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES('pos-later-quote','pos-company','THB','2026-10-01','40','manager',?,'Synthetic later quote; must not alter a source-linked return')",Date.now());
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const request={reason:'Reviewed later-day partial return',payoutShiftId:'shift-retail',lines:[{lineId:line.id,quantityMilli:1000,restock:true}]};
  const first=await change('refund.request',sale,request),race=await change('refund.request',sale,request);
  const approve=(id:unknown,key:string)=>command('refund.approve',{id,expectedVersion:1,reason:'Cash returned against the original verified receipt'},manager,key);
  const outcomes=await Promise.allSettled([approve(first.id,'pos-original-fx-first'),approve(race.id,'pos-original-fx-race')]);
  expect(outcomes.filter(result=>result.status==='fulfilled')).toHaveLength(1);
  expect(outcomes.find(result=>result.status==='rejected')).toMatchObject({reason:{code:'POS_REFUND_CHANGED'}});
  const approvedId=outcomes[0].status==='fulfilled'?first.id:race.id,approvedKey=outcomes[0].status==='fulfilled'?'pos-original-fx-first':'pos-original-fx-race';
  const journalCount=await one(db,'SELECT COUNT(*) n FROM finance_journals');
  expect(await approve(approvedId,approvedKey)).toEqual({id:approvedId});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journalCount);
  for(let n=0;n<2;n++){const refund=await change('refund.request',sale,request);await approve(refund.id,`pos-original-fx-rest-${n}`);}
  for(const sourceType of ['pos_refund','pos_refund_payment']){
   const rows=await all(db,'SELECT amount_units::text,usd_units::text,fx_rate_id,original_source_type,original_source_id,transaction_date FROM money_transaction_snapshots WHERE source_type=? ORDER BY captured_at,source_id',sourceType);
   expect(rows).toHaveLength(3);expect(rows.reduce((sum,row)=>sum+Number(row.amount_units),0)).toBe(-32100);expect(rows.reduce((sum,row)=>sum+Number(row.usd_units),0)).toBe(-892);
   expect(rows.map(row=>Math.abs(Number(row.usd_units))).sort()).toEqual([297,297,298]);
   for(const row of rows)expect(row).toMatchObject({fx_rate_id:source!.fx_rate_id,transaction_date:'2026-10-01',original_source_type:sourceType==='pos_refund'?'pos_sale':'pos_payment',original_source_id:sourceType==='pos_refund'?sale:payment.id});
  }
  expect(await one(db,'SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?','warehouse','tea')).toEqual({quantity_milli:10000,value_minor:30000});
  expect(await one(db,'SELECT amount_minor,refunded_minor FROM pos_payments WHERE id=?',payment.id)).toEqual({amount_minor:32100,refunded_minor:32100});
  expect(await one(db,"SELECT SUM(l.base_debit_minor-l.base_credit_minor) net FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id='pos-company' AND j.status='posted'")).toEqual({net:0});
  expect(await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_sale' AND source_id=? AND amount_kind='total'",sale)).toEqual(source);
 });
 it('does not silently open a sale in a different selected currency and requires a manual quote',async()=>{
  const request={shiftId:'shift-retail',serviceMode:'retail' as const};
  await expect(command('sale.create',{...request,currency:'USD'})).rejects.toMatchObject({code:'POS_CURRENCY_SETUP_REQUIRED'});
  await expect(command('sale.create',{...request,currency:'THB'})).rejects.toMatchObject({code:'MANUAL_RATE_REQUIRED'});
  await run(db,"INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)",'pos-manual-rate','pos-company','THB','2026-09-29','36','manager',Date.now(),'test quote');
  const sale=await command('sale.create',{...request,currency:'THB'});
  expect(await one(db,'SELECT currency FROM pos_sales WHERE id=?',sale.id)).toMatchObject({currency:'THB'});
  await add(String(sale.id));await pay(String(sale.id));await change('sale.complete',String(sale.id));
  const snapshot=await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_sale' AND source_id=?",sale.id);
  expect(snapshot).toMatchObject({amount_units:'10700',usd_units:'297',fx_rate_id:'pos-manual-rate'});
  await run(db,"INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)",'pos-revised-rate','pos-company','THB','2026-09-29','37','manager',Date.now()+1,'corrected later');
  expect(await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_sale' AND source_id=?",sale.id)).toEqual(snapshot);
 });
 it('bootstraps a retail register from Commerce once, reuses its primary accounts, and preserves code collisions',async()=>{
  await clearPosSetup();
  await run(db,"INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES('legacy-pos-advance','chart','pos-company','POS-CUSTOMER-ADVANCES','Legacy account','liability',0,0)");
  const result=await command('settings.bootstrap',{},actor,'pos-bootstrap-first-time');
  expect(result).toMatchObject({id:'pos-company',registerName:'Main retail register',warehouseId:'warehouse'});
  expect((result.accountsCreated as unknown[])).toHaveLength(11);
  const accounts=JSON.parse(String((await one(db,'SELECT accounts_json FROM pos_settings WHERE company_id=?','pos-company'))!.accounts_json));
  expect(accounts).toMatchObject({chartId:'chart',cash:'cash',receivable:'receivable',revenue:'revenue',outputTax:'outputTax',inventory:'inventory',cogs:'cogs'});
  expect(accounts.bank).toBe(result.bankAccountId);
  expect(accounts.edc).not.toBe('advance');
  expect(accounts.stripe).not.toBe('advance');
  expect(await one(db,'SELECT code,name,category FROM finance_accounts WHERE id=?','legacy-pos-advance')).toEqual({code:'POS-CUSTOMER-ADVANCES',name:'Legacy account',category:'liability'});
  expect(await one(db,'SELECT code,category,control_kind,is_control,currency FROM finance_accounts WHERE id=?',accounts.bank)).toMatchObject({code:'POS-BANK',category:'asset',control_kind:'bank',is_control:1,currency:'THB'});
  expect(await one(db,'SELECT id,mode,active,warehouse_id FROM pos_registers WHERE id=?',result.registerId)).toMatchObject({mode:'retail',active:true,warehouse_id:'warehouse'});
  expect(await one(db,'SELECT count(*) n FROM pos_shifts')).toEqual({n:0});
  expect(await command('settings.bootstrap',{},actor,'pos-bootstrap-first-time')).toEqual(result);
  await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-second-key')).rejects.toMatchObject({code:'POS_ALREADY_CONFIGURED'});
  expect(await one(db,"SELECT count(*) n FROM pos_registers WHERE company_id='pos-company' AND mode='retail'")).toEqual({n:1});
  expect(await one(db,"SELECT code FROM finance_accounts WHERE id=?",accounts.advance)).toEqual({code:'POS-CUSTOMER-ADVANCES-2'});
 });

 it('requires both POS configure and Finance write access for retail setup',async()=>{
  await clearPosSetup();
  const posOnly={...actor,permissions:['pos.configure']} as WorkContext;
  const financeOnly={...actor,permissions:['finance.write']} as WorkContext;
  await expect(command('settings.bootstrap',{},posOnly,'pos-bootstrap-no-finance')).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
  await expect(command('settings.bootstrap',{},financeOnly,'pos-bootstrap-no-pos')).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
  expect(await one(db,'SELECT count(*) n FROM pos_settings')).toEqual({n:0});
  expect(await one(db,"SELECT count(*) n FROM finance_accounts WHERE code LIKE 'POS-%'")).toEqual({n:0});
 });

 it('rejects a Commerce and chart currency mismatch before creating any accounts',async()=>{
  await clearPosSetup();
  await run(db,"UPDATE finance_charts SET base_currency='USD' WHERE id='chart'");
  await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-currency-mismatch')).rejects.toMatchObject({code:'POS_CURRENCY_MISMATCH'});
  expect(await one(db,'SELECT count(*) n FROM pos_settings')).toEqual({n:0});
  expect(await one(db,"SELECT count(*) n FROM finance_accounts WHERE code LIKE 'POS-%'")).toEqual({n:0});
 });

 it('keeps an existing POS policy and register unchanged when bootstrap is requested',async()=>{
  await run(db,"UPDATE pos_settings SET enabled=false,cashier_discount_bps=250,version=7 WHERE company_id='pos-company'");
  await run(db,"UPDATE pos_registers SET name='Keep existing register' WHERE id='retail'");
  const settingsBefore=await one(db,'SELECT * FROM pos_settings WHERE company_id=?','pos-company');
  const registerBefore=await one(db,'SELECT * FROM pos_registers WHERE id=?','retail');
  await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-preserve-existing')).rejects.toMatchObject({code:'POS_ALREADY_CONFIGURED'});
  expect(await one(db,'SELECT * FROM pos_settings WHERE company_id=?','pos-company')).toEqual(settingsBefore);
  expect(await one(db,'SELECT * FROM pos_registers WHERE id=?','retail')).toEqual(registerBefore);
 });

 it('attaches POS settings to one existing retail register without changing it',async()=>{
  await run(db,'DELETE FROM pos_settings WHERE company_id=?','pos-company');
  const registerBefore=await one(db,'SELECT * FROM pos_registers WHERE id=?','retail');
  const result=await command('settings.bootstrap',{},actor,'pos-bootstrap-reuse-register');
  expect(result).toMatchObject({registerId:'retail',registerName:'Shop',warehouseId:'warehouse',registerReused:true});
  expect(await one(db,'SELECT * FROM pos_registers WHERE id=?','retail')).toEqual(registerBefore);
  expect(await one(db,"SELECT count(*) n FROM pos_registers WHERE company_id='pos-company' AND mode='retail'")).toEqual({n:1});
  expect(await one(db,'SELECT currency,enabled FROM pos_settings WHERE company_id=?','pos-company')).toEqual({currency:'THB',enabled:true});
 });

 it('asks which existing retail register to configure when a company has several',async()=>{
  await run(db,'DELETE FROM pos_settings WHERE company_id=?','pos-company');
  await run(db,"INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode) VALUES('retail-2','pos-company','pos-branch','warehouse','Second shop','retail')");
  await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-register-choice-required')).rejects.toMatchObject({code:'POS_REGISTER_SELECTION_REQUIRED'});
  await expect(command('settings.bootstrap',{registerId:'restaurant'},actor,'pos-bootstrap-register-invalid')).rejects.toMatchObject({code:'POS_REGISTER_INVALID'});
  const result=await command('settings.bootstrap',{registerId:'retail-2'},actor,'pos-bootstrap-register-choice');
  expect(result).toMatchObject({registerId:'retail-2',registerName:'Second shop',registerReused:true});
  expect(await one(db,'SELECT name,active FROM pos_registers WHERE id=?','retail-2')).toEqual({name:'Second shop',active:true});
 });

 it('requires only the warehouse and bank choice that Commerce cannot infer',async()=>{
  await clearPosSetup();
  await run(db,"INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse-2','pos-company','pos-branch','POS-2','Second warehouse')");
  await run(db,"INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('bank-choice-1','chart','pos-company','BANK-1','Bank one','asset','THB',1,'bank',0,0),('bank-choice-2','chart','pos-company','BANK-2','Bank two','asset','THB',1,'bank',0,0)");
  await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-warehouse-choice')).rejects.toMatchObject({code:'POS_WAREHOUSE_SELECTION_REQUIRED'});
  await expect(command('settings.bootstrap',{warehouseId:'warehouse'},actor,'pos-bootstrap-bank-choice')).rejects.toMatchObject({code:'POS_BANK_ACCOUNT_SELECTION_REQUIRED'});
  const result=await command('settings.bootstrap',{warehouseId:'warehouse-2',bankAccountId:'bank-choice-2'},actor,'pos-bootstrap-selected-options');
  expect(result).toMatchObject({warehouseId:'warehouse-2',bankAccountId:'bank-choice-2'});
  expect(result.accountsCreated).toHaveLength(10);
  expect(await one(db,'SELECT branch_id,warehouse_id,mode FROM pos_registers WHERE id=?',result.registerId)).toEqual({branch_id:'pos-branch',warehouse_id:'warehouse-2',mode:'retail'});
 });

 it('rolls back every generated account and POS record if retail register creation fails',async()=>{
  await clearPosSetup();
  await db.exec("CREATE OR REPLACE FUNCTION pos_test_fail_register_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced POS bootstrap failure'; END $$; CREATE TRIGGER pos_test_fail_register_insert BEFORE INSERT ON pos_registers FOR EACH ROW EXECUTE FUNCTION pos_test_fail_register_insert();");
  try{
   await expect(command('settings.bootstrap',{},actor,'pos-bootstrap-forced-rollback')).rejects.toThrow('forced POS bootstrap failure');
  }finally{
   await db.exec('DROP TRIGGER IF EXISTS pos_test_fail_register_insert ON pos_registers; DROP FUNCTION IF EXISTS pos_test_fail_register_insert();');
  }
  expect(await one(db,'SELECT count(*) n FROM pos_settings')).toEqual({n:0});
  expect(await one(db,'SELECT count(*) n FROM pos_registers')).toEqual({n:0});
  expect(await one(db,"SELECT count(*) n FROM finance_accounts WHERE code LIKE 'POS-%'")).toEqual({n:0});
 });

 it('supports a cash sale after bootstrap without opening a session automatically',async()=>{
  await clearPosSetup();
  const setup=await command('settings.bootstrap',{},actor,'pos-bootstrap-cash-sale');
  await command('accounting.approve',{expectedVersion:1},manager);
  await command('catalog.save',{productId:'tea',barcode:'8850002',category:'Retail',kind:'stock',station:'Shop',recipe:[],modifiers:[],taxInclusive:false,registerIds:[setup.registerId],expectedVersion:1});
  expect(await one(db,'SELECT count(*) n FROM pos_shifts')).toEqual({n:0});
  const shift=await command('shift.open',{registerId:setup.registerId,openingMinor:25000},actor,'pos-bootstrap-open-cash');
  const sale=String((await command('sale.create',{shiftId:shift.id,serviceMode:'retail'},actor,'pos-bootstrap-cash-sale-create')).id);
  await add(sale,'tea');
  await pay(sale);
  await change('sale.complete',sale);
  expect(await one(db,'SELECT state,currency FROM pos_sales WHERE id=?',sale)).toEqual({state:'completed',currency:'THB'});
  expect(await one(db,"SELECT count(*) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.source_type='pos' AND j.source_id=? AND l.account_id='cash' AND l.debit_minor=10700",sale)).toEqual({n:1});
 });

 it('matches a transfer sale to imported bank evidence using the bootstrapped bank account',async()=>{
  await clearPosSetup();
  const setup=await command('settings.bootstrap',{},actor,'pos-bootstrap-transfer-sale');
  await command('accounting.approve',{expectedVersion:1},manager);
  await command('catalog.save',{productId:'tea',barcode:'8850002',category:'Retail',kind:'stock',station:'Shop',recipe:[],modifiers:[],taxInclusive:false,registerIds:[setup.registerId],expectedVersion:1});
  const shift=await command('shift.open',{registerId:setup.registerId,openingMinor:0},actor,'pos-bootstrap-open-transfer');
  const sale=String((await command('sale.create',{shiftId:shift.id,serviceMode:'retail'},actor,'pos-bootstrap-transfer-create')).id);
  await add(sale,'tea');
  const payment=await change('payment.create',sale,{method:'transfer',amountMinor:10700,tenderedMinor:10700,reference:'POS-TRANSFER-001'});
  const businessDate=String((await one(db,'SELECT business_date FROM pos_sales WHERE id=?',sale))!.business_date);
  await run(db,"INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES('bootstrap-statement','pos-company',?,'BOOTSTRAP-1',?,0,10700,'bootstrap-bank','finance',0)",setup.bankAccountId,businessDate);
  await run(db,"INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES('bootstrap-bank-row','bootstrap-statement','pos-company',?,'POS-TRANSFER-001',10700,'THB')",businessDate);
  expect((await one(db,'SELECT state FROM pos_payments WHERE id=?',payment.id))?.state).toBe('pending');
  const bank=await one(db,"SELECT version FROM finance_bank_transactions WHERE id='bootstrap-bank-row'");
  await command('payment.verify_transfer',{id:payment.id,expectedVersion:2,bankTransactionId:'bootstrap-bank-row',bankVersion:bank!.version},manager,'pos-bootstrap-transfer-match');
  expect(await one(db,"SELECT match_status,matched_type FROM finance_bank_transactions WHERE id='bootstrap-bank-row'")).toEqual({match_status:'matched',matched_type:'payment'});
  await change('sale.complete',sale);
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('completed');
 });

 it('cannot open a restaurant session while the hotel closes or has a different business date',async()=>{
  await run(db,"UPDATE pos_shifts SET state='closed',counted_minor=10000,expected_minor=10000,variance_minor=0,closed_at=1 WHERE id='shift-kitchen'");
  await run(db,"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,created_by,updated_at) VALUES('hotel','pos-company','pos-branch','Hotel',?,'closing',?,'system',0)",businessDate(),JSON.stringify(defaultConfig));
  await run(db,"UPDATE pos_registers SET property_id='hotel' WHERE id='restaurant'");
  await expect(command('shift.open',{registerId:'restaurant',openingMinor:0})).rejects.toMatchObject({code:'POS_HOTEL_DAY_CLOSED'});
  await run(db,"UPDATE pms_properties SET state='open',business_date='2000-01-01' WHERE id='hotel'");
  await expect(command('shift.open',{registerId:'restaurant',openingMinor:0})).rejects.toMatchObject({code:'POS_HOTEL_DAY_CLOSED'});
  await run(db,"UPDATE pms_properties SET business_date=? WHERE id='hotel'",businessDate());
  await expect(command('shift.open',{registerId:'restaurant',openingMinor:0})).resolves.toHaveProperty('id');
 });
 it('links an external tax reference to one posted sale with independent Finance review and correction history',async()=>{
  const {taxDocumentCommand,taxDocumentView,frontTaxDocumentCommand,frontTaxDocumentView}=await import("@erpx/server-finance/public/hospitality-tax-documents");
  const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);
  const invoiceId=String((await one(db,'SELECT invoice_id FROM pos_sales WHERE id=?',sale))!.invoice_id);
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  const front=await posWorkspace(actor,sale,'','sale',undefined,true);
  expect(front.taxDocumentInvoiceId).toBe(invoiceId);
  expect(front.sale).not.toHaveProperty('invoice_id');
  expect(front.accounts).toEqual([]);expect(front.bankTransactions).toEqual([]);
  expect((await posWorkspace(actor,sale,'','sale')).sale?.invoice_id).toBe(invoiceId);
  await expect(posWorkspace({...actor,companyId:'another-company'},sale,'','sale',undefined,true)).rejects.toBeDefined();
  await expect(posWorkspace({...actor,branchId:'another-branch'},sale,'','sale',undefined,true)).rejects.toBeDefined();
  await expect(taxDocumentView({...actor,companyId:'another-company'},invoiceId)).rejects.toMatchObject({code:'TAX_DOCUMENT_INVOICE_NOT_FOUND'});
  await expect(taxDocumentView({...actor,branchId:'another-branch'},invoiceId)).rejects.toMatchObject({code:'TAX_DOCUMENT_INVOICE_NOT_FOUND'});
  const input={action:'record' as const,invoiceId,issuer:'Fictional hotel tax desk',externalReference:'TAX-0001',issuedOn:'2026-09-23',documentTotalMinor:10700,documentTaxMinor:700,reason:'Matched to paper tax invoice'};
  await expect(frontTaxDocumentView(actor,invoiceId,'pms')).rejects.toMatchObject({code:'TAX_DOCUMENT_INVOICE_NOT_FOUND'});
  await expect(frontTaxDocumentCommand({...actor,companyId:'another-company'},input,'front-tax-cross-company','pos')).rejects.toMatchObject({code:'TAX_DOCUMENT_INVOICE_NOT_FOUND'});
  const fault='tax_document_late_'+crypto.randomUUID().replaceAll('-','');
  const snapshot=async()=>({invoice:await one(db,'SELECT * FROM finance_invoices WHERE id=?',invoiceId),documents:await one(db,'SELECT count(*) n FROM hospitality_tax_documents'),receipts:await one(db,'SELECT count(*) n FROM command_receipts'),audit:await one(db,'SELECT count(*) n FROM audit_events'),outbox:await one(db,'SELECT count(*) n FROM outbox_events')});
  const rejectLate=async(operation:string,attempt:()=>Promise<unknown>)=>{
   const before=await snapshot();
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'TAX_DOCUMENT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='${operation}') EXECUTE FUNCTION ${fault}();`);
   try{await expect(attempt()).rejects.toBeDefined();expect(await snapshot()).toEqual(before);}
   finally{await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON outbox_events;DROP FUNCTION IF EXISTS ${fault}();`);}
  };
  await rejectLate('hospitality.tax_document.record',()=>frontTaxDocumentCommand(actor,input,'tax-document-record-0001','pos'));
  const saved=await frontTaxDocumentCommand(actor,input,'tax-document-record-0001','pos');
  expect(await frontTaxDocumentCommand(actor,input,'tax-document-record-0001','pos')).toEqual(saved);
  expect(await frontTaxDocumentView(actor,invoiceId,'pos')).toMatchObject({canRecord:true,canReview:false});
  await expect(frontTaxDocumentCommand(manager,{action:'verify',invoiceId,documentId:saved.id,expectedVersion:1},'front-tax-review-forbidden','pos')).rejects.toMatchObject({code:'FRONT_COMMAND_FORBIDDEN'});
  expect((await taxDocumentView(actor,invoiceId)).documents).toMatchObject([{id:saved.id,state:'recorded',total_minor:10700,tax_minor:700}]);
  await expect(taxDocumentCommand(actor,{action:'verify',invoiceId,documentId:saved.id,expectedVersion:1},'tax-document-self-review')).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});
  await expect(taxDocumentCommand(manager,input,'tax-document-duplicate')).rejects.toMatchObject({code:'CONFLICT'});
  await expect(run(db,'UPDATE finance_invoices SET tax_minor=701 WHERE id=?',invoiceId)).rejects.toMatchObject({code:'23514'});
  const review={action:'verify' as const,invoiceId,documentId:saved.id,expectedVersion:1};
  await rejectLate('hospitality.tax_document.verify',()=>taxDocumentCommand(manager,review,'tax-document-reviewed'));
  const reviewed=await taxDocumentCommand(manager,review,'tax-document-reviewed');
  expect(reviewed).toMatchObject({state:'verified',version:2});
  expect(await taxDocumentCommand(manager,review,'tax-document-reviewed')).toEqual(reviewed);
  expect(await one(db,"SELECT count(*) n FROM command_receipts WHERE command_id IN ('tax-document-record-0001','tax-document-reviewed')")).toEqual({n:2});

  expect(await taxDocumentCommand(manager,{action:'void',invoiceId,documentId:saved.id,expectedVersion:2,reason:'Issuer voided paper original'},'tax-document-voided')).toMatchObject({state:'voided',version:3});
  const replacement=await taxDocumentCommand(actor,{...input,externalReference:'TAX-0002',documentTaxMinor:600,reason:'Replacement paper invoice has wrong tax'},'tax-document-replacement');
  await expect(taxDocumentCommand(manager,{action:'verify',invoiceId,documentId:replacement.id,expectedVersion:1},'tax-document-external-mismatch')).rejects.toMatchObject({code:'INVOICE_CHANGED'});
  await taxDocumentCommand(manager,{action:'void',invoiceId,documentId:replacement.id,expectedVersion:1,reason:'External tax amount incorrect'},'tax-document-mismatch-void');
  const corrected=await taxDocumentCommand(actor,{...input,externalReference:'TAX-0003',reason:'Corrected paper invoice'},'tax-document-corrected');
  expect((await taxDocumentView(actor,invoiceId)).documents).toEqual(expect.arrayContaining([expect.objectContaining({id:corrected.id,state:'recorded'}),expect.objectContaining({id:replacement.id,state:'voided'}),expect.objectContaining({id:saved.id,state:'voided'})]));
 });
 it('keeps company pilot rollout separate from setup, access, and production evidence',async()=>{
  const {companyFeatureCatalogue,changeCompanyFeatureRollout,assertProductionFeatureRoute}=await import("@erpx/server-pms/public/hospitality-rollout");
  vi.stubEnv('NODE_ENV','production');
  const initial=await companyFeatureCatalogue(actor);
  expect(initial.find(entry=>entry.id==='pos')).toMatchObject({state:'Coming Soon',rollout:'off',setupReady:true,productionAccepted:false,permitted:true});
  expect(initial.find(entry=>entry.id==='pms')).toMatchObject({state:'Coming Soon',setupReady:false,permitted:false});
  expect(initial.find(entry=>entry.id==='sales')).toMatchObject({state:'Coming Soon',rollout:'off'});
  const {readWorkSnapshot}=await import("@erpx/server-work_projection/public/work-items");
  expect(await readWorkSnapshot(actor)).toEqual({items:[],unavailableSources:[]});
  await expect(assertProductionFeatureRoute(db,actor.companyId,'pos/sale/create','POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  const change={moduleId:'pos',state:'pilot',expectedVersion:0,reason:'Approve isolated hotel trial'};
  const rolloutKey=`pilot-pos-rollout-${++serial}`;
  const result=await changeCompanyFeatureRollout(actor,change,rolloutKey);
  expect(result).toEqual({id:'pos',state:'pilot',version:1});
  expect(await changeCompanyFeatureRollout(actor,change,rolloutKey)).toEqual(result);
  expect((await companyFeatureCatalogue(actor)).find(entry=>entry.id==='pos')).toMatchObject({state:'Pilot',version:1});
  await expect(assertProductionFeatureRoute(db,actor.companyId,'pos/sale/create','POST')).resolves.toBeUndefined();
  await expect(assertProductionFeatureRoute(db,actor.companyId,'crm/lead/create','POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  await changeCompanyFeatureRollout(actor,{moduleId:'organization',state:'pilot',expectedVersion:0,reason:'Set up the hotel company'},`pilot-organization-${++serial}`);
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/organization/positions','POST')).resolves.toBeUndefined();
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/directory/person-1/grant','POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/lifecycle/cases/draft','POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  await changeCompanyFeatureRollout(actor,{moduleId:'identity_access',state:'pilot',expectedVersion:0,reason:'Invite hotel staff'},`pilot-identity-${++serial}`);
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/directory/person-1/grant','POST')).resolves.toBeUndefined();
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/lifecycle/cases/draft','POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  await changeCompanyFeatureRollout(actor,{moduleId:'people_payroll',state:'pilot',expectedVersion:0,reason:'Manage hotel staff'},`pilot-people-${++serial}`);
  await expect(assertProductionFeatureRoute(db,actor.companyId,'people/lifecycle/cases/draft','POST')).resolves.toBeUndefined();
  for(const destination of ['people/payroll/runs','people/pay-groups','people/country-packs','people/payments/batches'])
    await expect(assertProductionFeatureRoute(db,actor.companyId,destination,'POST')).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
  vi.stubEnv('ERPX_ENVIRONMENT','staging');
  for(const destination of ['people/payroll/runs','people/pay-groups','people/country-packs','people/payments/batches'])
    await expect(assertProductionFeatureRoute(db,actor.companyId,destination,'POST')).resolves.toBeUndefined();
  vi.stubEnv('ERPX_ENVIRONMENT','production');
  await expect(changeCompanyFeatureRollout(actor,{moduleId:'pos',state:'available',expectedVersion:1,reason:'Claim general release'},`pilot-pos-rollout-${++serial}`)).rejects.toMatchObject({code:'FEATURE_ACCEPTANCE_REQUIRED'});
  await expect(changeCompanyFeatureRollout(actor,{moduleId:'sales',state:'pilot',expectedVersion:0,reason:'Try broad orders'},`pilot-sales-rollout-${++serial}`)).rejects.toMatchObject({code:'FEATURE_FUTURE_GATE'});
 });
 it('limits production POS tenders to cash and verified bank transfer',async()=>{
  const sale=await createSale();await add(sale);
  const {changeCompanyFeatureRollout}=await import("@erpx/server-pms/public/hospitality-rollout");
  await changeCompanyFeatureRollout(actor,{moduleId:'pos',state:'pilot',expectedVersion:0,reason:'Accept POS tender trial'},`pilot-pos-tender-${++serial}`);
  vi.stubEnv('NODE_ENV','production');
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  const workspace=await posWorkspace(actor,sale,'','sale');
  expect(workspace.availableTenderMethods).toEqual(['cash','transfer']);
  expect(workspace.roomChargeEnabled).toBe(false);
  await expect(change('payment.create',sale,{method:'edc',amountMinor:10700,tenderedMinor:10700,reference:'EDC-001'})).rejects.toMatchObject({code:'POS_TENDER_COMING_SOON'});
  await expect(change('payment.create',sale,{method:'stripe_card',amountMinor:10700,tenderedMinor:10700,reference:''})).rejects.toMatchObject({code:'POS_TENDER_COMING_SOON'});
  await expect(command('connection.oauth',{livemode:false})).rejects.toMatchObject({code:'POS_PROVIDER_COMING_SOON'});
  expect(await one(db,'SELECT count(*) n FROM pos_oauth_states')).toEqual({n:0});
  expect(await one(db,'SELECT count(*) n FROM pos_payments')).toEqual({n:0});
  await expect(change('sale.charge_to_room',sale,{propertyId:'hotel',stayId:'stay',expectedPropertyVersion:1})).rejects.toMatchObject({code:'POS_ROOM_CHARGE_PILOT_GATE'});
 });
 it('keeps a transfer pending until a different Finance user matches exact imported bank evidence',async()=>{
  const sale=await createSale();await add(sale);
  const payment=await change('payment.create',sale,{method:'transfer',amountMinor:10700,tenderedMinor:10700,reference:'BANK-001'});
  expect((await one(db,'SELECT state FROM pos_payments WHERE id=?',payment.id))?.state).toBe('pending');
  expect(await one(db,'SELECT count(*) n FROM finance_payments')).toEqual({n:0});
  await expect(change('sale.complete',sale)).rejects.toMatchObject({code:'POS_NOT_PAID'});
  await run(db,"INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES('statement','pos-company','bank','S-1',?,0,10700,'fixture-bank-1','finance',0)",(await one(db,'SELECT business_date FROM pos_sales WHERE id=?',sale))!.business_date);
  await run(db,"INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES('bank-row','statement','pos-company',?,'BANK-001',10700,'THB')",(await one(db,'SELECT business_date FROM pos_sales WHERE id=?',sale))!.business_date);
  const verify={id:payment.id,expectedVersion:2,bankTransactionId:'bank-row',bankVersion:1};
  await expect(command('payment.verify_transfer',verify,actor)).rejects.toMatchObject({code:'POS_INDEPENDENT_VERIFICATION'});
  await run(db,"UPDATE finance_bank_transactions SET amount_minor=10600 WHERE id='bank-row'");
  await expect(command('payment.verify_transfer',verify,manager)).rejects.toMatchObject({code:'POS_BANK_EVIDENCE_MISMATCH'});
  expect((await one(db,'SELECT state FROM pos_payments WHERE id=?',payment.id))?.state).toBe('pending');
  await run(db,"UPDATE finance_bank_transactions SET amount_minor=10700 WHERE id='bank-row'");
  const result=await command('payment.verify_transfer',verify,manager,'verify-transfer-replay');
  expect(result).toMatchObject({id:payment.id,state:'succeeded',bankTransactionId:'bank-row'});
  expect(await command('payment.verify_transfer',verify,manager,'verify-transfer-replay')).toEqual(result);
  const settled=await one(db,'SELECT finance_payment_id FROM pos_payments WHERE id=?',payment.id);
  expect(await one(db,"SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id='bank-row'")).toEqual({match_status:'matched',matched_type:'payment',matched_id:settled!.finance_payment_id});
  await change('sale.complete',sale);
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('completed');
 });
 it('charges one restaurant bill to an in-house guest account atomically, without POS revenue or payment',async()=>{
  await packageGrant();await packageGrant('pos-restaurant');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
  const sale=await createSale('dine_in');await add(sale);
  const saleRow=await one(db,'SELECT * FROM pos_sales WHERE id=?',sale);
  const accounts={...defaultConfig.accounts};
  for(const [i,key] of pmsAccountKeys.entries()){
   const account=`pms-${key}`;accounts[key]=account;
   const category=['revenue','service'].includes(key)?'revenue':['advance','deposit','tax'].includes(key)?'liability':['consumption','commission','cashDifference'].includes(key)?'expense':'asset';
   await run(db,'INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,?,0,0)',account,'chart','pos-company',`PMS-${i}`,key,category,'THB');
  }
  const config={...defaultConfig,chartId:'chart',accounts,approvedBy:'independent-accountant',businessDate:saleRow!.business_date};
  await run(db,"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES(?,?,?,'Hotel',?,'open',?,1,'system',0)",'hotel','pos-company','pos-branch',saleRow!.business_date,JSON.stringify(config));
  await db.transaction(async tx=>{
   await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
   for(const [id,kind,data] of [
    ['guest','guest',{name:'Example Guest'}],
    ['room','room',{number:'101'}],
    ['guest_account','guest_account',{name:'Guest Account',guestId:'guest',reservationId:'reservation',partyId:'',state:'open',due:0,advance:0,advanceTax:0,deposit:0}],
    ['stay','stay',{guestId:'guest',reservationId:'reservation',roomId:'room',guestAccountId:'guest_account',state:'in_house'}],
   ] as const)await run(tx,'INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,0)',id,'hotel',kind,JSON.stringify(data));
  });
  await run(db,"UPDATE pos_registers SET property_id='hotel' WHERE id='restaurant'");
  const input={id:sale,expectedVersion:saleRow!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:1};
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  expect((await posWorkspace(actor,sale,'','sale')).roomChargeEnabled).toBe(false);
  await expect(command('sale.charge_to_room',input,actor,'room-charge-unpurchased-addon')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await packageGrant('pos-hotel-charge');
  await expect(command('sale.charge_to_room',input,actor,'room-charge-unpurchased-hotel')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await packageGrant('pms');
  expect((await readCompanyPackageGrants(db,actor.companyId)).map(grant=>grant.packageId).sort()).toEqual(['pms','pos','pos-hotel-charge','pos-restaurant']);
  expect((await posWorkspace(actor,sale,'','sale')).roomChargeStays).toMatchObject([{stay_id:'stay',room_number:'101',guest_name:'Example Guest'}]);
  await expectLatePosAuditRollback('sale.charge_to_room',input,actor,'room-charge-replay');
  const charged=await command('sale.charge_to_room',input,actor,'room-charge-replay');
  expect(charged).toMatchObject({id:sale,state:'completed',tender:'room_charge',propertyId:'hotel'});
  expect(await command('sale.charge_to_room',input,actor,'room-charge-replay')).toEqual(charged);
  expect((await one(db,"SELECT data_json::jsonb->>'due' due FROM pms_records WHERE id='guest_account'"))?.due).toBe('10700');
  expect(await one(db,'SELECT count(*) n FROM pms_sources')).toEqual({n:1});
  expect((await posWorkspace(actor,sale,'','sale')).roomCharge?.guest_account_entry_id).toBe(charged.guestAccountEntryId);
  expect(await one(db,'SELECT count(*) n FROM pms_postings')).toEqual({n:1});
  expect(await one(db,'SELECT count(*) n FROM pos_payments')).toEqual({n:0});
  expect(await one(db,"SELECT count(*) n FROM pos_postings WHERE source_key LIKE 'sale:%' OR source_key LIKE 'receipt:%'")).toEqual({n:0});
  expect((await posWorkspace(actor,undefined,'','reports')).reports).toEqual([]);
  expect((await posDayReport(db,actor,'pos-branch',String(saleRow!.business_date))).totals).toMatchObject({roomChargeMinor:10700,grossMinor:0,taxMinor:0,received:{}});
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tea'")).toEqual({quantity_milli:9000});
  const other=await createSale('dine_in');await add(other);
  const changed=await one(db,'SELECT version FROM pos_sales WHERE id=?',other);
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:1})).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT'});
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',other))?.state).toBe('open');
  await db.transaction(async tx=>{
   await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
   await run(tx,"UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"closed\"')::text WHERE id='guest_account'");
  });
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:2})).rejects.toMatchObject({code:'PMS_CHARGE_GUEST_ACCOUNT'});
  expect(await one(db,'SELECT count(*) n FROM pms_sources')).toEqual({n:1});
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',other))?.state).toBe('open');
  await run(db,'UPDATE assignments SET revoked_at=? WHERE id=?',Date.now(),'pos-cashier');
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:2})).rejects.toMatchObject({code:'POS_ACCESS_CHANGED'});
  const {pmsCommand}=await import("../../../packages/server-pms/src/public/pms");
  await expect(pmsCommand(db,manager,{operation:'charge.reverse',propertyId:'hotel',expectedVersion:2,input:{id:charged.guestAccountEntryId,reason:'Wrong room'}},'direct-pos-charge-reversal')).rejects.toMatchObject({code:'PMS_POS_REVERSAL_REQUIRED'});
  const current=(await one(db,'SELECT version FROM pos_sales WHERE id=?',sale))!;
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const reversalInput={id:sale,expectedVersion:current.version,propertyId:'hotel',expectedPropertyVersion:2,reason:'Guest bill corrected',restockLineIds:[line.id]};
  await expect(command('sale.reverse_room_charge',reversalInput,manager,'old-immediate-reversal')).rejects.toMatchObject({code:'POS_REVERSAL_WORKFLOW_REQUIRED'});
  const requested=await command('room_charge_reversal.request',reversalInput,manager,'room-charge-reversal-request');
  expect(await one(db,'SELECT count(*) n FROM pos_room_charge_reversals')).toEqual({n:0});
  await expect(command('room_charge_reversal.approve_pms',{id:requested.id,expectedVersion:1,reason:'Hotel review'},hotelManager)).rejects.toMatchObject({code:'POS_REVERSAL_STAGE'});
  await expect(command('room_charge_reversal.approve_pos',{id:requested.id,expectedVersion:1,reason:'Own request'},manager)).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  await run(db,"INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES('restaurant-reviewer','restaurant-reviewer','holding_owner','[\"pos\"]','pos-company','holding',0,9999999999999)");
  const restaurantReviewer={...manager,userId:'restaurant-reviewer',assignmentId:'restaurant-reviewer'};
  await command('room_charge_reversal.approve_pos',{id:requested.id,expectedVersion:1,reason:'Independent restaurant review'},restaurantReviewer);
  await expect(command('room_charge_reversal.approve_pms',{id:requested.id,expectedVersion:2,reason:'Same reviewer'},restaurantReviewer)).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  const decision={id:requested.id,expectedVersion:2,reason:'Independent hotel review'};
  await expectLatePosAuditRollback('room_charge_reversal.approve_pms',decision,hotelManager,'room-charge-reversal-replay');
  const reversed=await command('room_charge_reversal.approve_pms',decision,hotelManager,'room-charge-reversal-replay');
  expect(reversed).toMatchObject({id:requested.id,saleId:sale,state:'room_charge_reversed',restoredCostMinor:3000});
  expect(await command('room_charge_reversal.approve_pms',decision,hotelManager,'room-charge-reversal-replay')).toEqual(reversed);
  expect((await one(db,"SELECT data_json::jsonb->>'due' due FROM pms_records WHERE id='guest_account'"))?.due).toBe('0');
  expect(await one(db,'SELECT count(*) n FROM pms_postings')).toEqual({n:2});
  expect(await one(db,'SELECT count(*) n FROM pos_room_charge_reversals')).toEqual({n:1});
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tea'")).toEqual({quantity_milli:10000,value_minor:30000});
  expect((await posWorkspace(manager,sale,'','sale')).roomCharge?.reversal_entry_id).toBe(reversed.reversalEntryId);
  await expect(change('sale.reverse_room_charge',sale,{propertyId:'hotel',expectedPropertyVersion:3,reason:'Again',restockLineIds:[]},manager)).rejects.toMatchObject({code:'POS_REVERSAL_WORKFLOW_REQUIRED'});
  await run(db,'UPDATE assignments SET revoked_at=NULL WHERE id=?','pos-cashier');
  const second=await command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:3},actor);
  expect(second.state).toBe('completed');
  const staleRequest=await change('room_charge_reversal.request',other,{propertyId:'hotel',expectedPropertyVersion:4,reason:'Frozen correction review',restockLineIds:[]},manager);
  await expect(change('room_charge_reversal.request',other,{propertyId:'hotel',expectedPropertyVersion:4,reason:'Duplicate pending correction',restockLineIds:[]},manager)).rejects.toMatchObject({code:'POS_REVERSAL_PENDING'});
  await expect(run(db,'UPDATE pos_room_charge_reversal_requests SET reason=? WHERE id=?','Changed frozen reason',staleRequest.id)).rejects.toMatchObject({code:'23514'});
  await run(db,'UPDATE pos_sales SET version=version+1 WHERE id=?',other);
  await expect(command('room_charge_reversal.approve_pos',{id:staleRequest.id,expectedVersion:1,reason:'Changed source review'},restaurantReviewer)).rejects.toMatchObject({code:'POS_REVERSAL_STALE'});
  await command('room_charge_reversal.reject_pos',{id:staleRequest.id,expectedVersion:1,reason:'Source changed; request again'},restaurantReviewer);
  expect(await one(db,'SELECT count(*) n FROM pos_room_charge_reversals')).toEqual({n:1});
  await reviewShifts();
  const prepared=await command('day.prepare',{branchId:'pos-branch',businessDate:String(saleRow!.business_date)},manager);
  vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(new Date(new Date(`${saleRow!.business_date}T12:00:00+07:00`).getTime()+86400000));
  const correction={propertyId:'hotel',expectedPropertyVersion:4,reason:'Prepared item discarded',restockLineIds:[]};
  const correctionRequest=await change('room_charge_reversal.request',other,correction,manager);
  await command('room_charge_reversal.approve_pos',{id:correctionRequest.id,expectedVersion:1,reason:'Restaurant correction review'},restaurantReviewer);
  const hotelDecision={id:correctionRequest.id,expectedVersion:2,reason:'Hotel correction review'};
  await expect(command('room_charge_reversal.approve_pms',hotelDecision,hotelManager)).rejects.toMatchObject({code:'POS_DAY_REVIEW_PENDING'});
  await command('day.confirm',{id:prepared.id,expectedVersion:1});
  await expect(command('room_charge_reversal.approve_pms',hotelDecision,hotelManager)).rejects.toMatchObject({code:'POS_HOTEL_DAY_CLOSED'});
  expect(await one(db,'SELECT count(*) n FROM pos_room_charge_reversals')).toEqual({n:1});
  expect(await one(db,'SELECT version FROM pms_properties WHERE id=?','hotel')).toEqual({version:4});
  await run(db,'UPDATE pms_properties SET business_date=? WHERE id=?',businessDate(),'hotel');
  const wasted=await command('room_charge_reversal.approve_pms',hotelDecision,hotelManager);
  expect(wasted).toMatchObject({restoredCostMinor:0,wastedCostMinor:3000});
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tea'")).toEqual({quantity_milli:9000});
  expect(await one(db,"SELECT count(*) n FROM pos_postings WHERE source_key LIKE 'room-reversal:%'")).toEqual({n:2});
 });
 it('reports hotel blockers and actual product availability on every daily tab',async()=>{
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  await run(db,"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,created_by,updated_at) VALUES('hotel','pos-company','pos-branch','Hotel',?,'setup',?,'system',0)",businessDate(),JSON.stringify(defaultConfig));
  await run(db,"UPDATE pos_registers SET mode='restaurant',property_id='hotel'");
  for(const tab of ['sale','kitchen','shifts','history']) {
   const data=await posWorkspace(actor,undefined,'',tab,undefined,true);
   expect(data.frontReady).toBe(false);
   expect(data.frontSetup?.hasProducts).toBe(true);
   expect(data.frontSetup?.blockedRegisters).toMatchObject([{hotel_name:'Hotel',hotel_state:'setup',finance_approved:false},{hotel_name:'Hotel',hotel_state:'setup',finance_approved:false}]);
   if(tab!=='sale')expect(data.products).toEqual([]);
  }
  await run(db,"UPDATE pms_properties SET state='open'");
  const open=await posWorkspace(actor,undefined,'','kitchen',undefined,true);
  expect(open.frontReady).toBe(true);expect(open.frontSetup?.blockedRegisters).toEqual([]);
  await run(db,"UPDATE pms_properties SET business_date='2000-01-01'");
  expect((await posWorkspace(actor,undefined,'','shifts',undefined,true)).frontReady).toBe(false);
  await run(db,"UPDATE commerce_products SET status='archived'");
  expect((await posWorkspace(actor,undefined,'','history',undefined,true)).frontSetup?.hasProducts).toBe(false);
 });
 it('exposes company currency without exposing accounting configuration',async()=>{
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  const cashier={...actor,role:'pos_cashier'};
  await run(db,'DELETE FROM pos_settings WHERE company_id=?',actor.companyId);
  await run(db,"UPDATE commerce_settings SET currency='USD' WHERE company_id=?",actor.companyId);
  const unconfigured=await posWorkspace(cashier,undefined,'','sale');
  expect(unconfigured.currency).toBe('USD');
  expect(unconfigured.commerce).toBeNull();
  expect(unconfigured.accounts).toEqual([]);
  expect(formatPosMoney(10700,unconfigured.currency)).toContain('USD');
  expect(formatPosMoney(10700,null)).toBe('—');
 });
 it('keeps historical document currencies separate in sales and tender reports',async()=>{
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  const first=await createSale();await add(first);await pay(first);await change('sale.complete',first);
  const second=await createSale();await add(second);await pay(second);await change('sale.complete',second);
  // Simulate imported historical documents; this is not a currency-conversion command.
  await run(db,"UPDATE pos_sales SET currency='USD' WHERE id=?",second);
  const workspace=await posWorkspace(actor,undefined,'','reports');
  expect(workspace.currency).toBe('THB');
  expect(workspace.reports.map(row=>[row.currency,row.total_minor]).sort()).toEqual([['THB',10700],['USD',10700]]);
  expect(workspace.paymentMix.map(row=>[row.currency,row.received_minor]).sort()).toEqual([['THB',10700],['USD',10700]]);
  expect((await posWorkspace(actor,second,'','sale')).sale?.currency).toBe('USD');
 });
 it('pages history without loading other tabs and keeps every sale reachable',async()=>{
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  await db.exec(`INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,accounts_json)
   SELECT 'page-'||n,'pos-company','pos-branch','retail','shift-retail','PAGE-'||n,'THB','2026-09-01','cashier',1720000000000+n,1720000000000+n,'{}' FROM generate_series(1,65) n`);
  const first=await posWorkspace(actor,undefined,'PAGE-','history');
  expect(first.sales).toHaveLength(30);expect(first.products).toEqual([]);expect(first.stock).toEqual([]);expect(first.reports).toEqual([]);
  const second=await posWorkspace(actor,undefined,'PAGE-','history',first.salesPage!.nextCursor!);
  const last=await posWorkspace(actor,undefined,'PAGE-','history',second.salesPage!.nextCursor!);
  expect(last.sales).toHaveLength(5);expect(last.salesPage!.nextCursor).toBeNull();
  expect(new Set([...first.sales,...second.sales,...last.sales].map(s=>s.id)).size).toBe(65);
  const previous=await posWorkspace(actor,undefined,'PAGE-','history',second.salesPage!.previousCursor!);
  expect(previous.sales.map(s=>s.id)).toEqual(first.sales.map(s=>s.id));
  const kitchen=await posWorkspace(actor,undefined,'','kitchen');expect(kitchen.sales).toEqual([]);expect(kitchen.products).toEqual([]);
 });
 it('executes scoped commerce, finance and inventory page queries',async()=>{
  const {commercePage}=await import("@erpx/server-commerce/public/commerce");
  const {invoicesPage,bankWorkspace}=await import("@erpx/server-finance/public/finance");
  const {countsPage}=await import("@erpx/server-inventory/public/inventory-counts");
  const {transfersPage}=await import("@erpx/server-inventory/public/inventory-transfers");
  const reader={...actor,modules:['pos','commerce','finance','inventory']};
  for(const view of ['sales','purchase','quote','products','parties','warehouses','stock']){
   const page=await commercePage(reader,{view});expect(page.page.items.length).toBeLessThanOrEqual(30);
   expect(page.options).toBeNull();
  }
  expect((await invoicesPage(reader,{})).items).toEqual([]);
  expect((await countsPage(reader,{})).items).toEqual([]);
  expect((await transfersPage(reader,{})).items).toEqual([]);
  const bank=await bankWorkspace(reader,{});expect(bank.transactionPage).not.toBeNull();expect(bank.statementPage).not.toBeNull();
 });
 it('cash sale posts one balanced invoice, receipt, allocation and stock cost; lost-response replay is safe',async()=>{
  const sale=await createSale();await add(sale);await pay(sale);
  const row=(await one(db,'SELECT * FROM pos_sales WHERE id=?',sale))!,input={id:sale,expectedVersion:row.version};
  const first=await command('sale.complete',input,actor,'pos-complete-replay');expect(await command('sale.complete',input,actor,'pos-complete-replay')).toEqual(first);
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:9000,value_minor:27000});
  expect(await one(db,'SELECT status,outstanding_minor,total_minor FROM finance_invoices')).toEqual({status:'paid',outstanding_minor:0,total_minor:10700});
  const journals=await all(db,'SELECT journal_id,SUM(debit_minor-credit_minor) balance FROM finance_journal_lines GROUP BY journal_id');expect(journals.length).toBe(4);expect(journals.every(j=>j.balance===0)).toBe(true);
  expect(await one(db,'SELECT COUNT(*) n FROM finance_allocations')).toEqual({n:1});
  await expect(command('sale.complete',{...input,expectedVersion:99},actor,'pos-complete-replay')).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 });
 it('binds void evidence atomically and rejects a stale POS sale set',async()=>{
  const sale=await createSale();await add(sale);
  const original=await one(db,'SELECT version FROM pos_sales WHERE id=?',sale);
  const stale=await seedAttachmentSet(db,actor,{targetType:'pos_sale',targetId:sale,componentKind:'action',componentRef:'void',purposeKey:'void_evidence',expectedTargetVersion:Number(original!.version)});
  await change('sale.hold',sale);
  const held=await one(db,'SELECT version,state FROM pos_sales WHERE id=?',sale);
  await expect(command('sale.cancel',{id:sale,expectedVersion:Number(held!.version),attachmentSetIds:[stale.setId]})).rejects.toMatchObject({status:409,code:'ATTACHMENT_SET_STALE'});
  expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'held'});
  expect(await attachmentBindingState(db,stale.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
  const evidence=await seedAttachmentSet(db,actor,{targetType:'pos_sale',targetId:sale,componentKind:'action',componentRef:'void',purposeKey:'void_evidence',expectedTargetVersion:Number(held!.version)});
  const cancel={id:sale,expectedVersion:Number(held!.version),attachmentSetIds:[evidence.setId]},cancelKey='pos-void-rollback-replay';
  await expectLatePosAuditRollback('sale.cancel',cancel,actor,cancelKey);
  const cancelled=await command('sale.cancel',cancel,actor,cancelKey);expect(await command('sale.cancel',cancel,actor,cancelKey)).toEqual(cancelled);
  expect(await one(db,'SELECT COUNT(*) n FROM pos_receipts WHERE id=?',cancelKey)).toEqual({n:1});
  expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'cancelled'});
  expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'pos_void'});
 });
 it('split tender never overpays and closing a shift does not repost sales',async()=>{
  const sale=await createSale();await add(sale);await pay(sale,5000);await change('payment.create',sale,{method:'edc',amountMinor:5700,tenderedMinor:5700,reference:'EDC-123'});await change('sale.complete',sale);
  const before=await one(db,'SELECT COUNT(*) n FROM finance_journals');await command('shift.close',{id:'shift-retail',expectedVersion:1,countedMinor:15000});expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(before);
  await expect(command('shift.review',{id:'shift-retail',expectedVersion:2,reason:'count matches'})).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  const variance=await seedAttachmentSet(db,manager,{targetType:'pos_shift',targetId:'shift-retail',componentKind:'action',componentRef:'variance-review',purposeKey:'cash_variance',expectedTargetVersion:2});
  await command('shift.review',{id:'shift-retail',expectedVersion:2,reason:'count matches',attachmentSetIds:[variance.setId]},manager);
  expect(await attachmentBindingState(db,variance.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'pos_cash_variance_review'});
 });
 it('serializes two tills reserving the final unit and guards other inventory writers',async()=>{
  await run(db,"UPDATE commerce_stock SET quantity_milli=1000,value_minor=3000 WHERE product_id='tea'");const a=await createSale(),b=await createSale();await add(a);await add(b);
  const results=await Promise.allSettled([pay(a),pay(b)]);expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
  await expect(run(db,"UPDATE commerce_stock SET quantity_milli=0,value_minor=0 WHERE product_id='tea'")).rejects.toMatchObject({code:'23514'});
  expect(await one(db,"SELECT SUM(quantity_milli) qty FROM pos_reservations WHERE product_id='tea'")).toEqual({qty:1000});
 });
 it('serializes Sales and POS competing for the final unit through the same availability owner',async()=>{
  const {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder,changeOrderReservation}=await import("@erpx/server-commerce/public/commerce");
  const {assignFlowPrimary,resolveFlow}=await import("@erpx/server-workflow/public/flow-routing");
  const next=()=>`cross-stock-${++serial}`;
  for(const who of [actor,manager]){
   await run(db,"INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)",`person-${who.userId}`,who.userId,who.companyId,who.userId);
   await run(db,"INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'pos-branch','employee','salaried','active','2020-01-01','Cross-channel team','THB','fixture',1,1)",`engagement-${who.userId}`,`person-${who.userId}`,who.companyId);
  }

  await run(db,"UPDATE commerce_stock SET quantity_milli=1000,value_minor=3000 WHERE product_id='tea'");
  const party=await saveMaster(actor,'parties',null,{kind:'customer',code:next(),name:'Cross-channel stock customer'},next());
  const order=await saveOrder(actor,null,orderSchema.parse({kind:'sales',partyId:party.id,warehouseId:'warehouse',orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:'tea',quantity:'1',price:'100',taxBps:700}]}),next());
  const orderId=String(order.id);await transitionOrder(actor,orderId,{action:'submit',expectedVersion:1},next());
  const flow=await resolveFlow(db,actor,'order',orderId);
  await assignFlowPrimary(db,actor,'order',orderId,{stepKey:'submitted',userId:manager.userId,reason:'Independent sales manager for cross-channel test',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},next());
  await transitionOrder(manager,orderId,{action:'approve',expectedVersion:2},next());
  const details=await getOrder(actor,orderId),sale=await createSale();await add(sale);
  const results=await Promise.allSettled([
   changeOrderReservation(actor,orderId,{expectedVersion:3,action:'reserve',lineId:details.lines[0].id,quantity:'1',expiresAt:Date.now()+3600000,reason:'Customer dispatch commitment'},next()),pay(sale),
  ]);
  expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
  expect(await one(db,"SELECT SUM(quantity_milli) n FROM inventory_active_reservations WHERE product_id='tea'")).toEqual({n:1000});
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:1000,value_minor:3000});
 });
 it('shares the scoped kitchen queue with cashier and kitchen while retaining distinct command permissions',async()=>{
  const sale=await createSale('restaurant');await add(sale,'latte');
  const cashier=await posWorkspace({...actor,role:'pos_cashier'},undefined,'','kitchen',undefined,true);
  const cook=await posWorkspace({...actor,role:'pos_kitchen'},undefined,'','kitchen',undefined,true);
  expect(cashier.kitchen).toHaveLength(1);expect(cook.kitchen).toHaveLength(1);
  expect(cashier.permissions['pos.sell']).toBe(true);expect(cashier.permissions['pos.kitchen']).toBe(false);
  expect(cook.permissions['pos.kitchen']).toBe(true);expect(cook.permissions['pos.sell']).toBe(false);
  expect((await posWorkspace({...actor,role:'pos_cashier',companyId:'another-company'},undefined,'','kitchen',undefined,true)).kitchen).toEqual([]);
  expect((await posWorkspace({...actor,role:'pos_cashier',branchId:'another-branch'},undefined,'','kitchen',undefined,true)).kitchen).toEqual([]);
 });
 it('reserves recipes on send, consumes on start, moves WIP to COGS on delivery',async()=>{
  const sale=await createSale('restaurant');await add(sale,'latte');await kitchen(sale,'send');expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE product_id='milk'")).toEqual({quantity_milli:10000});
  await kitchen(sale,'start');expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='milk'")).toEqual({quantity_milli:9800,value_minor:49000});
  await kitchen(sale,'ready');await kitchen(sale,'deliver');await pay(sale,12840);await change('sale.complete',sale);
  expect(await one(db,"SELECT SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE account_id='wip'")).toEqual({balance:0});
  expect(await one(db,"SELECT SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE account_id='cogs'")).toEqual({balance:1000});
 });
 it('cancelled preparation records waste and never restores ingredients',async()=>{
  const sale=await createSale('restaurant');await add(sale,'latte');await kitchen(sale,'send');await kitchen(sale,'start');await kitchen(sale,'cancel');
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE product_id='milk'")).toEqual({quantity_milli:9800});expect(await one(db,"SELECT SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE account_id='waste'")).toEqual({balance:1000});
 });
 it('splits a dispatched recipe without creating another kitchen item or consuming twice',async()=>{
  const sale=await createSale('restaurant');await add(sale,'latte',2000);await kitchen(sale,'send');const line=(await one(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale))!;
  const split=await change('sale.split',sale,{lineId:line.id,quantityMilli:1000});expect(await one(db,'SELECT COUNT(*) n FROM pos_fulfillments')).toEqual({n:1});
  await kitchen(sale,'start');await kitchen(sale,'ready');await kitchen(sale,'deliver');await pay(sale,12840);await change('sale.complete',sale);await pay(String(split.id),12840);await change('sale.complete',String(split.id));
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE product_id='milk'")).toEqual({quantity_milli:9600});expect(await one(db,'SELECT COUNT(*) n FROM pos_stock_moves')).toEqual({n:1});
 });
 it('partial returns require independent review, restore original cost, and cannot exceed original quantity',async()=>{
  const refundRateId=`pos-refund-rate-${++serial}`;
  await run(db,"INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)",refundRateId,'pos-company','THB',businessDate(),'36','manager',Date.now(),'refund quote');
  const sale=await createSale();await add(sale,'tea',2000);await pay(sale,21400);await change('sale.complete',sale);const line=(await one(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'one damaged package',payoutShiftId:'shift-retail',lines:[{lineId:line.id,quantityMilli:1000,restock:true}]});
  await expect(command('refund.approve',{id:refund.id,expectedVersion:1,reason:'cash returned'})).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  const evidence=await seedAttachmentSet(db,manager,{targetType:'pos_refund',targetId:String(refund.id),componentKind:'action',componentRef:'refund-decision',purposeKey:'refund_evidence',expectedTargetVersion:1});
  await command('refund.approve',{id:refund.id,expectedVersion:1,reason:'cash returned',attachmentSetIds:[evidence.setId]},manager);
  expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'pos_refund_decision'});
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:9000,value_minor:27000});
  expect(await one(db,'SELECT state FROM pos_refunds WHERE id=?',refund.id)).toEqual({state:'completed'});
  expect((await posDayReport(db,manager,'pos-branch',businessDate())).totals).toMatchObject({taxMinor:1400,refundTaxMinor:700,netTaxMinor:700,refundMinor:10700,netMinor:10700});
  expect(await one(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_refund' AND source_id=?",refund.id)).toEqual({amount_units:'-10700',usd_units:'-297',fx_rate_id:refundRateId});
  expect((await all(db,"SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_type='pos_refund_payment' AND source_id IN (SELECT id FROM pos_refund_payments WHERE refund_id=?)",refund.id))).toEqual([{amount_units:'-10700',usd_units:'-297',fx_rate_id:refundRateId}]);
  await expect(change('refund.request',sale,{reason:'too many',lines:[{lineId:line.id,quantityMilli:2000,restock:true}]})).rejects.toMatchObject({code:'POS_REFUND_QUANTITY'});
 });
 it('closed periods and changed scopes leave no payment or journal effects',async()=>{
  const sale=await createSale();await add(sale);await run(db,"UPDATE finance_periods SET state='closed'");await expect(pay(sale)).rejects.toMatchObject({code:'POS_PERIOD_CLOSED'});expect(await one(db,'SELECT COUNT(*) n FROM pos_payments')).toEqual({n:0});
  await expect(change('sale.hold',sale,{}, {...actor,branchId:'other'})).rejects.toMatchObject({status:404});
 });
});
describe('POS provider recovery and reporting',()=>{
 it('requires a PromptPay payer email before reserving stock or queuing a provider request',async()=>{
  const sale=await createSale();await add(sale);
  await expect(change('payment.create',sale,{method:'stripe_promptpay',amountMinor:10700,tenderedMinor:10700})).rejects.toMatchObject({code:'POS_PROMPTPAY_EMAIL_REQUIRED',status:422});
  for(const table of ['pos_payments','pos_jobs','pos_reservations','finance_payments','finance_journals'])expect(await one(db,`SELECT COUNT(*) n FROM ${table}`)).toEqual({n:0});
  expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'open'});
 });
 it('repairs a legacy missing-email request using the same payment and provider idempotency key',async()=>{
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES('pos-company','acct_original',false,true,true,true,true,'ready','daily',true)");
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  const sale=await createSale();await add(sale);
  const payment=await change('payment.create',sale,{method:'stripe_promptpay',amountMinor:10700,tenderedMinor:10700,payerEmail:'payer@example.test'});
  await run(db,'UPDATE pos_payments SET payer_email=NULL WHERE id=?',payment.id);
  const fetcher=vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Idempotency-Key')).toBe(`pos-promptpay-${payment.id}`);
   expect(new URLSearchParams(String(init?.body)).get('payment_method_data[billing_details][email]')).toBe('restored@example.test');
   return new Response(JSON.stringify({id:'pi_restored_email',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'test-qr'}}}));
  });vi.stubGlobal('fetch',fetcher);
  await drainPos(db,'h001');expect(fetcher).not.toHaveBeenCalled();
  expect(await one(db,"SELECT state,last_error FROM pos_jobs WHERE record_id=? AND kind='checkout'",payment.id)).toEqual({state:'exception',last_error:'POS_PROMPTPAY_EMAIL_REQUIRED'});
  const input={id:payment.id,expectedVersion:1,payerEmail:'restored@example.test'};
  await command('payment.retry',input,actor,'same-email-recovery');await command('payment.retry',input,actor,'same-email-recovery');
  await expect(command('payment.retry',{...input,payerEmail:'another@example.test'})).rejects.toMatchObject({code:'POS_VERSION_CONFLICT'});
  await drainPos(db,'h001',1);
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(await one(db,'SELECT provider_id,payer_email,state FROM pos_payments WHERE id=?',payment.id)).toEqual({provider_id:'pi_restored_email',payer_email:'restored@example.test',state:'pending'});
  expect(await one(db,'SELECT COUNT(*) n FROM pos_payments')).toEqual({n:1});
  const current=(await one(db,'SELECT version FROM pos_payments WHERE id=?',payment.id))!;
  await expect(command('payment.retry',{id:payment.id,expectedVersion:current.version,payerEmail:'another@example.test'})).rejects.toMatchObject({code:'POS_PAYMENT_LOCKED'});
 });
 it('creates a bank-scannable PromptPay QR on the company account and settles only after provider verification',async()=>{
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES('pos-company','acct_original',false,true,true,true,true,'ready','daily',true)");
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  const sale=await createSale();await add(sale);
  const payment=await change('payment.create',sale,{method:'stripe_promptpay',amountMinor:10700,tenderedMinor:10700,payerEmail:'payer@example.test'});
  let paid=false;
  const fetcher=vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
   if(init?.method==='POST'){expect(new URLSearchParams(String(init.body)).get('payment_method_data[billing_details][email]')).toBe('payer@example.test');return new Response(JSON.stringify({id:'pi_promptpay',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201010212'}}}));}
   return new Response(JSON.stringify({id:'pi_promptpay',status:paid?'succeeded':'requires_action',amount_received:paid?10700:0,currency:'thb',metadata:{pos_payment_id:payment.id}}));
  });
  vi.stubGlobal('fetch',fetcher);await drainPos(db,'h001');
  expect(await one(db,'SELECT provider_id,qr_data,checkout_url,state FROM pos_payments WHERE id=?',payment.id)).toMatchObject({provider_id:'pi_promptpay',qr_data:'000201010212',checkout_url:null,state:'pending'});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_payments')).toEqual({n:0});
  paid=true;await run(db,"UPDATE pos_jobs SET next_attempt_at=0 WHERE kind='check_payment'");await drainPos(db,'h001');
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('completed');
  expect(await one(db,'SELECT COUNT(*) n FROM finance_payments')).toEqual({n:1});
  expect(fetcher.mock.calls.some(([url])=>String(url).includes('checkout/sessions'))).toBe(false);
 });
 it.each([true,false])('recovers cached legacy PromptPay validation errors only with definite non-creation evidence (%s)',async(definite)=>{
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES('pos-company','acct_original',false,true,true,true,true,'ready','daily',true)");
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  const sale=await createSale();await add(sale);const payment=await change('payment.create',sale,{method:'stripe_promptpay',amountMinor:10700,tenderedMinor:10700,payerEmail:'restored@example.test'});
  const keys:string[]=[];vi.stubGlobal('fetch',vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{
   const key=new Headers(init?.headers).get('Idempotency-Key')!;keys.push(key);
   const body=new URLSearchParams(String(init?.body));
   if(key.endsWith('-billing-email-v2'))return new Response(JSON.stringify({id:'pi_legacy_recovered',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'test-qr'}}}));
   if(body.has('payment_method_data[billing_details][email]')||body.has('receipt_email'))return new Response(JSON.stringify({error:{type:'idempotency_error'}}),{status:400});
   return definite?new Response(JSON.stringify({error:{type:'invalid_request_error',code:'parameter_missing',param:'billing_details[email]'}}),{status:400}):new Response(JSON.stringify({error:{type:'api_error'}}),{status:500});
  }));await drainPos(db,'h001',1);
  expect(keys.filter(key=>key.endsWith('-billing-email-v2'))).toHaveLength(definite?1:0);
  expect(await one(db,'SELECT COUNT(*) n FROM pos_payments')).toEqual({n:1});
  expect(await one(db,'SELECT state,provider_id FROM pos_payments WHERE id=?',payment.id)).toEqual(definite?{state:'pending',provider_id:'pi_legacy_recovered'}:{state:'requested',provider_id:null});
  if(!definite)expect(await one(db,"SELECT state,last_error FROM pos_jobs WHERE record_id=? AND kind='checkout'",payment.id)).toEqual({state:'exception',last_error:'POS_PROVIDER_LOOKUP_REQUIRED'});
 });
 it('stops a provider email-restricted refund with an actionable exception and no local refund posting',async()=>{
  const {sale,payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'provider email restriction test',lines:[{lineId:line.id,quantityMilli:500,restock:true}]});await command('refund.approve',{id:refund.id,expectedVersion:1},manager);
  const rp=(await one(db,'SELECT id FROM pos_refund_payments WHERE refund_id=?',refund.id))!;
  const before=await one(db,'SELECT COUNT(*) n FROM finance_journals');
  const returnsBefore=await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'");
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>String(url).includes('checkout/sessions/')?new Response(JSON.stringify({id:'cs_fixture',payment_intent:'pi_fixture'})):new Response(JSON.stringify({error:{type:'invalid_request_error',message:'Cannot send email. Verify the merchant email first.'}}),{status:400})));
  await drainPos(db,'h001');
  expect(await one(db,"SELECT state,last_error,attempts FROM pos_jobs WHERE record_id=? AND kind='refund'",rp.id)).toEqual({state:'exception',last_error:'POS_PROVIDER_REFUND_EMAIL_RESTRICTED',attempts:1});
  expect(await one(db,'SELECT state,provider_id FROM pos_refund_payments WHERE id=?',rp.id)).toEqual({state:'requested',provider_id:null});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(before);
  expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual(returnsBefore);
 });
 it('shows provider-confirmed accounting-pending payments to cashiers without revealing internal payment references',async()=>{
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  const {sale,payment}=await stripePending();
  const pending=await posWorkspace(actor,sale,'','sale',undefined,true);
  expect(pending.payments[0]).toMatchObject({id:payment,provider_payment_confirmed:false});
  await run(db,'UPDATE pos_payments SET provider_received_at=?,provider_id=? WHERE id=?',Date.now(),'cs_verified_provider',payment);
  const confirmed=await posWorkspace(actor,sale,'','sale',undefined,true);
  expect(confirmed.payments[0]).toMatchObject({id:payment,state:'pending',provider_payment_confirmed:true});
  for(const key of ['provider_received_at','provider_id','finance_payment_id'])expect(confirmed.payments[0]).not.toHaveProperty(key);
 });
 async function restrictedPromptPayRefund(){
  const {sale,payment}=await stripePending('stripe_promptpay');vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'customer supplied replacement refund email',lines:[{lineId:line.id,quantityMilli:500,restock:true}]});await command('refund.approve',{id:refund.id,expectedVersion:1},manager);
  const rp=(await one(db,'SELECT * FROM pos_refund_payments WHERE refund_id=?',refund.id))!;
  vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({error:{type:'invalid_request_error',message:'Cannot send email. Verify the merchant email first.'}}),{status:400})));await drainPos(db,'h001');
  const job=(await one(db,"SELECT * FROM pos_jobs WHERE kind='refund' AND record_id=?",rp.id))!;
  return {sale,payment,refund,rp,job};
 }
 it('reviews PromptPay instruction email corrections with Finance and version guards while preserving original snapshots',async()=>{
  const {sale,payment,refund,rp,job}=await restrictedPromptPayRefund();
  const frozen=await one(db,'SELECT md5(snapshot_json::text) digest FROM pos_sale_documents WHERE sale_id=?',sale),returns=await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'"),journals=await one(db,'SELECT COUNT(*) n FROM finance_journals');
  const input={id:job.id,accountingDate:businessDate(),reason:'customer controlled recipient correction',refundInstructionsEmail:'blocked@example.test',expectedRefundVersion:2};
  const posReviewOnly:WorkContext & Pick<PermissionActor,'permissions'>={...manager,permissions:['pos.approve']};
  await expect(command('exception.retry',input,posReviewOnly)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
  await expect(command('exception.retry',input,{...manager,branchId:'pos-branch'})).rejects.toMatchObject({code:'POS_COMPANY_SETTINGS'});
  await expect(command('exception.retry',{...input,expectedRefundVersion:1},manager)).rejects.toMatchObject({code:'POS_VERSION_CONFLICT'});
  await expect(command('exception.retry',{...input,refundInstructionsEmail:'payer@example.test'},manager)).rejects.toMatchObject({code:'POS_REFUND_EMAIL_UNCHANGED'});
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");expect((await posWorkspace(manager,undefined,'','reports')).jobs.find(j=>j.id===job.id)?.refund_version).toBe(2);
  await command('exception.retry',input,manager,'instruction-email-first');expect(await command('exception.retry',input,manager,'instruction-email-first')).toEqual({id:job.id});
  expect((await one(db,'SELECT version FROM pos_refunds WHERE id=?',refund.id))?.version).toBe(3);
  await drainPos(db,'h001');expect((await one(db,'SELECT state,last_error FROM pos_jobs WHERE id=?',job.id))).toEqual({state:'exception',last_error:'POS_PROVIDER_REFUND_EMAIL_RESTRICTED'});
  await command('exception.retry',{...input,refundInstructionsEmail:'allowed@example.test',expectedRefundVersion:3},manager,'instruction-email-second');
  const keys:string[]=[],emails:string[]=[];let status='pending';
  vi.stubGlobal('fetch',vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{
   if(init?.method==='POST'){const email=new URLSearchParams(String(init.body)).get('instructions_email');keys.push(new Headers(init.headers).get('Idempotency-Key')!);emails.push(email!);if(email!=='allowed@example.test')return new Response(JSON.stringify({error:{type:'invalid_request_error',message:'Cannot send email. Recipient is not permitted.'}}),{status:400});}
   return new Response(JSON.stringify({id:'re_corrected',amount:5350,status,metadata:{pos_refund_payment_id:rp.id}}));
  }));await drainPos(db,'h001');
  const originalKey=`pos-refund-${rp.id}-0`;expect(keys).toEqual([originalKey,`${originalKey}-instructions-1`,`${originalKey}-instructions-2`]);expect(emails).toEqual(['payer@example.test','blocked@example.test','allowed@example.test']);
  expect(await one(db,'SELECT payer_email FROM pos_payments WHERE id=?',payment)).toEqual({payer_email:'payer@example.test'});expect(await one(db,'SELECT md5(snapshot_json::text) digest FROM pos_sale_documents WHERE sale_id=?',sale)).toEqual(frozen);
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journals);expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual(returns);
  status='succeeded';await run(db,'UPDATE pos_jobs SET next_attempt_at=0 WHERE id=?',job.id);await drainPos(db,'h001');
  expect((await one(db,'SELECT state FROM pos_refunds WHERE id=?',refund.id))?.state).toBe('completed');expect(keys).toHaveLength(3);expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual({n:Number(journals!.n)+1});expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual(returns);
 });
 it('adopts an original provider refund instead of creating another after an email correction',async()=>{
  const {rp,job}=await restrictedPromptPayRefund();await command('exception.retry',{id:job.id,accountingDate:businessDate(),reason:'customer replacement recipient',refundInstructionsEmail:'allowed@example.test',expectedRefundVersion:2},manager);
  const keys:string[]=[];vi.stubGlobal('fetch',vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{if(init?.method==='POST')keys.push(new Headers(init.headers).get('Idempotency-Key')!);return new Response(JSON.stringify({id:'re_original',amount:5350,status:'succeeded',metadata:{pos_refund_payment_id:rp.id}}));}));await drainPos(db,'h001');
  expect(keys).toEqual([`pos-refund-${rp.id}-0`]);expect((await one(db,'SELECT state,provider_id FROM pos_refund_payments WHERE id=?',rp.id))).toEqual({state:'succeeded',provider_id:'re_original'});
 });
 it('never starts an email-corrected refund when the original provider outcome is unknown',async()=>{
  const {rp,job}=await restrictedPromptPayRefund(),journals=await one(db,'SELECT COUNT(*) n FROM finance_journals');await command('exception.retry',{id:job.id,accountingDate:businessDate(),reason:'customer replacement recipient',refundInstructionsEmail:'allowed@example.test',expectedRefundVersion:2},manager);
  const keys:string[]=[];vi.stubGlobal('fetch',vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);return new Response(JSON.stringify({error:{type:'api_error',message:'Unknown provider outcome'}}),{status:500});}));await drainPos(db,'h001');
  expect(keys).toEqual([`pos-refund-${rp.id}-0`]);expect(await one(db,'SELECT state,provider_id FROM pos_refund_payments WHERE id=?',rp.id)).toEqual({state:'requested',provider_id:null});expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journals);
 });
 async function stripePending(method:'stripe_card'|'stripe_promptpay'='stripe_card'){
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state) VALUES('pos-company','acct_original',false,true,true,true,true,'ready')");
  const sale=await createSale();await add(sale);
  const p=await change('payment.create',sale,{method,amountMinor:10700,tenderedMinor:10700,...(method==='stripe_promptpay'?{payerEmail:'payer@example.test'}:{})});
  await run(db,"UPDATE pos_jobs SET state='completed' WHERE kind='checkout'");
  await run(db,"UPDATE pos_payments SET provider_id=?,state='pending' WHERE id=?",method==='stripe_promptpay'?'pi_fixture':'cs_fixture',p.id);
  await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('check-fixture','pos-company','check_payment',?,0)",p.id);
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  return {sale,payment:String(p.id)};
 }
 function mockPaid(payment:string){return vi.fn(async(url: string|URL|Request,init?:RequestInit)=>{
  expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
  const body=String(url).includes('payment_intents/')?{id:'pi_fixture',status:'succeeded',amount_received:10700,currency:'thb',metadata:{pos_payment_id:payment}}:{id:'cs_fixture',payment_status:'paid',payment_intent:'pi_fixture'};
  return new Response(JSON.stringify(body),{status:200});
 });}
 it('package POS provider work refuses an unpurchased poll then settles draining funds without borrowing cashier authority',async()=>{
  const {sale,payment}=await stripePending('stripe_promptpay'),fetcher=mockPaid(payment);vi.stubGlobal('fetch',fetcher);vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
  await drainPos(db,'h001');expect(fetcher).not.toHaveBeenCalled();expect(await one(db,"SELECT state,last_error FROM pos_jobs WHERE id='check-fixture'")).toMatchObject({last_error:'PACKAGE_REQUIRED'});
  expect(await one(db,'SELECT state,provider_received_at FROM pos_payments WHERE id=?',payment)).toEqual({state:'pending',provider_received_at:null});expect(await one(db,'SELECT count(*) n FROM finance_payments')).toEqual({n:0});
  await packageGrant();await packageGrant('pos','draining');await run(db,"UPDATE assignments SET revoked_at=1 WHERE id='pos-cashier'");await run(db,"UPDATE pos_jobs SET next_attempt_at=0 WHERE id='check-fixture'");
  await drainPos(db,'h001');expect(fetcher).toHaveBeenCalled();expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'completed'});
  expect(await one(db,'SELECT quantity_milli FROM commerce_stock WHERE product_id=?','tea')).toEqual({quantity_milli:9000});expect(await one(db,'SELECT count(*) n FROM finance_payments')).toEqual({n:1});
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='check-fixture'");await drainPos(db,'h001');expect(await one(db,'SELECT count(*) n FROM finance_payments')).toEqual({n:1});expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
 });
 it('provider success survives account switch, revoked cashier and repeated polling without duplicate sale',async()=>{
  const {sale,payment}=await stripePending();await run(db,"UPDATE pos_connections SET account_id='acct_replacement'");await run(db,"UPDATE assignments SET revoked_at=1 WHERE id='pos-cashier'");
  vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('completed');
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='check-fixture'");await drainPos(db,'h001');
  expect(await one(db,'SELECT COUNT(*) n FROM finance_invoices')).toEqual({n:1});expect(await one(db,'SELECT COUNT(*) n FROM finance_payments')).toEqual({n:1});
 });
 it('paid at provider with a newly closed period enters review and resumes the same payment',async()=>{
  const {sale,payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await run(db,"UPDATE finance_periods SET state='closed'");await drainPos(db,'h001');
  expect((await one(db,"SELECT state,last_error FROM pos_jobs WHERE id='check-fixture'"))).toEqual({state:'exception',last_error:'POS_PERIOD_CLOSED'});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_payments')).toEqual({n:0});
  await run(db,"UPDATE finance_periods SET state='open'");await command('exception.retry',{id:'check-fixture',accountingDate:'2026-09-17',reason:'approved open period'},manager);await drainPos(db,'h001');
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('completed');
 });
 it('rejects webhook mode mismatch and deduplicates delivery on the company route',async()=>{
  await run(env.CONTROL_DB,"INSERT INTO pos_stripe_routes VALUES('acct_original',false,'h001','pos-company',0) ON CONFLICT DO NOTHING");
  vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_pos_fixture');vi.stubEnv('POS_STRIPE_LIVE_WEBHOOK_SECRET','whsec_live_fixture');
  const body=JSON.stringify({id:'evt_pos_fixture',type:'unhandled.test',account:'acct_original',livemode:false,data:{object:{id:'obj'}}}),t=Math.floor(Date.now()/1000),sign=(raw:string)=>`t=${t},v1=${createHmac('sha256','whsec_pos_fixture').update(`${t}.${raw}`).digest('hex')}`;
  await posStripeWebhook(body,sign(body));await posStripeWebhook(body,sign(body));expect(await one(db,"SELECT COUNT(*) n FROM pos_stripe_events WHERE id='evt_pos_fixture'")).toEqual({n:1});
  const live=body.replace('"livemode":false','"livemode":true');await expect(posStripeWebhook(live,sign(live))).rejects.toMatchObject({code:'POS_WEBHOOK_MODE'});
 });
 it('expired card checkout releases the unpaid reservation without cash, stock or accounting effects',async()=>{
  const {sale,payment}=await stripePending();
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:1});
  vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({id:'cs_fixture',status:'expired',payment_status:'unpaid'}))));
  await drainPos(db,'h001');
  expect(await one(db,'SELECT state FROM pos_payments WHERE id=?',payment)).toEqual({state:'cancelled'});
  expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'open'});
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:0});
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='check-fixture'");await drainPos(db,'h001');
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:10000,value_minor:30000});
  for(const table of ['finance_payments','finance_invoices','finance_journals','pos_stock_moves','pos_sale_documents'])expect(await one(db,`SELECT COUNT(*) n FROM ${table}`)).toEqual({n:0});
 });
 it.each(['expired','requires_payment_method'])('cancels %s PromptPay once and releases the unpaid bill without financial effects',async reason=>{
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES('pos-company','acct_original',false,true,true,true,true,'ready','daily',true)");
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  const sale=await createSale();await add(sale);
  const payment=await change('payment.create',sale,{method:'stripe_promptpay',amountMinor:10700,tenderedMinor:10700,payerEmail:'payer@example.test'});
  let status='requires_action';const cancelKeys:string[]=[];
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
   if(String(url).endsWith('/cancel')){cancelKeys.push(new Headers(init?.headers).get('Idempotency-Key')!);status='canceled';}
   return new Response(JSON.stringify({id:'pi_promptpay_expiry',status,next_action:{promptpay_display_qr_code:{data:'test-qr'}},amount_received:0,currency:'thb',metadata:{pos_payment_id:payment.id}}));
  }));
  await drainPos(db,'h001');
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:1});
  if(reason==='expired')await run(db,'UPDATE pos_payments SET expires_at=? WHERE id=?',Date.now()-1,payment.id);
  else status='requires_payment_method';
  await run(db,"UPDATE pos_jobs SET next_attempt_at=0 WHERE kind='check_payment'");await drainPos(db,'h001');
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE kind='check_payment'");await drainPos(db,'h001');
  expect(cancelKeys).toEqual([`pos-cancel-${payment.id}`]);
  expect(await one(db,'SELECT state FROM pos_payments WHERE id=?',payment.id)).toEqual({state:'cancelled'});
  expect(await one(db,'SELECT state FROM pos_sales WHERE id=?',sale)).toEqual({state:'open'});
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:0});
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:10000,value_minor:30000});
  for(const table of ['finance_payments','finance_invoices','finance_journals','pos_stock_moves','pos_sale_documents'])expect(await one(db,`SELECT COUNT(*) n FROM ${table}`)).toEqual({n:0});
 });
 it('reconciles newer refund callbacks before older payment/refund callbacks without duplicate accounting or stock',async()=>{
  const {sale,payment}=await stripePending();await packageGrant();vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');await packageGrant('pos','draining');
  const receipt=(await one(db,'SELECT snapshot_json FROM pos_sale_documents WHERE sale_id=?',sale))!;
  const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'partial provider return',lines:[{lineId:line.id,quantityMilli:500,restock:true}]});
  await command('refund.approve',{id:refund.id,expectedVersion:1},manager);
  const rp=(await one(db,'SELECT * FROM pos_refund_payments WHERE refund_id=?',refund.id))!;
  await run(db,"UPDATE pos_refund_payments SET provider_id='re_ordered',state='pending' WHERE id=?",rp.id);
  await run(db,"UPDATE pos_jobs SET state='completed' WHERE kind='refund'");
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
   const body=String(url).includes('/refunds/')?{id:'re_ordered',status:'succeeded',amount:5350,metadata:{pos_refund_payment_id:rp.id}}:String(url).includes('/payment_intents/')?{id:'pi_fixture',status:'succeeded',amount_received:10700,currency:'thb',metadata:{pos_payment_id:payment}}:{id:'cs_fixture',payment_status:'paid',payment_intent:'pi_fixture'};
   return new Response(JSON.stringify(body));
  }));
  await run(env.CONTROL_DB,"INSERT INTO pos_stripe_routes VALUES('acct_original',false,'h001','pos-company',0) ON CONFLICT DO NOTHING");
  vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_pos_fixture');
  const events=[
   {id:'evt_refund_newer',created:2,type:'refund.updated',data:{object:{id:'re_ordered',status:'succeeded',metadata:{pos_refund_payment_id:rp.id}}}},
   {id:'evt_payment_older',created:1,type:'checkout.session.completed',data:{object:{id:'cs_fixture',payment_status:'unpaid',metadata:{pos_payment_id:payment}}}},
   {id:'evt_refund_older',created:1,type:'refund.created',data:{object:{id:'re_ordered',status:'pending',metadata:{pos_refund_payment_id:rp.id}}}},
  ];
  for(const event of [...events,events[0],events[2]]){
   const raw=JSON.stringify({...event,account:'acct_original',livemode:false}),timestamp=Math.floor(Date.now()/1000);
   const signature=`t=${timestamp},v1=${createHmac('sha256','whsec_pos_fixture').update(`${timestamp}.${raw}`).digest('hex')}`;
   await posStripeWebhook(raw,signature);await drainPos(db,'h001');
  }
  expect(await one(db,"SELECT COUNT(*) n FROM pos_stripe_events WHERE applied_at IS NOT NULL")).toEqual({n:3});
  expect(await one(db,"SELECT COUNT(*) n FROM pos_jobs WHERE kind='event' AND state='completed'")).toEqual({n:3});
  expect(await one(db,'SELECT state FROM pos_refunds WHERE id=?',refund.id)).toEqual({state:'completed'});
  expect(await one(db,"SELECT COUNT(*) n FROM finance_payments WHERE kind='receipt'")).toEqual({n:1});
  expect(await one(db,"SELECT COUNT(*) n FROM finance_payments WHERE kind='refund'")).toEqual({n:1});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_invoices')).toEqual({n:1});
  expect(await one(db,'SELECT COUNT(*) n FROM pos_postings WHERE source_key=?',`refund-payment:${rp.id}`)).toEqual({n:1});
  expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual({n:1});
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:9500,value_minor:28500});
  expect(await one(db,'SELECT snapshot_json FROM pos_sale_documents WHERE sale_id=?',sale)).toEqual(receipt);
 });
 it('hosted onboarding accepts account-link responses without an object id',async()=>{
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_fixture');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://pos.example.test');
  const fetcher=vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).endsWith('/account_links')?{object:'account_link',url:'https://connect.stripe.com/setup/test',expires_at:2000000000}:String(url).includes('/external_accounts')?{data:[]}:{id:'acct_onboard_fixture',country:'TH',charges_enabled:false,payouts_enabled:false,capabilities:{},requirements:{currently_due:['business_profile.url']}})));
  vi.stubGlobal('fetch',fetcher);await command('connection.create',{livemode:false});await drainPos(db,'h001');
  const setup=(await one(db,"SELECT state,payload_json FROM pos_jobs WHERE kind='connect'"))!;
  expect(setup.state).toBe('completed');expect(JSON.parse(String(setup.payload_json))).toMatchObject({accountId:'acct_onboard_fixture',livemode:false,urlExpiresAt:2000000000000});expect(fetcher).toHaveBeenCalledTimes(4);
 });
 it('failed provider refund retries with a new attempt only after review and never restores stock twice',async()=>{
  const {sale,payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const line=(await one(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'partial return',lines:[{lineId:line.id,quantityMilli:500,restock:true}]});await command('refund.approve',{id:refund.id,expectedVersion:1},manager);
  const rp=(await one(db,'SELECT * FROM pos_refund_payments'))!;let succeed=false;const keys:string[]=[];
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
   if(String(url).includes('/checkout/'))return new Response(JSON.stringify({id:'cs_fixture',payment_intent:'pi_fixture'}));
   if(init?.method==='POST')keys.push(new Headers(init.headers).get('Idempotency-Key')!);
   return new Response(JSON.stringify({id:succeed?'re_second':'re_first',amount:5350,metadata:{pos_refund_payment_id:rp.id},status:succeed?'succeeded':'failed'}));
  }));
  await run(db,"UPDATE pos_connections SET account_id='acct_changed'");await drainPos(db,'h001');
  const job=(await one(db,"SELECT * FROM pos_jobs WHERE kind='refund'"))!;expect(job.state).toBe('exception');
  succeed=true;await command('exception.retry',{id:job.id,accountingDate:'2026-09-17',reason:'confirmed failed refund retry'},manager);await drainPos(db,'h001');
  expect(keys).toHaveLength(2);expect(keys[0]).not.toBe(keys[1]);expect((await one(db,'SELECT state FROM pos_refunds'))?.state).toBe('completed');expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual({n:1});
 });
 it('a payout groups charges and fees, posts once, and matches the existing bank statement',async()=>{
  const {payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const bt={id:'txn_fixture',currency:'thb',amount:10700,fee:300,net:10400,source:'ch_fixture',type:'charge'};
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).includes('/charges/')?{id:'ch_fixture',metadata:{pos_payment_id:payment}}:String(url).includes('balance_transactions?')?{data:[bt],has_more:false}:String(url).includes('balance_transactions/')?bt:{id:'po_fixture',currency:'thb',amount:10400,status:'paid'}))));
  await run(db,"INSERT INTO pos_stripe_events VALUES('evt_payout','acct_original',false,'pos-company','payout.paid',?,0,NULL)",JSON.stringify({id:'po_fixture'}));await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('job_payout','pos-company','event','evt_payout',0)");await drainPos(db,'h001');
  const payout=(await one(db,"SELECT * FROM pos_settlements WHERE kind='payout'"))!;expect(payout.state).toBe('paid');expect(await one(db,"SELECT SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE account_id='stripe'")).toEqual({balance:0});
  await run(db,"INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,source_hash,imported_by,created_at) VALUES('statement','pos-company','bank','POS test','2026-09-17','fixture','manager',0)");
  await run(db,"INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES('bank-line','statement','pos-company','2026-09-17','payout',10400,'THB')");
  await command('settlement.match',{id:payout.id,expectedVersion:1,bankTransactionId:'bank-line',bankVersion:1,reason:'bank statement verified'},manager);
  expect((await one(db,"SELECT matched_type FROM finance_bank_transactions WHERE id='bank-line'"))?.matched_type).toBe('pos_payout');
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='job_payout'");await drainPos(db,'h001');expect(await one(db,"SELECT COUNT(*) n FROM pos_settlements WHERE kind='payout'")).toEqual({n:1});
 });
 it('verifies a manual payout balance movement and requires an approved bank journal before matching',async()=>{
  const {payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const movement={id:'txn_manual_payout',currency:'thb',amount:-10400,fee:0,net:-10400,source:'po_manual',type:'payout'};
  let payoutStatus='paid';
  const fetcher=vi.fn(async(url:string|URL|Request)=>{
   expect(String(url)).not.toContain('balance_transactions?');
   return new Response(JSON.stringify(String(url).includes('balance_transactions/')?movement:{id:'po_manual',currency:'thb',amount:10400,status:payoutStatus,automatic:false,balance_transaction:movement.id}));
  });vi.stubGlobal('fetch',fetcher);
  await run(db,"INSERT INTO pos_stripe_events VALUES('evt_manual','acct_original',false,'pos-company','payout.paid',?,0,NULL)",JSON.stringify({id:'po_manual'}));
  await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('job_manual','pos-company','event','evt_manual',0)");
  const before=await one(db,'SELECT COUNT(*) n FROM finance_journals');await drainPos(db,'h001');
  const payout=(await one(db,"SELECT * FROM pos_settlements WHERE provider_id='po_manual'"))!;
  expect(payout.state).toBe('unmapped');expect(JSON.parse(String(payout.details_json))).toMatchObject({manualPayout:true,net:-10400,items:null,balanceTransaction:movement.id});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(before);
  await run(db,"INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,status,created_by,created_at,updated_at) VALUES('manual-j','pos-company','chart','period','MANUAL-PAYOUT','2026-09-17','THB',1000000,'manual','Verified payout','draft','cashier',0,0)");
  await run(db,"INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES('manual-bank','manual-j','bank',10400,0,10400,0,1),('manual-stripe','manual-j','stripe',0,10400,0,10400,2)");
  await run(db,"UPDATE finance_journals SET status='posted',posted_by='manager',posted_at=1 WHERE id='manual-j'");
  const input={id:payout.id,expectedVersion:1,journalId:'manual-j',reason:'verified provider balance movement and independently posted journal'};
  await run(db,"INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,status,created_by,created_at,updated_at) VALUES('manual-offset-j','pos-company','chart','period','MANUAL-PAYOUT-OFFSET','2026-09-17','THB',1000000,'manual','Incorrect bank offset','draft','cashier',0,0)");
  await run(db,"INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES('offset-bank-debit','manual-offset-j','bank',10400,0,10400,0,1),('offset-bank-credit','manual-offset-j','bank',0,100,0,100,2),('offset-stripe','manual-offset-j','stripe',0,10400,0,10400,3),('offset-cash','manual-offset-j','cash',100,0,100,0,4)");
  await run(db,"UPDATE finance_journals SET status='posted',posted_by='manager',posted_at=1 WHERE id='manual-offset-j'");
  await expect(command('settlement.map_external',{...input,journalId:'manual-offset-j'},manager)).rejects.toMatchObject({code:'POS_EXTERNAL_JOURNAL_MISMATCH'});
  expect((await one(db,'SELECT state,journal_id,version FROM pos_settlements WHERE id=?',payout.id))).toEqual({state:'unmapped',journal_id:null,version:1});
  await expect(command('settlement.map_external',input,{...manager,branchId:'pos-branch'})).rejects.toMatchObject({code:'POS_COMPANY_SETTINGS'});
  await expect(command('settlement.map_external',{...input,expectedVersion:2},manager)).rejects.toMatchObject({code:'POS_VERSION_CONFLICT'});
  await setFinanceCompanyTool(actor,{tool:'bank_automation',enabled:true,expectedVersion:0,reason:'Disposable native POS bank ownership fixture'},'pos-bank-tool-fixture');
  await run(db,"UPDATE finance_accounts SET currency='THB' WHERE id='bank'");
  const otherStatement=await importBankStatement(actor,{bankAccount:'bank',statementNumber:'OTHER-10400',statementDate:'2026-09-17',opening:'0',closing:'104.00',sourceHash:'different-native-bank-evidence',rows:[{externalId:'other-payout',transactionDate:'2026-09-17',amount:'104.00',currency:'THB'}]},'pos-other-bank-statement');
  const otherBank=String((await one(db,'SELECT id FROM finance_bank_transactions WHERE statement_id=?',otherStatement.id))!.id);
  const group=await proposeBankMatchGroup(actor,{bankTransactionIds:[otherBank],journalLineIds:['manual-bank'],reason:'Earlier generic bank match before native POS adoption'},'pos-earlier-bank-group');
  const staleGroup=await proposeBankMatchGroup(actor,{bankTransactionIds:[otherBank],journalLineIds:['manual-bank'],reason:'Earlier competing proposal retains its source before POS adoption'},'pos-earlier-stale-group');
  await decideBankMatchGroup(manager,String(group.id),{expectedVersion:1,reason:'Independent earlier bank evidence review'},'confirm','pos-earlier-bank-confirm');
  await command('settlement.map_external',input,manager,'manual-payout-map');
  expect((await one(db,"SELECT state,journal_id FROM pos_settlements WHERE id=?",payout.id))).toEqual({state:'paid',journal_id:'manual-j'});
  await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='job_manual'");await run(db,"UPDATE pos_stripe_events SET applied_at=NULL WHERE id='evt_manual'");await drainPos(db,'h001');
  expect((await one(db,"SELECT state,journal_id FROM pos_settlements WHERE id=?",payout.id))).toEqual({state:'paid',journal_id:'manual-j'});
  await run(db,"INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,source_hash,imported_by,created_at) VALUES('manual-statement','pos-company','bank','TEST ONLY','2026-09-17','manual-fixture','manager',0)");
  await run(db,"INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES('manual-bank-line','manual-statement','pos-company','2026-09-17','po_manual',10400,'THB')");
  const match={id:payout.id,expectedVersion:2,bankTransactionId:'manual-bank-line',bankVersion:1,reason:'sandbox statement matched to genuine payout reference'};
  await expect(command('settlement.match',match,manager,'manual-payout-bank-blocked')).rejects.toMatchObject({code:'POS_CASH_JOURNAL_ALREADY_MATCHED'});
  expect(await one(db,"SELECT match_status,version FROM finance_bank_transactions WHERE id='manual-bank-line'")).toEqual({match_status:'unmatched',version:1});
  expect(await one(db,'SELECT version,bank_transaction_id FROM pos_settlements WHERE id=?',payout.id)).toEqual({version:2,bank_transaction_id:null});
  await decideBankMatchGroup(manager,String(group.id),{expectedVersion:2,reason:'Release the earlier cash claim before native POS bank confirmation'},'reverse','pos-earlier-bank-reverse');
  await expect(decideBankMatchGroup(manager,String(staleGroup.id),{expectedVersion:1,reason:'Old proposal must not consume the now adopted POS cash journal'},'confirm','pos-earlier-stale-confirm')).rejects.toMatchObject({code:'BANK_TARGET_UNAVAILABLE'});
  const available=await bankAutomationWorkspace(actor);
  expect(available.targets.map(row=>row.id)).not.toContain('manual-bank');
  expect(available.suggestions.flatMap(row=>row.journalLineIds??[])).not.toContain('manual-bank');
  await expect(proposeBankMatchGroup(actor,{bankTransactionIds:[otherBank],journalLineIds:['manual-bank'],reason:'POS adoption already owns this cash journal'},'pos-adopted-bank-rejected')).rejects.toMatchObject({code:'BANK_TARGET_UNAVAILABLE'});
  await command('settlement.match',match,manager,'manual-payout-bank-match');
  expect(await command('settlement.match',match,manager,'manual-payout-bank-match')).toEqual({id:payout.id});
  expect((await one(db,"SELECT matched_type FROM finance_bank_transactions WHERE id='manual-bank-line'"))?.matched_type).toBe('pos_payout');
  const matchedJournalCount=(await one(db,'SELECT COUNT(*) n FROM finance_journals'))!.n;
  payoutStatus='failed';await run(db,"INSERT INTO pos_stripe_events VALUES('evt_manual_failed','acct_original',false,'pos-company','payout.failed',?,0,NULL)",JSON.stringify({id:'po_manual'}));await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('job_manual_failed','pos-company','event','evt_manual_failed',0)");await drainPos(db,'h001');
  expect((await one(db,"SELECT state FROM pos_settlements WHERE id=?",payout.id))?.state).toBe('failed');
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual({n:Number(matchedJournalCount)+1});
  await run(db,"UPDATE pos_stripe_events SET applied_at=NULL WHERE id='evt_manual_failed'");await run(db,"UPDATE pos_jobs SET state='pending',next_attempt_at=0 WHERE id='job_manual_failed'");await drainPos(db,'h001');
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual({n:Number(matchedJournalCount)+1});
 });
 it('refuses a manual payout whose verified balance transaction belongs to another source',async()=>{
  await stripePending();await run(db,"UPDATE pos_jobs SET state='completed'");vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).includes('balance_transactions/')?{id:'txn_wrong',currency:'thb',amount:-10400,fee:0,net:-10400,type:'payout',source:'po_other'}:{id:'po_manual',currency:'thb',amount:10400,status:'paid',automatic:false,balance_transaction:'txn_wrong'}))));
  await run(db,"INSERT INTO pos_stripe_events VALUES('evt_bad_manual','acct_original',false,'pos-company','payout.paid',?,0,NULL)",JSON.stringify({id:'po_manual'}));await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('job_bad_manual','pos-company','event','evt_bad_manual',0)");await drainPos(db,'h001');
  expect(await one(db,"SELECT state,last_error FROM pos_jobs WHERE id='job_bad_manual'")).toEqual({state:'exception',last_error:'POS_MANUAL_PAYOUT_BALANCE_MISMATCH'});expect(await one(db,"SELECT COUNT(*) n FROM pos_settlements WHERE kind='payout'")).toEqual({n:0});
 });
 it('refreshes verified PromptPay py_ fees, preserves payment postings, and records each fee once',async()=>{
  const {payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  const bt={id:'txn_prompt_fee',currency:'thb',amount:10700,fee:190,net:10510,source:'py_prompt',type:'payment'};
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
   expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
   return new Response(JSON.stringify(String(url).includes('/charges/')?{id:'py_prompt',metadata:{pos_payment_id:payment}}:String(url).includes('balance_transactions?')?{data:[bt],has_more:false}:bt));
  }));
  const before=await one(db,'SELECT COUNT(*) n FROM finance_payments');
  const input={expectedVersion:1,accountingDate:'2026-09-17',reason:'recover delayed provider fee'};
  const key='pos-fee-refresh-idempotent';const sync=await command('settlement.refresh',input,manager,key);
  expect(await command('settlement.refresh',input,manager,key)).toEqual(sync);
  await drainPos(db,'h001');
  expect(await one(db,"SELECT state,fee_minor FROM pos_settlements WHERE provider_id='txn_prompt_fee'")).toEqual({state:'recorded',fee_minor:190});
  expect(await one(db,"SELECT SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE account_id='fees'")).toEqual({balance:190});
  expect(await one(db,'SELECT COUNT(*) n FROM finance_payments')).toEqual(before);
  const journalCount=await one(db,'SELECT COUNT(*) n FROM finance_journals');
  await command('settlement.refresh',input,manager);await drainPos(db,'h001');
  expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journalCount);
  await expect(command('settlement.refresh',{...input,expectedVersion:2},manager)).rejects.toMatchObject({code:'POS_VERSION_CONFLICT'});
  await expect(command('settlement.refresh',input,{...manager,branchId:'pos-branch'})).rejects.toMatchObject({code:'POS_COMPANY_SETTINGS'});
 });
 it('adopts a known PromptPay pyr_ refund and records its owned balance without a duplicate payout',async()=>{
  const {sale,refund,rp,job}=await restrictedPromptPayRefund();
  const frozen=await one(db,'SELECT md5(snapshot_json::text) digest FROM pos_sale_documents WHERE sale_id=?',sale),returns=await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'");
  const attached={id:job.id,providerId:'pyr_owned_promptpay',accountingDate:businessDate(),reason:'verified existing PromptPay refund'};
  const before=await one(db,'SELECT COUNT(*) n FROM finance_journals');
  await command('exception.attach_provider',attached,manager,'attach-promptpay-pyr');expect(await command('exception.attach_provider',attached,manager,'attach-promptpay-pyr')).toEqual({id:job.id});
  const calls:string[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{expect(init?.method).not.toBe('POST');calls.push(String(url));return new Response(JSON.stringify({id:'pyr_owned_promptpay',amount:5350,status:'succeeded',metadata:{pos_refund_payment_id:rp.id}}));}));await drainPos(db,'h001');
  expect(calls.every(url=>url.endsWith('/refunds/pyr_owned_promptpay'))).toBe(true);
  expect(await one(db,'SELECT state FROM pos_refunds WHERE id=?',refund.id)).toEqual({state:'completed'});expect(await one(db,'SELECT state,provider_id FROM pos_refund_payments WHERE id=?',rp.id)).toEqual({state:'succeeded',provider_id:'pyr_owned_promptpay'});
  const journalCount=await one(db,'SELECT COUNT(*) n FROM finance_journals');expect(journalCount).toEqual({n:Number(before!.n)+1});
  const owned={id:'txn_owned_prompt_refund',currency:'thb',amount:-5350,fee:0,net:-5350,source:'pyr_owned_promptpay',type:'refund'},foreign={...owned,id:'txn_foreign_prompt_refund',source:'pyr_foreign_promptpay'};
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).includes('balance_transactions?')?{data:[owned,foreign],has_more:false}:String(url).endsWith(owned.id)?owned:foreign))));
  const input={expectedVersion:1,accountingDate:businessDate(),reason:'verify PromptPay refund balance ownership'};await command('settlement.refresh',input,manager);await drainPos(db,'h001');
  expect(await one(db,'SELECT state,journal_id FROM pos_settlements WHERE provider_id=?',owned.id)).toEqual({state:'recorded',journal_id:null});expect(await one(db,'SELECT state FROM pos_settlements WHERE provider_id=?',foreign.id)).toEqual({state:'unmapped'});
  await command('settlement.refresh',input,manager);await drainPos(db,'h001');expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journalCount);
  expect(await one(db,'SELECT md5(snapshot_json::text) digest FROM pos_sale_documents WHERE sale_id=?',sale)).toEqual(frozen);expect(await one(db,"SELECT COUNT(*) n FROM pos_stock_moves WHERE kind='return'")).toEqual(returns);
 });
 it('refuses fee synchronization after its connected account binding changes',async()=>{
  await stripePending();
  await run(db,"UPDATE pos_jobs SET state='completed' WHERE kind='check_payment'");
  await command('settlement.refresh',{expectedVersion:1,accountingDate:'2026-09-17',reason:'binding continuity test'},manager);
  await run(db,"UPDATE pos_connections SET account_id='acct_other' WHERE company_id='pos-company'");
  const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);await drainPos(db,'h001');
  expect(fetcher).not.toHaveBeenCalled();
  expect(await one(db,"SELECT state,last_error FROM pos_jobs WHERE kind='balance_sync'")).toEqual({state:'exception',last_error:'POS_CONNECTION_AUTHORITY_CHANGED'});
 });
 it('waits for a charge balance transaction instead of marking its fee event applied prematurely',async()=>{
  const {payment}=await stripePending();vi.stubGlobal('fetch',mockPaid(payment));await drainPos(db,'h001');
  let ready=false;const bt={id:'txn_delayed_fee',currency:'thb',amount:10700,fee:190,net:10510,source:'py_delayed',type:'payment'};
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).includes('/charges/')?{id:'py_delayed',metadata:{pos_payment_id:payment},balance_transaction:ready?bt.id:null}:bt))));
  await run(db,"INSERT INTO pos_stripe_events VALUES('evt_delayed_charge','acct_original',false,'pos-company','charge.succeeded',?,0,NULL)",JSON.stringify({id:'py_delayed'}));
  await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('job_delayed_charge','pos-company','event','evt_delayed_charge',0)");await drainPos(db,'h001');
  expect(await one(db,"SELECT applied_at FROM pos_stripe_events WHERE id='evt_delayed_charge'")).toEqual({applied_at:null});
  expect(await one(db,"SELECT state FROM pos_jobs WHERE id='job_delayed_charge'")).toEqual({state:'pending'});
  ready=true;await run(db,"UPDATE pos_jobs SET next_attempt_at=0 WHERE id='job_delayed_charge'");await drainPos(db,'h001');
  expect((await one(db,"SELECT applied_at FROM pos_stripe_events WHERE id='evt_delayed_charge'"))?.applied_at).not.toBeNull();
  expect(await one(db,"SELECT state FROM pos_jobs WHERE id='job_delayed_charge'")).toEqual({state:'completed'});
  expect(await one(db,"SELECT fee_minor,state FROM pos_settlements WHERE provider_id='txn_delayed_fee'")).toEqual({fee_minor:190,state:'recorded'});
 });
 it('full manual advance refund releases the reservation and lets the original bill be edited',async()=>{
  const sale=await createSale();await add(sale);await pay(sale);
  const r=await change('refund.request',sale,{amountMinor:10700,reason:'customer changed mind',payoutShiftId:'shift-retail'});
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");
  expect((await posWorkspace(actor,sale,'','sale',undefined,true)).refunds).toMatchObject([{id:r.id,sale_id:sale,state:'requested',amount_minor:10700}]);
  const otherSale=await createSale();
  expect((await posWorkspace(actor,otherSale,'','sale',undefined,true)).refunds).toEqual([]);
  expect((await posWorkspace(actor,sale,'','reports',undefined,true)).refunds).toEqual([]);
  await command('refund.approve',{id:r.id,expectedVersion:1,reason:'cash returned in full'},manager);
  expect((await posWorkspace(actor,sale,'','sale',undefined,true)).refunds).toMatchObject([{id:r.id,state:'completed'}]);
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:0});expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('open');await add(sale);
 });
 it('reports actual return cost and disallows discount preparer self approval',async()=>{
  const sale=await createSale();await change('sale.add',sale,{productId:'tea',quantityMilli:1000,discountMinor:1000},manager);
  await expect(change('sale.approve_discount',sale,{reason:'approve my discount'},manager)).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  await change('sale.approve_discount',sale,{reason:'independent review'});await pay(sale,9630);await change('sale.complete',sale);
  const {posWorkspace}=await import("@erpx/server-pos/public/pos");const data=await posWorkspace(actor,sale);
  expect(data.reports[0].cost_minor).toBe(3000);expect(data.reports[0].gross_profit_minor).toBe(6000);expect(data.paymentMix[0].received_minor).toBe(9630);
 });
});
describe('POS monetary and webhook boundaries',()=>{
 it('allocates every cent across repeated partial refunds',()=>{expect(recipeQuantities([{productId:'milk',quantityMilli:200000,conversionMicros:1000}],1000)).toEqual([{productId:'milk',quantityMilli:200}]);expect(portion(100,0,1,3)+portion(100,1,1,3)+portion(100,2,1,3)).toBe(100);expect(saleAmounts(1000,10700,700,true,0)).toEqual({subtotal:10000,tax:700,total:10700,discount:0});});
 it('requires a fresh, exact signed webhook body',()=>{const now=Date.now(),t=Math.floor(now/1000),body='{"id":"evt_one"}',secret='whsec_test',signature=`t=${t},v1=${createHmac('sha256',secret).update(`${t}.${body}`).digest('hex')}`;expect(verifyPosStripeSignature(body,signature,[secret],now)).toBe(true);expect(verifyPosStripeSignature(body+' ',signature,[secret],now)).toBe(false);expect(verifyPosStripeSignature(body,signature,[secret],now+301000)).toBe(false);});
});

it('quality quarantine blocks a reserved retail payment without consuming stock or cash',async()=>{
 const {createQualityHold}=await import("@erpx/server-quality/public/quality");
 const sale=await createSale();await add(sale);
 const stock=await one(db,'SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?','warehouse','tea');
 await createQualityHold(manager,{warehouseId:'warehouse',productId:'tea',expectedStockVersion:Number(stock!.version),dueDate:'2026-09-22',reason:'Packaging damage requires inspection'},'quality-pos-hold-command');
 const receiptsBefore=await one(db,'SELECT count(*) n FROM pos_receipts');
 await expect(pay(sale)).rejects.toMatchObject({code:'QUALITY_HOLD'});
 expect((await one(db,'SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?','warehouse','tea'))).toMatchObject({quantity_milli:10000,value_minor:30000});
 expect(await one(db,'SELECT count(*) n FROM pos_payments WHERE sale_id=?',sale)).toEqual({n:0});
 expect(await one(db,'SELECT count(*) n FROM pos_receipts')).toEqual(receiptsBefore);
 const {posWorkspace}=await import("@erpx/server-pos/public/pos");const workspace=await posWorkspace(actor,undefined,'','sale');
 expect(workspace.stock.find(s=>s.product_id==='tea')?.quality_held).toBe(1);
});

it('bootstraps a named restaurant while omitted mode still selects retail',async()=>{
 await clearPosSetup();
 const result=await command('settings.bootstrap',{mode:'restaurant',registerName:'Practice kitchen',warehouseId:'warehouse'},actor,'restaurant-bootstrap');
 expect(await one(db,'SELECT name,mode,warehouse_id FROM pos_registers WHERE id=?',result.registerId)).toMatchObject({name:'Practice kitchen',mode:'restaurant',warehouse_id:'warehouse'});
 expect(await command('settings.bootstrap',{mode:'restaurant',registerName:'Practice kitchen',warehouseId:'warehouse'},actor,'restaurant-bootstrap')).toEqual(result);
 expect(await one(db,'SELECT count(*) n FROM pos_shifts')).toEqual({n:0});
});

it('rejects duplicate table names while replaying the successful table command',async()=>{
 const input={registerId:'restaurant',name:'Table 1',seats:4};
 const saved=await command('table.create',input,actor,'setup-table-first');
 expect(await command('table.create',input,actor,'setup-table-first')).toEqual(saved);
 await expect(command('table.create',{...input,name:' Table 1 '},actor,'setup-table-duplicate')).rejects.toMatchObject({code:'POS_TABLE_NAME_EXISTS'});
 expect(await one(db,"SELECT count(*) n FROM pos_tables WHERE register_id='restaurant'")).toEqual({n:1});
});

it('replays simultaneous copies of one command without adding a second line',async()=>{
 const sale=await createSale(),input={id:sale,expectedVersion:1,productId:'tea',quantityMilli:1000,modifierIds:[],discountMinor:0,note:''};
 const results=await Promise.all([command('sale.add',input,actor,'same-in-flight-command'),command('sale.add',input,actor,'same-in-flight-command')]);
 expect(results[0]).toEqual(results[1]);expect(await one(db,'SELECT COUNT(*) n FROM pos_lines WHERE sale_id=?',sale)).toEqual({n:1});
});

it('rejects ingredients and products excluded from this register catalog',async()=>{
 const sale=await createSale();
 await expect(add(sale,'milk')).rejects.toMatchObject({code:'POS_PRODUCT_INVALID'});
 await run(db,"DELETE FROM pos_register_catalog WHERE register_id='retail' AND product_id='tea'");
 await expect(add(sale)).rejects.toMatchObject({code:'POS_PRODUCT_INVALID'});
 await run(db,"INSERT INTO pos_register_catalog(register_id,product_id) VALUES('retail','tea')");
 await run(db,"UPDATE pos_catalog SET enabled=false WHERE product_id='tea'");
 await expect(add(sale)).rejects.toMatchObject({code:'POS_PRODUCT_INVALID'});
});

it('checks the draft catalog version and price before changing the bill',async()=>{
 const sale=await createSale();
 await expect(change('sale.add',sale,{productId:'tea',quantityMilli:1000,expectedCatalogVersion:2,expectedPriceMinor:10000})).rejects.toMatchObject({code:'POS_CATALOG_CHANGED'});
 await expect(change('sale.add',sale,{productId:'tea',quantityMilli:1000,expectedCatalogVersion:1,expectedPriceMinor:9999})).rejects.toMatchObject({code:'POS_CATALOG_CHANGED'});
 expect(await one(db,'SELECT COUNT(*) n FROM pos_lines WHERE sale_id=?',sale)).toEqual({n:0});
});

it('preserves line and bill discounts exactly when a quantity is split',async()=>{
 const sale=await createSale();await add(sale,'tea',3000);await change('sale.discount',sale,{amountMinor:101});
 const line=(await one(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale))!;
 const split=await change('sale.split',sale,{lineId:line.id,quantityMilli:1000});
 const portions=await all(db,'SELECT discount_minor,bill_discount_minor FROM pos_lines WHERE sale_id IN (?,?)',sale,split.id);
 expect(portions.reduce((sum,l)=>sum+Number(l.discount_minor),0)).toBe(101);expect(portions.reduce((sum,l)=>sum+Number(l.bill_discount_minor),0)).toBe(101);
 await change('sale.discount',String(split.id),{amountMinor:0});
 expect(await one(db,'SELECT discount_minor FROM pos_sales WHERE id=?',split.id)).toEqual({discount_minor:0});
});

it('shares CRM parties, refuses duplicate contact identities, and freezes receipt data',async()=>{
 const customer=await command('customer.create',{name:'Snapshot Customer',phone:'0801234567',email:'receipt@example.test'});
 await expect(command('customer.create',{name:'Different name',phone:'080-123-4567'})).rejects.toMatchObject({code:'POS_CUSTOMER_DUPLICATE'});
 expect((await posCustomers(db,actor,'receipt@example.test')).map(p=>p.id)).toContain(customer.id);
 const sale=await createSale();await change('sale.customer',sale,{partyId:customer.id});await add(sale);await pay(sale);await change('sale.complete',sale);
 const before=(await one(db,'SELECT snapshot_json FROM pos_sale_documents WHERE sale_id=?',sale))!;
 const journalCount=await one(db,'SELECT COUNT(*) n FROM finance_journals');
 await run(db,'UPDATE finance_parties SET name=? WHERE id=?','Later changed name',customer.id);
 expect((await one(db,'SELECT snapshot_json FROM pos_sale_documents WHERE sale_id=?',sale))!.snapshot_json).toEqual(before.snapshot_json);
 await expect(run(db,'UPDATE pos_sale_documents SET created_at=created_at+1 WHERE sale_id=?',sale)).rejects.toMatchObject({code:'23514'});
 expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journalCount);
});

async function reviewShifts(){
 for(const id of ['shift-retail','shift-kitchen']){
  const cash=(await one(db,"SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=? AND p.method='cash' AND p.state='succeeded'",id))!;
  const shift=(await one(db,'SELECT * FROM pos_shifts WHERE id=?',id))!;
  await command('shift.close',{id,expectedVersion:shift.version,countedMinor:10000+Number(cash.n)});
  const closed=(await one(db,'SELECT version FROM pos_shifts WHERE id=?',id))!;
  await command('shift.review',{id,expectedVersion:closed.version,reason:'Independent cash count'},manager);
 }
}

it('blocks close for open sales and requires independent Finance without reposting revenue',async()=>{
 const date=businessDate(),sale=await createSale();await add(sale);
 expect((await posDayReport(db,manager,'pos-branch',date)).blockers.map(b=>b.code)).toContain('OPEN_SALE');
 await expect(command('day.prepare',{branchId:'pos-branch',businessDate:date},manager)).rejects.toMatchObject({code:'POS_DAY_BLOCKED'});
 await pay(sale);await change('sale.complete',sale);await reviewShifts();
 const journals=await one(db,'SELECT COUNT(*) n FROM finance_journals');
 const prepared=await command('day.prepare',{branchId:'pos-branch',businessDate:date},manager);
 await expect(command('day.confirm',{id:prepared.id,expectedVersion:1},manager)).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
 await command('day.confirm',{id:prepared.id,expectedVersion:1});
 expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(journals);
 await expect(run(db,"UPDATE pos_day_closes SET snapshot_json='{}'::jsonb WHERE id=?",prepared.id)).rejects.toMatchObject({code:'23514'});
 await expect(command('shift.open',{registerId:'retail',openingMinor:0})).rejects.toMatchObject({code:'POS_DAY_CLOSED'});
});

it('validates denomination totals and rejects the person who counted the drawer as reviewer',async()=>{
 await expect(command('shift.close',{id:'shift-retail',expectedVersion:1,countedMinor:10000,denominations:{10000:2}})).rejects.toMatchObject({code:'POS_CASH_COUNT'});
 await command('shift.close',{id:'shift-retail',expectedVersion:1,countedMinor:10000,denominations:{5000:2}});
 await expect(command('shift.review',{id:'shift-retail',expectedVersion:2,reason:'Self review'})).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
 expect(await one(db,"SELECT denominations_json->>'5000' count FROM pos_shifts WHERE id='shift-retail'")).toEqual({count:'2'});
});

it('keeps prepared day evidence stable until Finance confirms before a later-day refund',async()=>{
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(new Date('2026-09-30T12:00:00Z'));
 const date=businessDate();
 // These isolated fixture shifts were inserted before this test fixed its clock.
 await run(db,"UPDATE pos_shifts SET business_date=? WHERE id IN ('shift-retail','shift-kitchen')",date);
 const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);await reviewShifts();
 const prepared=await command('day.prepare',{branchId:'pos-branch',businessDate:date},manager);
 const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!;
 vi.setSystemTime(new Date('2026-10-01T12:00:00Z'));
 const refund={reason:'Later-day original-cost return',lines:[{lineId:line.id,quantityMilli:1000,restock:true}]};
 await expect(change('refund.request',sale,refund)).rejects.toMatchObject({code:'POS_DAY_REVIEW_PENDING'});
 expect(await one(db,'SELECT COUNT(*) n FROM pos_refunds WHERE sale_id=?',sale)).toEqual({n:0});
 await command('day.confirm',{id:prepared.id,expectedVersion:1});
 const result=await change('refund.request',sale,refund);
 expect(await one(db,'SELECT accounting_date,tax_minor FROM pos_refunds WHERE id=?',result.id)).toMatchObject({accounting_date:'2026-10-01',tax_minor:700});
});

it('keeps daily reports within the authorized company and branch',async()=>{
 await expect(posDayReport(db,{...actor,branchId:'another-branch'},'pos-branch',businessDate())).rejects.toMatchObject({code:'POS_NOT_FOUND'});
 await expect(posDayReport(db,{...actor,companyId:'another-company'},'pos-branch',businessDate())).rejects.toMatchObject({code:'POS_NOT_FOUND'});
});

it('drills a POS report group into exactly its completed source receipts with Company and Branch isolation',async()=>{
 const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);
 const report=await posWorkspace(actor,undefined,'','reports'),group=report.reports.find(row=>row.register_id==='retail')!;
 const detail=await posReportReceipts(actor,{businessDate:String(group.business_date),registerId:String(group.register_id),shiftId:String(group.shift_id),currency:String(group.currency)});
 expect(detail.rows.map(row=>row.id)).toContain(sale);
 expect(Number(detail.row.total_minor)).toBe(Number(group.total_minor));
 expect(detail.rows.every(row=>row.state==='completed'&&row.register_id===group.register_id&&row.shift_id===group.shift_id&&row.currency===group.currency&&row.business_date===group.business_date)).toBe(true);
 await expect(posReportReceipts({...actor,companyId:'other-company'},{businessDate:String(group.business_date),registerId:String(group.register_id),shiftId:String(group.shift_id),currency:String(group.currency)})).rejects.toMatchObject({status:404});
 await expect(posReportReceipts({...actor,branchId:'other-branch'},{businessDate:String(group.business_date),registerId:String(group.register_id),shiftId:String(group.shift_id),currency:String(group.currency)})).rejects.toMatchObject({status:404});
});

describe('POS presentation movement query',()=>{
 it('resolves the original sale only within its Company and optional Branch without mutating owner records',async()=>{
  const {findPosMovementSale}=await import("@erpx/server-pos/public/movement-presentation-query");
  const sale=await createSale();await add(sale);await pay(sale);await change('sale.complete',sale);
  const movement=await one(db,'SELECT m.id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id WHERE l.sale_id=? ORDER BY m.created_at LIMIT 1',sale);
  expect(movement).not.toBeNull();
  const movementId=String(movement!.id),before=await all(db,'SELECT row_to_json(s)::text row FROM pos_sales s ORDER BY id');
  expect(await findPosMovementSale(db,movementId,actor.companyId,null)).toEqual({sale_id:sale});
  expect(await findPosMovementSale(db,movementId,actor.companyId,'pos-branch')).toEqual({sale_id:sale});
  expect(await findPosMovementSale(db,movementId,actor.companyId,'other-branch')).toBeNull();
  expect(await findPosMovementSale(db,movementId,'other-company',null)).toBeNull();
  expect(await findPosMovementSale(db,'missing-movement',actor.companyId,null)).toBeNull();
  expect(await all(db,'SELECT row_to_json(s)::text row FROM pos_sales s ORDER BY id')).toEqual(before);
 });
});

it('keeps POS setup independent from Finance, and applies only the selected assignment grants',async()=>{
 await run(db,"INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES('operations-only','operations-only','pos_manager','[\"pos\"]','pos-company','holding',0,9999999999999),('finance-only','finance-only','finance_approver','[\"finance\"]','pos-company','holding',0,9999999999999)");
 const operations={...actor,userId:'operations-only',assignmentId:'operations-only',role:'pos_manager',modules:['pos']};
 const finance={...actor,userId:'finance-only',assignmentId:'finance-only',role:'finance_approver',modules:['finance']};
 await expect(command('register.save',{warehouseId:'warehouse',name:'Operations-only register',mode:'retail'},operations)).resolves.toHaveProperty('id');
 await expect(command('accounting.prepare',{},operations)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 await expect(command('room_charge_reversal.approve_pos',{id:'unreadable',expectedVersion:1,reason:'Finance role'},finance)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 await expect(command('room_charge_reversal.approve_pms',{id:'unreadable',expectedVersion:1,reason:'Finance role'},finance)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 await expect(posWorkspace(finance,undefined,'','settings')).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 const financial=await posWorkspace(finance,undefined,'','settings',undefined,false,true);
 expect(financial.permissions['pos.configure']).toBe(false);expect(financial.permissions['pos.accounting.approve']).toBe(true);
 expect(financial.products).toEqual([]);expect(financial.kitchen).toEqual([]);
 await expect(command('settings.operations',{expectedVersion:2,enabled:true,cashierDiscountBps:100},finance)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 await expect(command('settings.operations',{expectedVersion:2,enabled:true,cashierDiscountBps:100},operations)).resolves.toHaveProperty('id');
 const otherBranch={...operations,branchId:'different-branch',assignmentScope:'branch',scope:{...operations.scope,kind:'branch'}} as WorkContext;
 await expect(command('register.save',{id:'retail',warehouseId:'warehouse',name:'Scope bypass',mode:'retail',expectedVersion:1},otherBranch)).rejects.toMatchObject({status:404});
 await run(db,"UPDATE assignments SET revoked_at=? WHERE id='operations-only'",Date.now());
 await expect(command('settings.operations',{expectedVersion:3,enabled:false,cashierDiscountBps:0},operations)).rejects.toMatchObject({code:'POS_ACCESS_CHANGED'});
});
it('allows independent shops to sell while a hotel closes and locks only its linked restaurant',async()=>{
 await run(db,"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,created_by,updated_at) VALUES('closing-hotel','pos-company','pos-branch','Closing hotel',?,'closing',?,'system',0)",businessDate(),JSON.stringify(defaultConfig));
 await run(db,"UPDATE pos_registers SET property_id='closing-hotel' WHERE id='restaurant'");
 await expect(createSale('retail')).resolves.toEqual(expect.any(String));
 await expect(createSale('dine_in')).rejects.toMatchObject({code:'POS_HOTEL_DAY_CLOSED'});
 const {hospitalityDayControls}=await import("@erpx/server-pms/public/hospitality-day-controls");
 const controls=await hospitalityDayControls(db,'pos-company','pos-branch','closing-hotel',businessDate());
 expect(controls.openShifts).toBe(1);expect(controls.openBills).toBe(0);
});

it('replays a committed immediate reversal receipt without entering the new workflow or reposting stock',async()=>{
 const input={id:'historical-sale',expectedVersion:4,propertyId:'historical-property',expectedPropertyVersion:8,reason:'Historical correction',restockLineIds:[]};
 const result={id:'historical-sale',state:'room_charge_reversed',guestAccountEntryId:'original-entry',reversalEntryId:'original-reversal',restoredCostMinor:3000};
 const digest=createHash('sha256').update(JSON.stringify({user:manager.userId,assignment:manager.assignmentId,company:manager.companyId,branch:manager.branchId,operation:'sale.reverse_room_charge',input:legacyGuestAccount(input)})).digest('hex');
 await run(db,'INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) VALUES(?,?,?,?,1)','historical-reversal-receipt',actor.companyId,digest,JSON.stringify(legacyGuestAccount(result)));
 const before=await one(db,'SELECT count(*) n FROM finance_journals');
 expect(await command('sale.reverse_room_charge',input,manager,'historical-reversal-receipt')).toEqual(canonicalGuestAccount(result));
 expect(await one(db,'SELECT count(*) n FROM finance_journals')).toEqual(before);
 expect(await one(db,'SELECT count(*) n FROM pos_room_charge_reversal_requests')).toEqual({n:0});
 await expect(command('sale.reverse_room_charge',{...input,reason:'Different input'},manager,'historical-reversal-receipt')).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
});
// Organization/worker authority is a diagnostic native fixture only. Every
// ledger, mapping, master, catalogue and sale must use its real owner command.
async function zeroLedgerPosCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-pos-${randomUUID()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger POS fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native POS team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<WorkContext>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,?,?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,JSON.stringify(['pos']),company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:WorkContext['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'pos-test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native POS team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'pos-test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'pos-test',holdingName:'Test',companyId:company};
    const context:WorkContext={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'pos-test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:['pos'],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),salesManager=await actor('sales-manager','sales_manager'),treasury=await actor('treasury','treasury');
  const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');
  await installStaffLevels(db,owner,randomUUID());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),randomUUID())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[salesManager,3],[treasury,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),randomUUID())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices','pos_settings','pos_registers','commerce_warehouses','commerce_products'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0,0,0,0,0]);
  const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-zero-ledger-POS-provider'}),obligations:async()=>{throw Error('ZERO_POS_FIXTURE_CANNOT_WITHDRAW');}}),grant={companyId:company,packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated POS-only foundation',evidence:'test:zero-ledger-POS'};
  async function purchase(){await store.apply({expectedVersion:0,idempotencyKey:randomUUID(),grant});}
  vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  return {company,branch,department,owner,reviewer,manager,salesManager,treasury,counts,purchase,store,grant,actor};
}


it('retains original POS first-time bootstrap missing-mapping guard without effects',async()=>{
 const z=await zeroLedgerPosCompany();await z.purchase();expect((await db.prepare("SELECT package_id,state FROM company_package_grants WHERE company_id=? AND state='active' ORDER BY package_id").bind(z.company).all()).results).toEqual([{package_id:'pos',state:'active'}]);const before=await z.counts();await expect(posCommand(db,z.owner,'settings.bootstrap',{},randomUUID())).rejects.toMatchObject({status:409,code:'POS_COMMERCE_SETUP_REQUIRED'});expect(await z.counts()).toEqual(before);
});
it('POS-only zero ledger uses actual startup references, nine accounts, masters and independent accounting before its first service cash sale',async()=>{
 const z=await zeroLedgerPosCompany(),finance=await import('@erpx/server-finance/public/pos-startup-prerequisites'),commerce=await import('@erpx/server-commerce/public/pos-posting-setup'),masters=await import('@erpx/server-commerce/public/pos-startup-masters'),startup=await import('@erpx/server-pos/public/startup');
 vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');const denied=await finance.posStartupPrerequisites(z.reviewer);expect(denied).toMatchObject({available:false,unavailableReason:'purchase-required',charts:[],accounts:[]});const empty=await z.counts();await expect(finance.createPosStartupChart(z.reviewer,{expectedVersion:0,contextKey:'a'.repeat(64),input:{name:'Unauthorized chart',baseCurrency:'THB',accountingBasis:'TFRS'}},randomUUID())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await z.counts()).toEqual(empty);await z.purchase();
 const request=async()=>{const value=await finance.posStartupPrerequisites(z.reviewer);expect(value.available).toBe(true);return {expectedVersion:0 as const,contextKey:value.contextKey!};};
 const chartInput={...await request(),input:{name:'Actual POS Company chart',baseCurrency:'THB',accountingBasis:'TFRS'}},chartKey=randomUUID(),chart=await finance.createPosStartupChart(z.reviewer,chartInput,chartKey);expect(await finance.createPosStartupChart(z.reviewer,chartInput,chartKey)).toEqual(chart);
 const specs=[['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank']] as const,accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of specs){const account=await finance.createPosStartupAccount(z.reviewer,{...await request(),input:{chartId:chart.id,code,name:`Actual POS ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},randomUUID());accounts[name]=account.id;}
 await finance.createPosStartupPeriod(z.reviewer,{...await request(),input:{name:'Actual POS current period',startDate:'2020-01-01',endDate:'2035-12-31'}},randomUUID());const rateSeed=await finance.posStartupPrerequisites(z.manager);expect(rateSeed.canConfirmRate).toBe(true);await finance.confirmPosStartupRate(z.manager,{expectedVersion:0,contextKey:rateSeed.contextKey!,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Independently confirmed native rate'}},randomUUID());
 const mapping=await commerce.posPostingSetup(z.reviewer);expect(mapping).toMatchObject({available:true,canConfigure:true,configured:false,expectedSalesDefaults:null});const input={expectedVersion:0 as const,contextKey:mapping.contextKey!,expectedSalesDefaults:null,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Deliberate first POS Company nine-role mapping'},mappingKey=randomUUID(),saved=await commerce.initializePosPostingSetup(z.reviewer,input,mappingKey);expect(saved).toEqual({id:z.company,status:'configured',version:1});expect(await commerce.initializePosPostingSetup(z.reviewer,input,mappingKey)).toEqual(saved);
 const ready=await startup.posStartupWorkspace(z.owner);expect(ready).toMatchObject({available:true,canCreateWarehouse:true,canCreateProduct:true,canBootstrap:false});expect(ready.branches.map(row=>row.id)).toContain(z.branch);const warehouse=await masters.createPosStartupWarehouse(z.owner,{branchId:z.branch,code:'MAIN',name:'Actual Company POS warehouse',status:'active'},randomUUID()),product=await masters.createPosStartupProduct(z.owner,{sku:'CONSULT',name:'Actual POS service',itemType:'service',trackingMode:'none',unit:'visit',price:'100',taxBps:0,status:'active'},randomUUID());expect(await db.prepare('SELECT count(*) n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?').bind(z.company).first<number>('n')).toBe(0);
 const eligible=await startup.posStartupWorkspace(z.owner);expect(eligible.canBootstrap).toBe(true);const setupKey=randomUUID(),setup=await posCommand(db,z.owner,'settings.bootstrap',{warehouseId:warehouse.id,bankAccountId:accounts.cash},setupKey);expect(await posCommand(db,z.owner,'settings.bootstrap',{warehouseId:warehouse.id,bankAccountId:accounts.cash},setupKey)).toEqual(setup);await expect(posCommand(db,z.owner,'accounting.approve',{expectedVersion:1},randomUUID())).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});const review=await startup.posStartupWorkspace(z.manager);expect(review.canApproveAccounting).toBe(true);await posCommand(db,z.manager,'accounting.approve',{expectedVersion:review.settingsVersion},randomUUID());expect((await posWorkspace(z.manager,undefined,'','settings',undefined,false,true)).permissions['pos.accounting.approve']).toBe(true);
 await posCommand(db,z.owner,'catalog.save',{productId:product.id,barcode:'',category:'Services',kind:'service',station:'Shop',recipe:[],modifiers:[],taxInclusive:false,registerIds:[setup.registerId],expectedVersion:0},randomUUID());const shift=await posCommand(db,z.owner,'shift.open',{registerId:setup.registerId,openingMinor:0},randomUUID()),sale=await posCommand(db,z.owner,'sale.create',{shiftId:shift.id,serviceMode:'retail'},randomUUID()),saleId=String(sale.id),mutate=async(operation:string,extra:Record<string,unknown>={})=>{const current=await db.prepare('SELECT version FROM pos_sales WHERE id=? AND company_id=?').bind(saleId,z.company).first<{version:number}>();return posCommand(db,z.owner,operation,{id:saleId,expectedVersion:current!.version,...extra},randomUUID());};await mutate('sale.add',{productId:product.id,quantityMilli:1000,modifierIds:[],discountMinor:0,note:'Actual first service'});await mutate('payment.create',{method:'cash',amountMinor:10000,tenderedMinor:10000,reference:''});await mutate('sale.complete');
 expect(await db.prepare('SELECT state,currency,total_minor FROM pos_sales WHERE id=? AND company_id=?').bind(saleId,z.company).first()).toMatchObject({state:'completed',currency:'THB',total_minor:10000});expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE company_id=? AND source_type='pos' AND source_id=? AND status='posted'").bind(z.company,saleId).first<number>('n')).toBeGreaterThan(0);expect(await db.prepare('SELECT count(*) n FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?').bind(z.company).first<number>('n')).toBe(0);expect((await finance.posStartupPrerequisites(z.reviewer)).accounts.every(account=>typeof account.currency==='string')).toBe(true);expect((await startup.posStartupWorkspace(z.owner)).accountingApproved).toBe(true);expect((await db.prepare("SELECT package_id FROM company_package_grants WHERE company_id=? AND state='active' ORDER BY package_id").bind(z.company).all()).results).toEqual([{package_id:'pos'}]);
},120000);

async function zeroLedgerPosInitialStockCompany(){
 const z=await zeroLedgerPosCompany(),finance=await import('@erpx/server-finance/public/pos-startup-prerequisites'),commerce=await import('@erpx/server-commerce/public/pos-posting-setup'),masters=await import('@erpx/server-commerce/public/pos-startup-masters');await z.purchase();
 const request=async()=>{const seed=await finance.posStartupPrerequisites(z.reviewer);if(!seed.contextKey)throw Error('Actual POS Finance prerequisite context required');return {expectedVersion:0 as const,contextKey:seed.contextKey};};
 const chart=await finance.createPosStartupChart(z.reviewer,{...await request(),input:{name:'Physical POS chart',baseCurrency:'THB',accountingBasis:'TFRS'}},randomUUID()),accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank'],['variance','5400','expense','none']] as const){const account=await finance.createPosStartupAccount(z.reviewer,{...await request(),input:{chartId:chart.id,code,name:`Physical POS ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},randomUUID());accounts[name]=account.id;}
 await finance.createPosStartupPeriod(z.reviewer,{...await request(),input:{name:'Actual physical observation period',startDate:'2020-01-01',endDate:'2035-12-31'}},randomUUID());const rateSeed=await finance.posStartupPrerequisites(z.manager);if(!rateSeed.contextKey)throw Error('Actual rate approver context required');await finance.confirmPosStartupRate(z.manager,{expectedVersion:0,contextKey:rateSeed.contextKey,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Normal independent POS fixture rate confirmation'}},randomUUID());
 const seed=await commerce.posPostingSetup(z.reviewer);if(!seed.contextKey)throw Error('Actual POS posting context required');const mappingKey=randomUUID(),mapping={expectedVersion:0 as const,contextKey:seed.contextKey,expectedSalesDefaults:null,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Deliberate original POS-only stock posting map'};await commerce.initializePosPostingSetup(z.reviewer,mapping,mappingKey);
 const warehouse=await masters.createPosStartupWarehouse(z.owner,{branchId:z.branch,code:'PHYSICAL',name:'Actual first physical POS warehouse',status:'active'},randomUUID()),product=await masters.createPosStartupProduct(z.owner,{sku:'STOCKED',name:'Actual first stocked POS item',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},randomUUID()),bootstrapKey=randomUUID(),setup=await posCommand(db,z.owner,'settings.bootstrap',{warehouseId:warehouse.id,bankAccountId:accounts.cash},bootstrapKey);await posCommand(db,z.manager,'accounting.approve',{expectedVersion:1},randomUUID());
 const snapshot=async()=>{const tables=['inventory_counts','inventory_count_lines','inventory_count_decisions','inventory_adjustment_moves','inventory_ledger','commerce_stock','commerce_stock_moves','finance_journals','finance_journal_lines','finance_financial_events','money_transaction_snapshots','command_receipts','audit_events','outbox_events'];return Object.fromEntries(await Promise.all(tables.map(async table=>[table,(await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image'))])));};
 expect((await db.prepare("SELECT package_id FROM company_package_grants WHERE company_id=? AND state='active' ORDER BY package_id").bind(z.company).all()).results).toEqual([{package_id:'pos'}]);expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);
 return {z,chart,accounts,warehouse,product,mappingKey,bootstrapKey,setup,snapshot,input:{warehouseId:warehouse.id,countDate:'2026-10-17',reason:'Actual first physical observation at current-period variance',lines:[{productId:product.id,countedQuantity:'2',unitCost:'6',allocations:[]}]}};
}

it('POS-only first physical Count retains original Supply refusal then binds evidence, independently posts variance and funds its first stocked cash sale',async()=>{
 const s=await zeroLedgerPosInitialStockCompany(),counts=await import('@erpx/server-inventory/public/inventory-counts'),initial=await import('@erpx/server-inventory/public/pos-initial-physical-stock-count'),startup=await import('@erpx/server-pos/public/startup'),before=await s.snapshot();
 expect(s.z.manager.role).toBe('finance_approver');expect(counts.canPrepareCounts(s.z.manager)).toBe(false);expect(counts.canPrepareCounts(s.z.owner)).toBe(true);expect(counts.canApproveCounts(s.z.manager)).toBe(true);expect(s.z.owner.userId).not.toBe(s.z.manager.userId);
 await expect(counts.saveCount(s.z.owner,null,s.input,randomUUID())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await s.snapshot()).toEqual(before);
 const selected={warehouseId:s.warehouse.id,productId:s.product.id,countDate:s.input.countDate},seed=await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,selected);expect(seed).toMatchObject({available:true,canPrepare:true,canCreateWarehouse:false,canCreateProduct:false,readiness:{schemaAvailable:true,postingConfigured:true,accountingApproved:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});expect(await startup.posStartupWorkspace(s.z.manager)).toMatchObject({available:true,canPrepareInitialStock:false});expect(await startup.posStartupWorkspace(s.z.owner)).toMatchObject({available:true,canPrepareInitialStock:true});if(!seed.contextKey)throw Error('Actual selected POS Count context required');
 const createKey=randomUUID(),createBody={expectedVersion:0 as const,contextKey:seed.contextKey,input:s.input},created=await initial.preparePosInitialPhysicalStockCount(s.z.owner,createBody,createKey),id=String(created.id);expect(created).toEqual({id:`count-${createKey}`,status:'draft',version:1});expect(await initial.preparePosInitialPhysicalStockCount(s.z.owner,createBody,createKey)).toEqual(created);
 const privateSource=await db.prepare('SELECT proof_json FROM inventory_pos_initial_physical_sources WHERE count_id=? AND company_id=?').bind(id,s.z.company).first<{proof_json:string}>();if(!privateSource)throw Error('Actual POS initial Count private source required');expect(JSON.parse(privateSource.proof_json)).toMatchObject({kind:'pos-initial-physical-stock-count',posSetupCommandId:s.mappingKey,posBootstrapReceiptId:s.bootstrapKey,chartId:s.chart.id,inventoryAccountId:s.accounts.inventory,varianceAccountId:s.accounts.variance,currency:'THB'});
 const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionPosInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},randomUUID());
 for(const [step,recordVersion] of [['submitted',2],['approved',3]] as const){const flow=await initial.posInitialPhysicalStockCountFlow(s.z.owner,id);await initial.assignPosInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Current original Company Flow authority chooses a different actual Stock reviewer',saveDefault:false},randomUUID());if(step==='submitted')await initial.transitionPosInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independent physical observation verifies quantity two and unit cost six'},randomUUID());}
 const postKey=randomUUID(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Original Count current-period physical variance, never an opening balance or acquisition'},posted=await initial.transitionPosInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});expect(await initial.transitionPosInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);
 const record=await initial.posInitialPhysicalStockCountWorkspace(s.z.manager,{countId:id});expect(record.record).toMatchObject({sourceKind:'pos-initial-physical-stock-count',id,status:'posted',version:4});expect(Number(record.record?.countedQuantity)).toBe(2);expect(Number(record.record?.countedValue)).toBe(12);const journalId=record.record?.journalId;if(!journalId)throw Error('Actual independently posted Count journal required');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});expect(await db.prepare('SELECT source_type,source_id,status,is_opening FROM finance_journals WHERE id=? AND company_id=?').bind(journalId,s.z.company).first()).toMatchObject({source_type:'stock_count',source_id:id,status:'posted',is_opening:0});expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(journalId).all()).results).toEqual(expect.arrayContaining([{account_id:s.accounts.inventory,debit_minor:1200,credit_minor:0},{account_id:s.accounts.variance,debit_minor:0,credit_minor:1200}]));
 const {journalDetail}=await import('@erpx/server-finance/public/ledger');expect((await journalDetail(s.z.reviewer,journalId)).journal).toMatchObject({id:journalId,source_type:'stock_count',source_id:id,status:'posted'});
 await posCommand(db,s.z.owner,'catalog.save',{productId:s.product.id,barcode:'',category:'Physical stock',kind:'stock',station:'Shop',recipe:[],modifiers:[],taxInclusive:false,registerIds:[s.setup.registerId],expectedVersion:0},randomUUID());const shift=await posCommand(db,s.z.owner,'shift.open',{registerId:s.setup.registerId,openingMinor:0},randomUUID()),sale=await posCommand(db,s.z.owner,'sale.create',{shiftId:shift.id,serviceMode:'retail'},randomUUID()),saleId=String(sale.id),mutate=async(operation:string,extra:Record<string,unknown>={})=>{const row=await db.prepare('SELECT version FROM pos_sales WHERE id=? AND company_id=?').bind(saleId,s.z.company).first<{version:number}>();if(!row)throw Error('Actual current POS sale required');return posCommand(db,s.z.owner,operation,{id:saleId,expectedVersion:row.version,...extra},randomUUID());};await mutate('sale.add',{productId:s.product.id,quantityMilli:1000,modifierIds:[],discountMinor:0,note:'The first actual physically counted unit'});await mutate('payment.create',{method:'cash',amountMinor:10000,tenderedMinor:10000,reference:''});await mutate('sale.complete');
 expect(await db.prepare('SELECT state,currency,total_minor FROM pos_sales WHERE id=? AND company_id=?').bind(saleId,s.z.company).first()).toEqual({state:'completed',currency:'THB',total_minor:10000});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:1000,value_minor:600});expect(await db.prepare('SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type=\'pos\' AND source_id=? AND status=\'posted\'').bind(s.z.company,saleId).first('n')).toBeGreaterThan(0);
 const consumed=await s.snapshot();expect(await initial.preparePosInitialPhysicalStockCount(s.z.owner,createBody,createKey)).toEqual(created);expect(await initial.transitionPosInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await s.snapshot()).toEqual(consumed);expect((await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,selected)).unavailableReason).toBe('prior-stock-history');expect((await db.prepare("SELECT package_id FROM company_package_grants WHERE company_id=? AND state='active' ORDER BY package_id").bind(s.z.company).all()).results).toEqual([{package_id:'pos'}]);
},120000);

it('POS-only retained first physical Counts survive normal unused variance-account reclassification while original posting still refuses unavailable 5400',async()=>{
 const s=await zeroLedgerPosInitialStockCompany(),initial=await import('@erpx/server-inventory/public/pos-initial-physical-stock-count'),counts=await import('@erpx/server-inventory/public/inventory-counts'),finance=await import('@erpx/server-finance/public/finance'),masters=await import('@erpx/server-commerce/public/pos-startup-masters');
 const second=await masters.createPosStartupProduct(s.z.owner,{sku:'RETAINED-SECOND',name:'Second actual observed POS stock item',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},randomUUID());
 const prepare=async(productId:string)=>{const input={...s.input,lines:[{...s.input.lines[0],productId}]},seed=await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,{warehouseId:s.warehouse.id,productId,countDate:input.countDate});expect(seed.canPrepare).toBe(true);if(!seed.contextKey)throw Error('Actual selected POS Count context required');const key=randomUUID(),body={expectedVersion:0 as const,contextKey:seed.contextKey,input},result=await initial.preparePosInitialPhysicalStockCount(s.z.owner,body,key);expect(result).toEqual({id:`count-${key}`,status:'draft',version:1});return {id:String(result.id),key,body,result};};
 const draft=await prepare(s.product.id),reviewable=await prepare(second.id),sourceImage=async(id:string)=>db.prepare('SELECT proof_json::text proof FROM inventory_pos_initial_physical_sources WHERE count_id=? AND company_id=?').bind(id,s.z.company).first<string>('proof'),draftProof=await sourceImage(draft.id),reviewableProof=await sourceImage(reviewable.id);
 // This is the normal Finance import's original structural edit of an unused
 // account. There is no code-recode API; no SQL repair or fabricated identity.
 expect(await db.prepare('SELECT count(*)::int n FROM finance_journal_lines WHERE account_id=?').bind(s.accounts.variance).first('n')).toBe(0);
 const reclassified=await finance.importAccounts(s.z.reviewer,s.chart.id,[{code:'5400',name:'Unused expense reclassified through original Finance owner',category:'other_expense',currency:'THB',controlKind:'none'}],randomUUID());expect(reclassified).toMatchObject({imported:1,chartId:s.chart.id});
 expect(await db.prepare('SELECT id,code,category,currency,status,version FROM finance_accounts WHERE id=? AND company_id=?').bind(s.accounts.variance,s.z.company).first()).toEqual({id:s.accounts.variance,code:'5400',category:'other_expense',currency:'THB',status:'active',version:2});
 const retainedDraft=await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,{countId:draft.id});expect(retainedDraft.record).toMatchObject({id:draft.id,sourceKind:'pos-initial-physical-stock-count',status:'draft',version:1,canCancel:true});expect(retainedDraft.currency).toBe('THB');expect(await sourceImage(draft.id)).toBe(draftProof);expect(await initial.preparePosInitialPhysicalStockCount(s.z.owner,draft.body,draft.key)).toEqual(draft.result);
 const cancelKey=randomUUID(),cancelBody={expectedVersion:1,attachmentSetIds:[],reason:'Cancel the retained original draft after an authorized Finance account change'},cancelled=await initial.transitionPosInitialPhysicalStockCount(s.z.owner,draft.id,'cancel',cancelBody,cancelKey);expect(cancelled).toEqual({id:draft.id,status:'cancelled',version:2,journalId:null});expect(await initial.transitionPosInitialPhysicalStockCount(s.z.owner,draft.id,'cancel',cancelBody,cancelKey)).toEqual(cancelled);expect((await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,{countId:draft.id})).record).toMatchObject({id:draft.id,status:'cancelled',version:2});expect(await sourceImage(draft.id)).toBe(draftProof);
 const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:reviewable.id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionPosInitialPhysicalStockCount(s.z.owner,reviewable.id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},randomUUID());
 for(const [step,recordVersion] of [['submitted',2],['approved',3]] as const){const flow=await initial.posInitialPhysicalStockCountFlow(s.z.owner,reviewable.id);await initial.assignPosInitialPhysicalStockCountFlow(s.z.owner,reviewable.id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Existing Company workflow authority assigns independent actual Stock reviewer',saveDefault:false},randomUUID());if(step==='submitted')await initial.transitionPosInitialPhysicalStockCount(s.z.manager,reviewable.id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'The independent physical observation remains valid despite missing posting readiness'},randomUUID());}
 const snapshot=async()=>({original:await s.snapshot(),sources:(await db.prepare("SELECT COALESCE(jsonb_agg(to_jsonb(source) ORDER BY source.count_id),'[]'::jsonb)::text image FROM inventory_pos_initial_physical_sources source").first<string>('image')),assertions:(await db.prepare("SELECT COALESCE(jsonb_agg(to_jsonb(assertion) ORDER BY to_jsonb(assertion)::text),'[]'::jsonb)::text image FROM command_assertions assertion").first<string>('image'))}),before=await snapshot(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Original post must require its currently valid expense variance account'};
 await expect(counts.transitionCount(s.z.manager,reviewable.id,'post',postBody,randomUUID())).rejects.toMatchObject({status:409,code:'VARIANCE_MAPPING_REQUIRED'});expect(await snapshot()).toEqual(before);
 await expect(initial.transitionPosInitialPhysicalStockCount(s.z.manager,reviewable.id,'post',postBody,randomUUID())).rejects.toMatchObject({status:409,code:'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await snapshot()).toEqual(before);
 await expect(counts.saveCount(s.z.owner,null,s.input,randomUUID())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await snapshot()).toEqual(before);
 expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=?').bind(s.warehouse.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_journals WHERE company_id=?').bind(s.z.company).first('n')).toBe(0);expect((await initial.posInitialPhysicalStockCountWorkspace(s.z.manager,{countId:reviewable.id})).record).toMatchObject({id:reviewable.id,status:'approved',version:3});
 await initial.transitionPosInitialPhysicalStockCount(s.z.manager,reviewable.id,'reject',{expectedVersion:3,attachmentSetIds:[],reason:'Return the reviewed observation because its original posting account is no longer eligible'},randomUUID());await initial.transitionPosInitialPhysicalStockCount(s.z.owner,reviewable.id,'cancel',{expectedVersion:4,attachmentSetIds:[],reason:'Cancel the returned unposted observation without replacing original economics'},randomUUID());
 expect((await initial.posInitialPhysicalStockCountWorkspace(s.z.owner,{countId:reviewable.id})).record).toMatchObject({id:reviewable.id,status:'cancelled',version:5});expect(await sourceImage(reviewable.id)).toBe(reviewableProof);expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=?').bind(s.warehouse.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_journals WHERE company_id=?').bind(s.z.company).first('n')).toBe(0);expect((await db.prepare("SELECT package_id FROM company_package_grants WHERE company_id=? AND state='active' ORDER BY package_id").bind(s.z.company).all()).results).toEqual([{package_id:'pos'}]);
});

// Native addon withdrawal: normal Commercial Preview/Apply, actual owner work.
const posAddonInput=(packageId:'pos-restaurant'|'pos-hotel-charge')=>({companyId:actor.companyId,packageId,packageVersion:1});
const posAddonObservation=(packageId:'pos-restaurant'|'pos-hotel-charge')=>withOwnerPackageObligations(db,posAddonInput(packageId),tx=>readOwnerPackageObligations(db,tx,{...posAddonInput(packageId),now:Date.now()}));
const posAddonStore=()=>packageGrantStore(db,{authorize:async()=>({userId:'isolated-addon-withdrawal-reviewer'}),obligations:(tx,company,packageId)=>ownerPackageObligations(db,tx,company,packageId)});
async function posAddonWithdrawal(packageId:'pos-restaurant'|'pos-hotel-charge'){
 const grants=await readCompanyPackageGrants(db,actor.companyId),current=grants.find(grant=>grant.packageId===packageId)!;
 return {companyId:actor.companyId,expectedRevision:packageGrantRevision(actor.companyId,grants),idempotencyKey:randomUUID(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Native addon work independently settled',evidence:'test:exact-hospitality-addon-owner'}}]};
}
const posAddonReview=(packageId:'pos-restaurant'|'pos-hotel-charge',request:Awaited<ReturnType<typeof posAddonWithdrawal>>,apply=false)=>withOwnerPackageObligations(db,posAddonInput(packageId),tx=>apply?posAddonStore().batch.applyIn(tx,request,async()=>{}):posAddonStore().batch.previewIn(tx,request));
async function closeAddonRestaurantShift(){
 const cash=(await one(db,"SELECT COALESCE(SUM(p.amount_minor),0) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id='shift-kitchen' AND p.method='cash' AND p.state='succeeded'"))!;
 const refunds=(await one(db,"SELECT COALESCE(SUM(rp.amount_minor),0) n FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.payout_shift_id='shift-kitchen' AND p.method='cash' AND rp.state='succeeded'"))!;
 const current=(await one(db,"SELECT version FROM pos_shifts WHERE id='shift-kitchen'"))!;
 await command('shift.close',{id:'shift-kitchen',expectedVersion:current.version,countedMinor:10000+Number(cash.n)-Number(refunds.n)});
 const closed=(await one(db,"SELECT version FROM pos_shifts WHERE id='shift-kitchen'"))!;
 await command('shift.review',{id:'shift-kitchen',expectedVersion:closed.version,reason:'Independent addon till review'},manager);
}
async function posAddonHistory(){
 return Promise.all(['pos_sales','pos_lines','pos_fulfillments','pos_reservations','pos_stock_moves','pos_payments','pos_refunds','pos_refund_payments','pos_jobs','pos_stripe_events','pos_receipts','pos_postings','pms_properties','pms_records','pms_sources','pms_postings','finance_journals','finance_journal_lines','finance_invoices','finance_payments','money_transaction_snapshots'].map(table=>all(db,`SELECT row_to_json(source)::text image FROM ${table} source ORDER BY row_to_json(source)::text`)));
}
it('restaurant addon withdrawal settles the original recipe, refund and till while independent retail work remains active',async()=>{
 await packageGrant();await packageGrant('pos-restaurant');await packageGrant('pos-hotel-charge');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
 const retail=await createSale();await add(retail);
 const independentAdvance=await createSale();await add(independentAdvance);await pay(independentAdvance);
 const createBody={shiftId:'shift-kitchen',serviceMode:'dine_in'},createKey=randomUUID(),created=await command('sale.create',createBody,actor,createKey),sale=String(created.id);
 await add(sale,'latte');await kitchen(sale,'send');
 const initial=await posAddonObservation('pos-restaurant');expect(initial.observations.map(row=>row.moduleId)).toEqual(['pos','finance','inventory']);expect(initial.observations.find(row=>row.moduleId==='pos')!.outstanding).toBeGreaterThan(0);expect(initial.observations.find(row=>row.moduleId==='inventory')!.outstanding).toBeGreaterThan(0);expect(JSON.stringify(initial)).not.toMatch(/Cashier|Latte|accounts_json|cashier@example/);
 await packageGrant('pos-restaurant','draining');let withdrawal=await posAddonWithdrawal('pos-restaurant');await expect(posAddonReview('pos-restaurant',withdrawal)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(posAddonReview('pos-restaurant',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await expect(createSale('restaurant')).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await kitchen(sale,'start');await kitchen(sale,'ready');await kitchen(sale,'deliver');await pay(sale,12840);
 expect((await posAddonObservation('pos-restaurant')).observations.find(row=>row.moduleId==='finance')!.outstanding).toBeGreaterThan(0);
 await change('sale.complete',sale);
 const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!,refund=await change('refund.request',sale,{reason:'Guest returned the prepared drink',payoutShiftId:'shift-kitchen',lines:[{lineId:line.id,quantityMilli:1000,restock:false}]});
 const evidence=await seedAttachmentSet(db,manager,{targetType:'pos_refund',targetId:String(refund.id),componentKind:'action',componentRef:'refund-decision',purposeKey:'refund_evidence',expectedTargetVersion:1});
 await expect(posAddonReview('pos-restaurant',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const refundBody={id:refund.id,expectedVersion:1,reason:'Cash returned and original receipt reviewed',attachmentSetIds:[evidence.setId]},refundKey=randomUUID(),refunded=await command('refund.approve',refundBody,manager,refundKey);
 await closeAddonRestaurantShift();expect((await posAddonObservation('pos-restaurant')).total).toBe(0);
 expect(await one(db,"SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id='pos-company' AND l.account_id='advance' AND j.status='posted'")).toEqual({balance:-10700});

 expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',retail))!.state).toBe('open');expect((await one(db,"SELECT state FROM pos_shifts WHERE id='shift-retail'"))!.state).toBe('open');expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='milk'")).toEqual({quantity_milli:9800});
 withdrawal=await posAddonWithdrawal('pos-restaurant');const before=await posAddonHistory();expect((await posAddonReview('pos-restaurant',withdrawal)).after.find(row=>row.packageId==='pos-restaurant')!.state).toBe('inactive');const applied=await posAddonReview('pos-restaurant',withdrawal,true);expect(await posAddonReview('pos-restaurant',withdrawal,true)).toEqual(applied);expect(await posAddonHistory()).toEqual(before);
 expect(await command('sale.create',createBody,actor,createKey)).toEqual(created);expect(await command('refund.approve',refundBody,manager,refundKey)).toEqual(refunded);await expect(createSale('restaurant')).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await add(retail);
 expect((await readCompanyPackageGrants(db,actor.companyId)).map(row=>({id:row.packageId,state:row.state}))).toEqual([{id:'pos',state:'active'},{id:'pos-hotel-charge',state:'active'},{id:'pos-restaurant',state:'inactive'}]);expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
});
it('restaurant addon retains actual signed payment callbacks and jobs through draining without requiring the shared provider connection to be withdrawn',async()=>{
 await packageGrant();await packageGrant('pos-restaurant');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
 await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES('pos-company','acct_addon',false,true,true,true,true,'ready','daily',true)");
 await run(env.CONTROL_DB,"INSERT INTO pos_stripe_routes VALUES('acct_addon',false,'h001','pos-company',0) ON CONFLICT DO NOTHING");
 vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_addon_fixture');vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_addon_fixture');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://pos.example.test');
 const sale=await createSale('restaurant');await add(sale);const payment=await change('payment.create',sale,{method:'stripe_card',amountMinor:10700,tenderedMinor:10700});let providerPaid=false;
 vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).includes('payment_intents/')?{id:'pi_addon',status:'succeeded',amount_received:10700,currency:'thb',metadata:{pos_payment_id:payment.id}}:{id:'cs_addon',url:'https://checkout.stripe.com/addon',status:'open',payment_status:providerPaid?'paid':'unpaid',payment_intent:'pi_addon'}))));
 await drainPos(db,'h001',1);expect((await one(db,'SELECT state FROM pos_payments WHERE id=?',payment.id))!.state).toBe('pending');
 await packageGrant('pos-restaurant','draining');const withdrawal=await posAddonWithdrawal('pos-restaurant');
 const raw=JSON.stringify({id:'evt_addon_paid',type:'checkout.session.completed',account:'acct_addon',livemode:false,data:{object:{id:'cs_addon',metadata:{pos_payment_id:payment.id}}}}),time=Math.floor(Date.now()/1000),signature=`t=${time},v1=${createHmac('sha256','whsec_addon_fixture').update(`${time}.${raw}`).digest('hex')}`;
 await posStripeWebhook(raw,signature);await posStripeWebhook(raw,signature);expect(await one(db,"SELECT count(*) n FROM pos_stripe_events WHERE id='evt_addon_paid'")).toEqual({n:1});await expect(posAddonReview('pos-restaurant',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 providerPaid=true;await drainPos(db,'h001',8);expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))!.state).toBe('completed');expect((await one(db,"SELECT applied_at FROM pos_stripe_events WHERE id='evt_addon_paid'"))!.applied_at).not.toBeNull();await closeAddonRestaurantShift();expect((await posAddonObservation('pos-restaurant')).total).toBe(0);
 const applied=await posAddonReview('pos-restaurant',withdrawal,true);expect(await posAddonReview('pos-restaurant',withdrawal,true)).toEqual(applied);expect((await one(db,"SELECT state FROM pos_connections WHERE company_id='pos-company'"))!.state).toBe('ready');expect((await readCompanyPackageGrants(db,actor.companyId)).find(row=>row.packageId==='pos')!.state).toBe('active');expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
});
async function posAddonHotelFixture(sale:string){
 const saleRow=(await one(db,'SELECT * FROM pos_sales WHERE id=?',sale))!,accounts={...defaultConfig.accounts};
 for(const [index,key] of pmsAccountKeys.entries()){
  const id=`addon-pms-${key}`;accounts[key]=id;const category=['revenue','service'].includes(key)?'revenue':['advance','deposit','tax'].includes(key)?'liability':['consumption','commission','cashDifference'].includes(key)?'expense':'asset';
  await run(db,'INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,?,0,0)',id,'chart','pos-company',`ADDON-PMS-${index}`,key,category,'THB');
 }
 // Same declared in-house Hotel fixture as the original atomic room-charge
 // case. All charges, approvals, reversals and grant transitions below use owners.
 await run(db,"INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES('addon-hotel','pos-company','pos-branch','Addon Hotel',?,'open',?,1,'system',0)",saleRow.business_date,JSON.stringify({...defaultConfig,chartId:'chart',accounts,approvedBy:'independent-accountant',businessDate:saleRow.business_date}));
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();for(const [id,kind,data] of [
  ['addon-guest','guest',{name:'Addon Guest'}],['addon-room','room',{number:'A101'}],['addon-account','guest_account',{name:'Addon Account',guestId:'addon-guest',reservationId:'addon-reservation',partyId:'',state:'open',due:0,advance:0,advanceTax:0,deposit:0}],['addon-stay','stay',{guestId:'addon-guest',reservationId:'addon-reservation',roomId:'addon-room',guestAccountId:'addon-account',state:'in_house'}],
 ] as const)await run(tx,'INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,0)',id,'addon-hotel',kind,JSON.stringify(data));});
 await run(db,"UPDATE pos_registers SET property_id='addon-hotel' WHERE id='restaurant'");
}
it('room-charge addon withdrawal requires its real Hotel receivable and independent reversal, leaving POS Restaurant and Hotel base work active',async()=>{
 await packageGrant();await packageGrant('pos-restaurant');await packageGrant('pos-hotel-charge');await packageGrant('pms');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const retail=await createSale();await add(retail);const sale=await createSale('restaurant');await add(sale);await posAddonHotelFixture(sale);
 expect((await posAddonObservation('pos-hotel-charge')).total).toBe(0);
 const current=(await one(db,'SELECT version FROM pos_sales WHERE id=?',sale))!,chargeBody={id:sale,expectedVersion:current.version,propertyId:'addon-hotel',stayId:'addon-stay',expectedPropertyVersion:1},chargeKey=randomUUID(),charged=await command('sale.charge_to_room',chargeBody,actor,chargeKey);
 const pending=await posAddonObservation('pos-hotel-charge');expect(pending.observations.map(row=>row.moduleId)).toEqual(['pos','pms','finance','inventory']);expect(pending.observations.find(row=>row.moduleId==='pms')!.outstanding).toBeGreaterThan(0);
 await packageGrant('pos-hotel-charge','draining');const withdrawal=await posAddonWithdrawal('pos-hotel-charge');await expect(posAddonReview('pos-hotel-charge',withdrawal)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const other=await createSale('restaurant');await add(other);const {posWorkspace}=await import('@erpx/server-pos/public/pos'),drainingView=await posWorkspace(actor,other,'','sale');expect(drainingView.roomChargeEnabled).toBe(false);expect(drainingView.roomChargeStays).toEqual([]);const deniedHistory=await posAddonHistory();await expect(change('sale.charge_to_room',other,{propertyId:'addon-hotel',stayId:'addon-stay',expectedPropertyVersion:2})).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect(await posAddonHistory()).toEqual(deniedHistory);
 const line=(await one(db,'SELECT id FROM pos_lines WHERE sale_id=?',sale))!,requested=await change('room_charge_reversal.request',sale,{propertyId:'addon-hotel',expectedPropertyVersion:2,reason:'Wrong guest account independently corrected',restockLineIds:[line.id]},manager);
 await run(db,"INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES('addon-restaurant-reviewer','addon-restaurant-reviewer','holding_owner','[\"pos\"]','pos-company','holding',0,9999999999999)");
 const restaurantReviewer={...manager,userId:'addon-restaurant-reviewer',assignmentId:'addon-restaurant-reviewer'};await command('room_charge_reversal.approve_pos',{id:requested.id,expectedVersion:1,reason:'Independent restaurant source review'},restaurantReviewer);await expect(posAddonReview('pos-hotel-charge',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const decision={id:requested.id,expectedVersion:2,reason:'Independent Hotel correction review'},decisionKey=randomUUID(),reversed=await command('room_charge_reversal.approve_pms',decision,hotelManager,decisionKey);
 expect(reversed).toMatchObject({id:requested.id,saleId:sale,state:'room_charge_reversed',restoredCostMinor:3000});expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tea'")).toEqual({quantity_milli:10000,value_minor:30000});expect((await posAddonObservation('pos-hotel-charge')).total).toBe(0);
 const before=await posAddonHistory(),applied=await posAddonReview('pos-hotel-charge',withdrawal,true);expect(await posAddonReview('pos-hotel-charge',withdrawal,true)).toEqual(applied);expect(await posAddonHistory()).toEqual(before);expect(await command('sale.charge_to_room',chargeBody,actor,chargeKey)).toEqual(charged);expect(await command('room_charge_reversal.approve_pms',decision,hotelManager,decisionKey)).toEqual(reversed);
 expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',retail))!.state).toBe('open');expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',other))!.state).toBe('open');expect((await readCompanyPackageGrants(db,actor.companyId)).filter(row=>row.packageId!=='pos-hotel-charge').every(row=>row.state==='active')).toBe(true);expect(await one(db,'SELECT count(*) n FROM command_assertions')).toEqual({n:0});
});
it('restaurant addon final review rolls back a deferred actual-sale job and the grant receipt together',async()=>{
 await packageGrant();await packageGrant('pos-restaurant');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');const sale=await createSale('restaurant');await add(sale);await pay(sale);await change('sale.complete',sale);await closeAddonRestaurantShift();await packageGrant('pos-restaurant','draining');expect((await posAddonObservation('pos-restaurant')).total).toBe(0);
 const withdrawal=await posAddonWithdrawal('pos-restaurant'),job=randomUUID(),trigger=`restaurant_addon_deferred_${randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO pos_jobs(id,company_id,kind,record_id,state,created_at) VALUES('${job}','pos-company','complete_sale','${sale}','pending',0);RETURN NEW;END $$;
 CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.company_id='pos-company') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(posAddonReview('pos-restaurant',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await one(db,'SELECT id FROM pos_jobs WHERE id=?',job)).toBeNull();expect(await one(db,'SELECT id FROM company_package_change_sets WHERE id=?',withdrawal.idempotencyKey)).toBeNull();expect((await readCompanyPackageGrants(db,actor.companyId)).find(row=>row.packageId==='pos-restaurant')).toMatchObject({state:'draining',version:2});}
 finally{await db.exec(`DROP TRIGGER ${trigger} ON company_package_change_sets;DROP FUNCTION ${trigger}()`);}
});
it('addon owner readers refuse forged or wrong-Company leases and fence absent POS Hotel Finance and Stock writes',async()=>{
 const {posAddonPackageObligations}=await import('@erpx/server-pos/public/addon-package-obligations'),{hospitalityAddonFinancePackageObligations}=await import('@erpx/server-finance/public/hospitality-addon-package-obligations');
 const input={...posAddonInput('pos-hotel-charge'),moduleId:'pos',now:Date.now()};await expect(posAddonPackageObligations(db,db,input)).rejects.toThrow('TRANSACTION_NOT_ACTIVE_OR_OWNED');await db.transaction(async tx=>{await expect(posAddonPackageObligations(db,tx,input)).rejects.toThrow('POS_PACKAGE_OBLIGATION_SOURCE_REQUIRED');});
 await withOwnerPackageObligations(db,posAddonInput('pos-hotel-charge'),async tx=>{await expect(posAddonPackageObligations(db,tx,{...input,companyId:'foreign-company'})).rejects.toThrow('POS_PACKAGE_OBLIGATION_SOURCE_REQUIRED');await expect(hospitalityAddonFinancePackageObligations(db,tx,{...input,moduleId:'finance',companyId:'foreign-company'})).rejects.toThrow('HOSPITALITY_ADDON_FINANCE_SOURCE_REQUIRED');
  for(const [table,column] of [['company_package_grants','version'],['pos_sales','version'],['pos_registers','version'],['pos_jobs','attempts'],['pos_stripe_events','received_at'],['pos_room_charge_reversal_requests','version'],['pms_properties','version'],['pms_records','version'],['pms_postings','business_date'],['pms_charge_corrections','version'],['finance_invoices','version'],['finance_payments','version'],['finance_journals','version'],['finance_journal_lines','ordinal'],['commerce_stock','version'],['commerce_warehouses','version'],['commerce_products','version']])await expect(db.transaction(async competing=>{await competing.prepare("SET LOCAL lock_timeout='100ms'").run();await competing.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();})).rejects.toMatchObject({code:'55P03'});
 });
});
it('restaurant addon refuses an unrelated same-Company native Hotel journal or retail Finance receipt behind its owner pointers',async()=>{
 await packageGrant();await packageGrant('pos-restaurant');await packageGrant('pos-hotel-charge');await packageGrant('pms');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const retail=await createSale();await add(retail);await pay(retail);await change('sale.complete',retail);
 const restaurant=await createSale('restaurant');await add(restaurant);await pay(restaurant);await change('sale.complete',restaurant);
 const hotelSale=await createSale('restaurant');await add(hotelSale);await posAddonHotelFixture(hotelSale);const hotelSaleRow=(await one(db,'SELECT version FROM pos_sales WHERE id=?',hotelSale))!;
 await command('sale.charge_to_room',{id:hotelSale,expectedVersion:hotelSaleRow.version,propertyId:'addon-hotel',stayId:'addon-stay',expectedPropertyVersion:1});await closeAddonRestaurantShift();await packageGrant('pos-restaurant','draining');expect((await posAddonObservation('pos-restaurant')).total).toBe(0);
 const withdrawal=await posAddonWithdrawal('pos-restaurant'),restaurantPosting=(await one(db,'SELECT journal_id FROM pos_postings WHERE source_key=?',`sale:${restaurant}`))!,otherJournal=(await one(db,"SELECT journal_id FROM pms_postings WHERE property_id='addon-hotel' ORDER BY source_key LIMIT 1"))!,restaurantPayment=(await one(db,'SELECT id,finance_payment_id FROM pos_payments WHERE sale_id=?',restaurant))!,otherPayment=(await one(db,'SELECT finance_payment_id FROM pos_payments WHERE sale_id=?',retail))!;
 expect(await one(db,'SELECT company_id,status FROM finance_journals WHERE id=?',otherJournal.journal_id)).toEqual({company_id:actor.companyId,status:'posted'});expect(await one(db,'SELECT kind,currency,amount_minor,outstanding_minor,status FROM finance_payments WHERE id=?',otherPayment.finance_payment_id)).toEqual({kind:'receipt',currency:'THB',amount_minor:10700,outstanding_minor:0,status:'settled'});
 // These are deliberate owned-source faults, not additional business APIs.
 // FK/uniqueness permits each pointer; posted journal identity stays unchanged.
 for(const fault of ['journal','payment'] as const){
  if(fault==='journal')await run(db,'UPDATE pos_postings SET journal_id=? WHERE source_key=?',otherJournal.journal_id,`sale:${restaurant}`);else await run(db,'UPDATE pos_payments SET finance_payment_id=? WHERE id=?',otherPayment.finance_payment_id,restaurantPayment.id);
  try{const corrupted=await posAddonHistory(),observation=await posAddonObservation('pos-restaurant');expect(observation.observations.find(row=>row.moduleId==='finance')!.outstanding).toBeGreaterThan(0);await expect(posAddonReview('pos-restaurant',withdrawal)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(posAddonReview('pos-restaurant',withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await posAddonHistory()).toEqual(corrupted);expect(await one(db,'SELECT id FROM company_package_change_sets WHERE id=?',withdrawal.idempotencyKey)).toBeNull();expect((await readCompanyPackageGrants(db,actor.companyId)).find(row=>row.packageId==='pos-restaurant')).toMatchObject({state:'draining',version:2});}
  finally{if(fault==='journal')await run(db,'UPDATE pos_postings SET journal_id=? WHERE source_key=?',restaurantPosting.journal_id,`sale:${restaurant}`);else await run(db,'UPDATE pos_payments SET finance_payment_id=? WHERE id=?',restaurantPayment.finance_payment_id,restaurantPayment.id);}
  expect((await posAddonObservation('pos-restaurant')).total).toBe(0);
 }
 const applied=await posAddonReview('pos-restaurant',withdrawal,true);expect(await posAddonReview('pos-restaurant',withdrawal,true)).toEqual(applied);expect((await readCompanyPackageGrants(db,actor.companyId)).filter(row=>row.packageId!=='pos-restaurant').every(row=>row.state==='active')).toBe(true);
});
