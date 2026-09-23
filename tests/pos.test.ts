import { beforeAll,beforeEach,afterEach,describe,it,expect,vi } from 'vitest';
import { createHmac } from 'node:crypto';
import { env,applyMigrations } from './runtime-env';
import { posCommand,one,all,run } from '../src/server/pos-core';
import { accountKeys,saleAmounts,portion,recipeQuantities,formatPosMoney } from '../src/lib/pos';
import { defaultConfig,pmsAccountKeys } from '../src/lib/pms';
import { drainPos,posStripeWebhook,verifyPosStripeSignature } from '../src/server/pos-stripe';
import type { WorkContext } from '../src/server/authorization';
vi.mock('../src/server/runtime',async()=>{const {env}=await import('./runtime-env');return {runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB})};});
const db=env.H001_DB;
afterEach(()=>{vi.unstubAllGlobals();vi.unstubAllEnvs();});
const actor={userId:'cashier',assignmentId:'pos-cashier',role:'holding_owner',companyId:'pos-company',branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'pos-test',name:'Cashier',email:'cashier@example.test',assignmentScope:'holding',scope:{kind:'holding',holdingId:'h001',holdingSlug:'pos-test',holdingName:'POS',companyId:'pos-company'},modules:['pos'],assignments:[],workspaces:[],inherited:false,holdingName:'POS'} as WorkContext;
const manager={...actor,userId:'manager',assignmentId:'pos-manager'};
let serial=0;
const command=(op:string,input:unknown,who=actor,key=`pos-test-command-${++serial}`)=>posCommand(db,who,op,input,key);
async function change(op:string,saleId:string,extra:Record<string,unknown>={},who=actor){const row=await one(db,'SELECT * FROM pos_sales WHERE id=?',saleId);return command(op,{id:saleId,expectedVersion:row!.version,...extra},who);}
async function createSale(mode='retail'){
 const sale=await command('sale.create',{shiftId:mode==='retail'?'shift-retail':'shift-kitchen',serviceMode:mode==='retail'?'retail':'dine_in'});return String(sale.id);
}
async function add(sale:string,product='tea',qty=1000){await change('sale.add',sale,{productId:product,quantityMilli:qty,modifierIds:[],discountMinor:0,note:''});}
async function pay(sale:string,amount=10700){await change('payment.create',sale,{method:'cash',amountMinor:amount,tenderedMinor:amount,reference:''});}
async function kitchen(sale:string,action:string){const f=(await one(db,'SELECT f.* FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=?',sale))!;return command(`kitchen.${action}`,{id:f.id,expectedVersion:f.version,reason:'customer changed order'});}
beforeAll(async()=>{await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);},120000);
beforeEach(async()=>{
 await db.exec(`TRUNCATE pms_sources,pms_postings,pms_records,pms_properties,pos_receipts,pos_audit,pos_jobs,pos_stripe_events,pos_settlements,pos_oauth_states,pos_connections,pos_connection_history,pos_cash_movements,pos_refund_payments,pos_refunds,pos_payments,pos_lines,pos_reservations,pos_stock_moves,pos_fulfillments,pos_sales,pos_tables,pos_shifts,pos_registers,pos_catalog,pos_settings,pos_postings,finance_bank_transactions,finance_bank_statements,finance_allocations,finance_invoices,finance_payments,finance_journal_lines,finance_journals,finance_parties,commerce_stock,commerce_products,commerce_warehouses,commerce_settings,finance_accounts,finance_periods,finance_charts,assignments,command_assertions CASCADE;
 INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('pos-company','company','pos-company','POS test','pos-company') ON CONFLICT DO NOTHING;
 INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,parent_id,branch_id) VALUES('pos-branch','branch','pos-branch','Branch','pos-company','pos-company','pos-branch') ON CONFLICT DO NOTHING;
 INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES('pos-cashier','cashier','holding_owner','["pos"]','pos-company','holding',0,9999999999999),('pos-manager','manager','holding_owner','["pos"]','pos-company','holding',0,9999999999999);
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
 await command('catalog.save',{productId:'latte',barcode:'8850001',category:'Coffee',kind:'recipe',station:'Bar',recipe:[{productId:'milk',quantityMilli:200}],modifiers:[],taxInclusive:false,expectedVersion:0});
});
describe('POS committed business flows',()=>{
 it('limits production POS tenders to cash and verified bank transfer',async()=>{
  const sale=await createSale();await add(sale);
  vi.stubEnv('NODE_ENV','production');
  await expect(change('payment.create',sale,{method:'edc',amountMinor:10700,tenderedMinor:10700,reference:'EDC-001'})).rejects.toMatchObject({code:'POS_TENDER_COMING_SOON'});
  await expect(change('payment.create',sale,{method:'stripe_card',amountMinor:10700,tenderedMinor:10700,reference:''})).rejects.toMatchObject({code:'POS_TENDER_COMING_SOON'});
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
 it('charges one restaurant bill to an in-house folio atomically, without POS revenue or payment',async()=>{
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
    ['folio','folio',{name:'Guest folio',guestId:'guest',reservationId:'reservation',partyId:'',state:'open',due:0,advance:0,advanceTax:0,deposit:0}],
    ['stay','stay',{guestId:'guest',reservationId:'reservation',roomId:'room',folioId:'folio',state:'in_house'}],
   ] as const)await run(tx,'INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,0)',id,'hotel',kind,JSON.stringify(data));
  });
  const input={id:sale,expectedVersion:saleRow!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:1};
  const {posWorkspace}=await import('../src/server/pos');
  expect((await posWorkspace(actor,sale,'','sale')).roomChargeStays).toMatchObject([{stay_id:'stay',room_number:'101',guest_name:'Example Guest'}]);
  const charged=await command('sale.charge_to_room',input,actor,'room-charge-replay');
  expect(charged).toMatchObject({id:sale,state:'completed',tender:'room_charge',propertyId:'hotel'});
  expect(await command('sale.charge_to_room',input,actor,'room-charge-replay')).toEqual(charged);
  expect((await one(db,"SELECT data_json::jsonb->>'due' due FROM pms_records WHERE id='folio'"))?.due).toBe('10700');
  expect(await one(db,'SELECT count(*) n FROM pms_sources')).toEqual({n:1});
  expect((await posWorkspace(actor,sale,'','sale')).roomCharge?.folio_entry_id).toBe(charged.folioEntryId);
  expect(await one(db,'SELECT count(*) n FROM pms_postings')).toEqual({n:1});
  expect(await one(db,'SELECT count(*) n FROM pos_payments')).toEqual({n:0});
  expect(await one(db,"SELECT count(*) n FROM pos_postings WHERE source_key LIKE 'sale:%' OR source_key LIKE 'receipt:%'")).toEqual({n:0});
  expect(await one(db,"SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='tea'")).toEqual({quantity_milli:9000});
  const other=await createSale('dine_in');await add(other);
  const changed=await one(db,'SELECT version FROM pos_sales WHERE id=?',other);
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:1})).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT'});
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',other))?.state).toBe('open');
  await db.transaction(async tx=>{
   await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
   await run(tx,"UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"closed\"')::text WHERE id='folio'");
  });
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:2})).rejects.toMatchObject({code:'PMS_CHARGE_FOLIO'});
  expect(await one(db,'SELECT count(*) n FROM pms_sources')).toEqual({n:1});
  expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',other))?.state).toBe('open');
  await run(db,'UPDATE assignments SET revoked_at=? WHERE id=?',Date.now(),'pos-cashier');
  await expect(command('sale.charge_to_room',{id:other,expectedVersion:changed!.version,propertyId:'hotel',stayId:'stay',expectedPropertyVersion:2})).rejects.toMatchObject({code:'POS_ACCESS_CHANGED'});
 });
 it('exposes company currency without exposing accounting configuration',async()=>{
  const {posWorkspace}=await import('../src/server/pos');
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
  const {posWorkspace}=await import('../src/server/pos');
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
  const {posWorkspace}=await import('../src/server/pos');
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
  const {commercePage}=await import('../src/server/commerce');
  const {invoicesPage,bankWorkspace}=await import('../src/server/finance');
  const {countsPage}=await import('../src/server/inventory-counts');
  const {transfersPage}=await import('../src/server/inventory-transfers');
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
 it('split tender never overpays and closing a shift does not repost sales',async()=>{
  const sale=await createSale();await add(sale);await pay(sale,5000);await change('payment.create',sale,{method:'edc',amountMinor:5700,tenderedMinor:5700,reference:'EDC-123'});await change('sale.complete',sale);
  const before=await one(db,'SELECT COUNT(*) n FROM finance_journals');await command('shift.close',{id:'shift-retail',expectedVersion:1,countedMinor:15000});expect(await one(db,'SELECT COUNT(*) n FROM finance_journals')).toEqual(before);
  await expect(command('shift.review',{id:'shift-retail',expectedVersion:2,reason:'count matches'})).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  await command('shift.review',{id:'shift-retail',expectedVersion:2,reason:'count matches'},manager);
 });
 it('serializes two tills reserving the final unit and guards other inventory writers',async()=>{
  await run(db,"UPDATE commerce_stock SET quantity_milli=1000,value_minor=3000 WHERE product_id='tea'");const a=await createSale(),b=await createSale();await add(a);await add(b);
  const results=await Promise.allSettled([pay(a),pay(b)]);expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
  await expect(run(db,"UPDATE commerce_stock SET quantity_milli=0,value_minor=0 WHERE product_id='tea'")).rejects.toMatchObject({code:'23514'});
  expect(await one(db,"SELECT SUM(quantity_milli) qty FROM pos_reservations WHERE product_id='tea'")).toEqual({qty:1000});
 });
 it('serializes Sales and POS competing for the final unit through the same availability owner',async()=>{
  const {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder,changeOrderReservation}=await import('../src/server/commerce');
  const {assignFlowPrimary,resolveFlow}=await import('../src/server/flow-routing');
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
  const sale=await createSale();await add(sale,'tea',2000);await pay(sale,21400);await change('sale.complete',sale);const line=(await one(db,'SELECT * FROM pos_lines WHERE sale_id=?',sale))!;
  const refund=await change('refund.request',sale,{reason:'one damaged package',payoutShiftId:'shift-retail',lines:[{lineId:line.id,quantityMilli:1000,restock:true}]});
  await expect(command('refund.approve',{id:refund.id,expectedVersion:1,reason:'cash returned'})).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  await command('refund.approve',{id:refund.id,expectedVersion:1,reason:'cash returned'},manager);
  expect(await one(db,"SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id='tea'")).toEqual({quantity_milli:9000,value_minor:27000});
  expect(await one(db,'SELECT state FROM pos_refunds WHERE id=?',refund.id)).toEqual({state:'completed'});
  await expect(change('refund.request',sale,{reason:'too many',lines:[{lineId:line.id,quantityMilli:2000,restock:true}]})).rejects.toMatchObject({code:'POS_REFUND_QUANTITY'});
 });
 it('closed periods and changed scopes leave no payment or journal effects',async()=>{
  const sale=await createSale();await add(sale);await run(db,"UPDATE finance_periods SET state='closed'");await expect(pay(sale)).rejects.toMatchObject({code:'POS_PERIOD_CLOSED'});expect(await one(db,'SELECT COUNT(*) n FROM pos_payments')).toEqual({n:0});
  await expect(change('sale.hold',sale,{}, {...actor,branchId:'other'})).rejects.toMatchObject({status:404});
 });
});
describe('POS provider recovery and reporting',()=>{
 async function stripePending(){
  await run(db,"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state) VALUES('pos-company','acct_original',false,true,true,true,true,'ready')");
  const sale=await createSale();await add(sale);
  const p=await change('payment.create',sale,{method:'stripe_card',amountMinor:10700,tenderedMinor:10700});
  await run(db,"UPDATE pos_jobs SET state='completed' WHERE kind='checkout'");
  await run(db,"UPDATE pos_payments SET provider_id='cs_fixture',state='pending' WHERE id=?",p.id);
  await run(db,"INSERT INTO pos_jobs(id,company_id,kind,record_id,created_at) VALUES('check-fixture','pos-company','check_payment',?,0)",p.id);
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_pos_fixture_not_a_real_key');
  return {sale,payment:String(p.id)};
 }
 function mockPaid(payment:string){return vi.fn(async(url: string|URL|Request,init?:RequestInit)=>{
  expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_original');
  const body=String(url).includes('payment_intents/')?{id:'pi_fixture',status:'succeeded',amount_received:10700,currency:'thb',metadata:{pos_payment_id:payment}}:{id:'cs_fixture',payment_status:'paid',payment_intent:'pi_fixture'};
  return new Response(JSON.stringify(body),{status:200});
 });}
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
 it('webhook mode, duplicate and reverse delivery cannot route across companies',async()=>{
  await run(env.CONTROL_DB,"INSERT INTO pos_stripe_routes VALUES('acct_original',false,'h001','pos-company',0) ON CONFLICT DO NOTHING");
  vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_pos_fixture');vi.stubEnv('POS_STRIPE_LIVE_WEBHOOK_SECRET','whsec_live_fixture');
  const body=JSON.stringify({id:'evt_pos_fixture',type:'unhandled.test',account:'acct_original',livemode:false,data:{object:{id:'obj'}}}),t=Math.floor(Date.now()/1000),sign=(raw:string)=>`t=${t},v1=${createHmac('sha256','whsec_pos_fixture').update(`${t}.${raw}`).digest('hex')}`;
  await posStripeWebhook(body,sign(body));await posStripeWebhook(body,sign(body));expect(await one(db,"SELECT COUNT(*) n FROM pos_stripe_events WHERE id='evt_pos_fixture'")).toEqual({n:1});
  const live=body.replace('"livemode":false','"livemode":true');await expect(posStripeWebhook(live,sign(live))).rejects.toMatchObject({code:'POS_WEBHOOK_MODE'});
 });
 it('hosted onboarding accepts account-link responses without an object id',async()=>{
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_fixture');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://pos.example.test');
  const fetcher=vi.fn(async(url:string|URL|Request)=>new Response(JSON.stringify(String(url).endsWith('/account_links')?{object:'account_link',url:'https://connect.stripe.com/setup/test'}:{id:'acct_onboard_fixture',country:'TH',charges_enabled:false,payouts_enabled:false,capabilities:{},requirements:{currently_due:['business_profile.url']}})));
  vi.stubGlobal('fetch',fetcher);await command('connection.create',{livemode:false});await drainPos(db,'h001');
  expect((await one(db,"SELECT state FROM pos_jobs WHERE kind='connect'"))?.state).toBe('completed');expect(fetcher).toHaveBeenCalledTimes(3);
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
 it('full manual advance refund releases the reservation and lets the original bill be edited',async()=>{
  const sale=await createSale();await add(sale);await pay(sale);
  const r=await change('refund.request',sale,{amountMinor:10700,reason:'customer changed mind',payoutShiftId:'shift-retail'});
  await command('refund.approve',{id:r.id,expectedVersion:1,reason:'cash returned in full'},manager);
  expect(await one(db,'SELECT COUNT(*) n FROM pos_reservations')).toEqual({n:0});expect((await one(db,'SELECT state FROM pos_sales WHERE id=?',sale))?.state).toBe('open');await add(sale);
 });
 it('reports actual return cost and disallows discount preparer self approval',async()=>{
  const sale=await createSale();await change('sale.add',sale,{productId:'tea',quantityMilli:1000,discountMinor:1000},manager);
  await expect(change('sale.approve_discount',sale,{reason:'approve my discount'},manager)).rejects.toMatchObject({code:'POS_INDEPENDENT_REVIEW'});
  await change('sale.approve_discount',sale,{reason:'independent review'});await pay(sale,9630);await change('sale.complete',sale);
  const {posWorkspace}=await import('../src/server/pos');const data=await posWorkspace(actor,sale);
  expect(data.reports[0].cost_minor).toBe(3000);expect(data.reports[0].gross_profit_minor).toBe(6000);expect(data.paymentMix[0].received_minor).toBe(9630);
 });
});
describe('POS monetary and webhook boundaries',()=>{
 it('allocates every cent across repeated partial refunds',()=>{expect(recipeQuantities([{productId:'milk',quantityMilli:200000,conversionMicros:1000}],1000)).toEqual([{productId:'milk',quantityMilli:200}]);expect(portion(100,0,1,3)+portion(100,1,1,3)+portion(100,2,1,3)).toBe(100);expect(saleAmounts(1000,10700,700,true,0)).toEqual({subtotal:10000,tax:700,total:10700,discount:0});});
 it('requires a fresh, exact signed webhook body',()=>{const now=Date.now(),t=Math.floor(now/1000),body='{"id":"evt_one"}',secret='whsec_test',signature=`t=${t},v1=${createHmac('sha256',secret).update(`${t}.${body}`).digest('hex')}`;expect(verifyPosStripeSignature(body,signature,[secret],now)).toBe(true);expect(verifyPosStripeSignature(body+' ',signature,[secret],now)).toBe(false);expect(verifyPosStripeSignature(body,signature,[secret],now+301000)).toBe(false);});
});

it('quality quarantine blocks a reserved retail payment without consuming stock or cash',async()=>{
 const {createQualityHold}=await import('../src/server/quality');
 const sale=await createSale();await add(sale);
 const stock=await one(db,'SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?','warehouse','tea');
 await createQualityHold(manager,{warehouseId:'warehouse',productId:'tea',expectedStockVersion:Number(stock!.version),dueDate:'2026-09-22',reason:'Packaging damage requires inspection'},'quality-pos-hold-command');
 const receiptsBefore=await one(db,'SELECT count(*) n FROM pos_receipts');
 await expect(pay(sale)).rejects.toMatchObject({code:'QUALITY_HOLD'});
 expect((await one(db,'SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?','warehouse','tea'))).toMatchObject({quantity_milli:10000,value_minor:30000});
 expect(await one(db,'SELECT count(*) n FROM pos_payments WHERE sale_id=?',sale)).toEqual({n:0});
 expect(await one(db,'SELECT count(*) n FROM pos_receipts')).toEqual(receiptsBefore);
 const {posWorkspace}=await import('../src/server/pos');const workspace=await posWorkspace(actor,undefined,'','sale');
 expect(workspace.stock.find(s=>s.product_id==='tea')?.quality_held).toBe(1);
});
