import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema,transitionOrder} from '@erpx/server-commerce/public/commerce';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {pricingWorkspace} from "@erpx/server-sales/public/sales-pricing";
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {recordSupplementData} from '@erpx/server-records/public/record-details';
import {ledgerWorkspace} from '@erpx/server-finance/public/ledger';

let salesSettlementDatabase:import("@erpx/server-runtime/public/runtime/types").SqlDatabase|undefined;
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:salesSettlementDatabase??env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>{salesSettlementDatabase=undefined;vi.unstubAllEnvs();});
let f:Awaited<ReturnType<typeof expenseFixture>>,productId:string,customerId:string,warehouseId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.manager.role='sales_manager';f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,'sales_manager');warehouseId=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Pricing warehouse'},key())).id);productId=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Controlled service kit',price:'120',taxBps:700},key())).id);customerId=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Pricing customer'},key())).id);});
type PriceSeed={name:string;currency:string;customerId:string|null;validFrom:string;validUntil:string;maxAutoDiscountBps:number;items:{productId:string;price:string;taxBps:number}[]};
async function seedPriceList(input:PriceSeed,options:{active?:boolean;id?:string}={},database:import("@erpx/server-runtime/public/runtime/types").SqlDatabase=db){
 const id=options.id??`price-list-${key()}`,now=Date.now(),minor=(value:string)=>Math.round(Number(value)*100);
 const statements=[
  database.prepare("INSERT INTO sales_price_lists(id,company_id,name,currency,customer_id,valid_from,valid_until,max_auto_discount_bps,status,created_by,prepared_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'draft',?,?,1,?,?)").bind(id,f.company,input.name,input.currency,input.customerId,input.validFrom,input.validUntil,input.maxAutoDiscountBps,f.owner.userId,f.owner.userId,now,now),
  ...input.items.map((item,index)=>database.prepare("INSERT INTO sales_price_list_items(id,price_list_id,product_id,price_minor,tax_bps,ordinal) VALUES(?,?,?,?,?,?)").bind(`${id}-item-${index}`,id,item.productId,minor(item.price),item.taxBps,index)),
  ...(options.active?[database.prepare("UPDATE sales_price_lists SET status='active',approved_by=?,approval_reason=?,activated_at=?,version=2,updated_at=? WHERE id=?").bind(f.manager.userId,'Independent commercial review',now,now,id)]:[]),
 ];
 await database.batch(statements);
 return {id,status:options.active?'active' as const:'draft' as const,version:options.active?2:1};
}
async function cancelPriceList(id:string,database:import("@erpx/server-runtime/public/runtime/types").SqlDatabase=db){const now=Date.now(),row=await database.prepare("SELECT status,version FROM sales_price_lists WHERE id=? AND company_id=?").bind(id,f.company).first<{status:string;version:number}>();if(!row)throw Error('PRICE_LIST_NOT_FOUND');if(row.status==='cancelled')return {id,status:'cancelled' as const,version:row.version};await database.prepare("UPDATE sales_price_lists SET status='cancelled',approval_reason=?,version=version+1,updated_at=? WHERE id=?").bind('Retain the abandoned original unapproved customer price book',now,id).run();return {id,status:'cancelled' as const,version:row.version+1};}

async function pricingSales(state:'active'|'draining'='active',endsAt:number|null=null){const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId==='sales');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-sales-pricing-provider'}),obligations:async()=>{throw Error('PRICING_FIXTURE_CANNOT_CERTIFY_BASE_WITHDRAWAL');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'sales',packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Original isolated Sales pricing owner authority',evidence:'test:sales-pricing-boundary'}});}

it('reads exact price-list detail through the Sales owner without a full Finance purchase and retains source history while draining',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await pricingSales();const created=await seedPriceList({name:'Sales-only retained price-list source',currency:'THB',customerId,validFrom:'2026-10-01',validUntil:'2026-12-31',maxAutoDiscountBps:500,items:[{productId,price:'100',taxBps:700}]}),target={kind:'price_list' as const,id:String(created.id)};
 const detail=await recordSupplementData(f.owner,target,{});expect(detail).toMatchObject({kind:'price_list',row:{id:created.id,name:'Sales-only retained price-list source'}});expect(detail).toMatchObject({data:{purchase:{newWork:true,settleExisting:true,history:true},lists:[{id:created.id}],items:[{price_list_id:created.id,product_id:productId,price_minor:10000}]}});await expect(ledgerWorkspace(f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await pricingSales('draining');expect(await recordSupplementData(f.owner,target,{})).toMatchObject({kind:'price_list',data:{purchase:{newWork:false,settleExisting:true,history:true}}});await expect(recordSupplementData({...f.owner,branchId:f.branch},target,{})).rejects.toMatchObject({code:'COMPANY_PRICING_SCOPE_REQUIRED'});await expect(recordSupplementData(f.owner,{...target,id:key()},{})).rejects.toMatchObject({code:'PRICE_LIST_NOT_FOUND'});await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();await expect(recordSupplementData(f.owner,target,{})).rejects.toMatchObject({code:'SALES_PRICING_ACCESS_CHANGED'});
});

it('does not let a guessed generic price-list kind bypass the actual Company purchase or original current Sales permissions',async()=>{
 const created=await seedPriceList({name:'Retained exact original generic detail',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-12-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700}]}),target={kind:'price_list' as const,id:String(created.id)};vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(recordSupplementData(f.owner,target,{})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await pricingSales();const narrowed={...f.owner,iam:{permissions:['commerce.read'],grantablePermissions:[],principalId:'declared-source-only-pricing-fixture',personId:null,roleRevision:1,identityVersion:0,freshUntil:null}};await expect(recordSupplementData(narrowed,target,{})).rejects.toMatchObject({code:'SALES_PRICING_ACCESS_CHANGED'});
});
const controlledPrices=()=>({name:'Original independently reviewed purchased Sales price book',currency:'THB',customerId,validFrom:'2026-09-01',validUntil:'2026-12-31',maxAutoDiscountBps:500,items:[{productId,price:'100',taxBps:700}]});
const salesWithdrawalScope=()=>({companyId:f.company,packageId:'sales',packageVersion:1});
async function salesWithdrawalObservation(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,salesWithdrawalScope(),tx=>readOwnerPackageObligations(db,tx,{...salesWithdrawalScope(),now:Date.now()}));}
async function salesWithdrawalRequest(){const {readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId==='sales');if(!current)throw Error('ACTUAL_SALES_GRANT_REQUIRED');return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Review all actual Sales customer and owner settlement sources',evidence:'test:sales-four-owner-withdrawal'}}]};}
async function salesWithdrawalReview(request:Awaited<ReturnType<typeof salesWithdrawalRequest>>,apply=false,after?:(tx:import("@erpx/server-runtime/public/runtime/types").SqlDatabase)=>Promise<void>){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-sales-withdrawal-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,salesWithdrawalScope(),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after)await after(tx);return result;});}
const salesInquiry=()=>({branchId:f.branch,title:'Original Sales withdrawal inquiry',contactName:'Declared synthetic customer contact',contactDetail:'Declared fictional internal contact channel',source:'Original staff direct inquiry',brief:'Close original customer inquiry through its normal owner decision.',dueDate:'2026-10-20'});

it('reads actual four-owner Sales settlement sources without inventing duties for retained customer and product identities',async()=>{
 await pricingSales();const observed=await salesWithdrawalObservation();expect(observed.observations.map(row=>row.moduleId)).toEqual(['sales','crm','inventory','finance']);expect(observed.total).toBe(0);expect(observed.observations.every(row=>/^[a-f0-9]{64}$/.test(row.revision))).toBe(true);expect(JSON.stringify(observed)).not.toMatch(/contact_detail|authority_json|notification_email/);
});

it('withdraws Sales only after normal retained quotation pricing and CRM settlement and preserves exact original history retries',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await pricingSales();const {createCRMLead,changeCRMLead,getCRMLead}=await import('@erpx/server-crm/public/crm'),lead=await createCRMLead(f.owner,salesInquiry(),key()),quote=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-10-16',dueDate:'2026-10-20',lines:[{productId,quantity:'1',price:'100',taxBps:0}]}),key());
 const opened=await salesWithdrawalObservation();expect(opened.observations.find(row=>row.moduleId==='sales')?.outstanding).toBeGreaterThan(0);expect(opened.observations.find(row=>row.moduleId==='crm')?.outstanding).toBeGreaterThan(0);await pricingSales('draining');const request=await salesWithdrawalRequest();await expect(salesWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(salesWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const close={action:'disqualify' as const,expectedVersion:1,reason:'Original customer withdrew the retained inquiry'},closeKey=key(),closed=await changeCRMLead(f.owner,String(lead.id),close,closeKey),cancelQuote={action:'cancel' as const,expectedVersion:1},quoteKey=key(),cancelledQuote=await transitionOrder(f.owner,String(quote.id),cancelQuote,quoteKey);expect((await salesWithdrawalObservation()).total).toBe(0);
 const tables=['commerce_orders','commerce_order_lines','sales_price_lists','sales_price_list_items','crm_leads','crm_accounts','crm_contacts','crm_case_events'],history=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(row)::text value FROM ${table} row ORDER BY to_jsonb(row)::text`).all()).results)),before=await history();expect((await salesWithdrawalReview(request)).after.find(grant=>grant.packageId==='sales')?.state).toBe('inactive');const applied=await salesWithdrawalReview(request,true);expect(await salesWithdrawalReview(request,true)).toEqual(applied);expect(await history()).toEqual(before);expect(await changeCRMLead(f.owner,String(lead.id),close,closeKey)).toEqual(closed);expect(await transitionOrder(f.owner,String(quote.id),cancelQuote,quoteKey)).toEqual(cancelledQuote);expect(await getCRMLead(f.owner,String(lead.id))).toMatchObject({status:'disqualified'});await expect(createCRMLead(f.owner,salesInquiry(),key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});expect((await pricingWorkspace(f.owner)).purchase).toMatchObject({newWork:false,settleExisting:false,history:true});await expect(ledgerWorkspace(f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
});

it('rolls back a late normal Sales price obligation and its normal Commercial inactive receipt after deferred owner effects',async()=>{
 await pricingSales();await pricingSales('draining');const request=await salesWithdrawalRequest(),command=key();await expect(salesWithdrawalReview(request,true,async tx=>{salesSettlementDatabase=tx;try{await seedPriceList(controlledPrices(),{id:`price-list-${command}`},tx);}finally{salesSettlementDatabase=undefined;}})).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='sales'").bind(f.company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT 1 FROM sales_price_lists WHERE id=?').bind(`price-list-${command}`).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect((await salesWithdrawalObservation()).total).toBe(0);
});

it('observes an actual committed Sales source obligation after owner leases before the first snapshot without retrying',async()=>{
 await pricingSales();await pricingSales('draining');const request=await salesWithdrawalRequest(),command=key(),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),observed=await committedSourceVisibility(db,async tx=>{salesSettlementDatabase=tx;try{await seedPriceList(controlledPrices(),{id:`price-list-${command}`},tx);}finally{salesSettlementDatabase=undefined;}},()=>measureReads(async()=>{try{await salesWithdrawalReview(request);return null;}catch(error){return error;}}));expect(observed.result).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await db.prepare('SELECT status FROM sales_price_lists WHERE id=?').bind(`price-list-${command}`).first('status')).toBe('draft');
});

it('requires exact minted Sales sources and rejects unsupported versions and cross-Company owner tokens',async()=>{
 const {salesPackageObligations}=await import('@erpx/server-sales/public/package-obligations'),{withOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),input={...salesWithdrawalScope(),moduleId:'sales',now:Date.now()};await expect(db.transaction(tx=>salesPackageObligations(db,tx,input))).rejects.toThrow('SALES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...salesWithdrawalScope(),packageVersion:2},async()=>{})).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,salesWithdrawalScope(),tx=>salesPackageObligations(db,tx,{...input,moduleId:'crm'}))).rejects.toThrow('SALES_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,salesWithdrawalScope(),tx=>salesPackageObligations(db,tx,{...input,companyId:'other-company'}))).rejects.toThrow('SALES_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
});

it('keeps forged terminal Sales and CRM labels unresolved when their actual immutable owner decisions are missing',async()=>{
 const quote=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-10-16',dueDate:'2026-10-20',lines:[{productId,quantity:'1',price:'100',taxBps:0}]}),key()),{createCRMLead}=await import('@erpx/server-crm/public/crm'),lead=await createCRMLead(f.owner,salesInquiry(),key());await db.prepare("UPDATE commerce_orders SET state='cancelled',version=version+1 WHERE id=?").bind(quote.id).run();await db.prepare("UPDATE crm_leads SET status='disqualified',version=version+1 WHERE id=?").bind(lead.id).run();await pricingSales();await pricingSales('draining');const observation=await salesWithdrawalObservation();expect(observation.observations.find(row=>row.moduleId==='sales')?.outstanding).toBeGreaterThan(0);expect(observation.observations.find(row=>row.moduleId==='crm')?.outstanding).toBeGreaterThan(0);await expect(salesWithdrawalReview(await salesWithdrawalRequest())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
});

it('reports the original sales approval credit assertion before a simultaneous stale version',async()=>{
 const attempt=async(creditLimit:string)=>{
  const party=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Credit ordering customer',paymentTermsDays:30,creditLimit},key())).id);
  const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]}),key()),id=String(created.id);
  await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());
  const before=await getOrder(f.owner,id),command=key();let failure:unknown;
  try{await transitionOrder(f.manager,id,{action:'approve',expectedVersion:1},command);}catch(error){failure=error;}
  expect(failure).toBeDefined();expect(failure).toMatchObject({status:409,code:'CONFLICT'});
  // Workflow retains the underlying PostgreSQL check violation in `cause`;
  // its failing-row detail identifies which command_assertions step fired.
  const cause=(failure as {cause?:{code?:string;detail?:string}}).cause;
  expect(cause?.code).toBe('23514');expect(cause?.detail).toContain(creditLimit==='0'?'version-commerce_orders':'customer-credit-limit');
  expect(await getOrder(f.owner,id)).toEqual(before);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
 };
 await attempt('1');
 await attempt('0');
});

it('reports original save errors when stale version collides with product party and Money guards',async()=>{
 const body=orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]}),created=await saveOrder(f.owner,null,body,key()),id=String(created.id);
 await saveOrder(f.owner,id,orderSchema.parse({...body,expectedVersion:1,note:'Actual second version'}),key());
 const before=await getOrder(f.owner,id);
 const money=async()=> (await db.prepare("SELECT to_jsonb(m)::text value FROM money_transaction_snapshots m WHERE company_id=? AND source_type IN ('commerce_order_revision','commerce_order_line_revision') AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${id}:%`,`${id}-line-%`).all<{value:string}>()).results;
 const moneyBefore=await money();expect(moneyBefore).toHaveLength(8);
 const cases=[
  {name:'stale version',input:{...body,expectedVersion:99},code:'CONFLICT',sql:'23514',detail:'version-commerce_orders'},
  {name:'party before stale',input:{...body,expectedVersion:99,partyId:`missing-${key()}`},code:'CONFLICT',sql:'23514',detail:'party'},
  {name:'product before stale',input:{...body,expectedVersion:99,lines:[{...body.lines[0],productId:`missing-${key()}`}]},code:'PRODUCT_INVALID'},
  {name:'duplicate Money before stale',input:{...body,expectedVersion:1},code:'CONFLICT',sql:'23505',detail:'money_transaction_snapshots'},
 ] as const;
 for(const item of cases){
  const command=key();let failure:unknown;try{await saveOrder(f.owner,id,orderSchema.parse(item.input),command);}catch(error){failure=error;}
  expect(failure,item.name).toBeDefined();expect(failure,item.name).toMatchObject({code:item.code});
  if('sql' in item){const cause=(failure as {cause?:{code?:string;detail?:string;constraint?:string}}).cause;expect(cause?.code,item.name).toBe(item.sql);expect(`${cause?.detail??''} ${cause?.constraint??''}`,item.name).toContain(item.detail);}
  expect(await getOrder(f.owner,id),item.name).toEqual(before);expect(await money(),item.name).toEqual(moneyBefore);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),`${item.name} ${table}`).toBeNull();
 }
});

it('checks sales-versus-quote conversion before a stale transition version',async()=>{
 const create=async(kind:'quote'|'sales')=>String((await saveOrder(f.owner,null,orderSchema.parse({kind,partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]}),key())).id);
 const sales=await create('sales'),quote=await create('quote');
 for(const [id,expected] of [[sales,'QUOTE_REQUIRED'],[quote,'CONFLICT']] as const){
  const before=await getOrder(f.owner,id),command=key();let failure:unknown;try{await transitionOrder(f.owner,id,{action:'convert',expectedVersion:99},command);}catch(error){failure=error;}
  expect(failure).toMatchObject({status:expected==='QUOTE_REQUIRED'?422:409,code:expected});
  if(expected==='CONFLICT')expect((failure as {cause?:{detail?:string}}).cause?.detail).toContain('version-commerce_orders');
  expect(await getOrder(f.owner,id)).toEqual(before);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
  expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE source_id=?').bind(id).first()).toBeNull();
 }
});

it('retains original Money error precedence for a stale quotation conversion',async()=>{
 const body=orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]});
 const original=String((await saveOrder(f.owner,null,body,key())).id);
 const missingRate=`missing-fx-${key()}`,wrongScale=`wrong-scale-${key()}`;
 // Historical quoted rows are separate fixtures; no Finance rate or currency
 // guard is disabled, and the production TS conversion itself is the oracle.
 for(const [id,currency] of [[missingRate,'NZD'],[wrongScale,'JPY']] as const){
  await db.prepare("INSERT INTO commerce_orders SELECT (jsonb_populate_record(NULL::commerce_orders,to_jsonb(o)||jsonb_build_object('id',?::text,'number',?::text,'currency',?::text))).* FROM commerce_orders o WHERE o.id=? AND o.company_id=?").bind(id,id,currency,original,f.company).run();
 }
 expect(Number((await db.prepare("SELECT count(*) AS n FROM company_fx_rates WHERE company_id=? AND currency='NZD'").bind(f.company).first<{n:number}>())?.n)).toBe(0);
 const money=async()=> (await db.prepare("SELECT to_jsonb(m)::text AS value FROM money_transaction_snapshots m WHERE company_id=? ORDER BY source_type,source_id,amount_kind").bind(f.company).all<{value:string}>()).results;
 const beforeMoney=await money();
 for(const [id,code,status] of [[original,'CONFLICT',409],[missingRate,'MANUAL_RATE_REQUIRED',422],[wrongScale,'CURRENCY_SCALE_MISMATCH',422]] as const){
  const before=await getOrder(f.owner,id),command=key();let failure:unknown;
  try{await transitionOrder(f.owner,id,{action:'convert',expectedVersion:99},command);}catch(error){failure=error;}
  expect(failure,id).toMatchObject({status,code});
  if(code==='CONFLICT')expect((failure as {cause?:{detail?:string}}).cause?.detail).toContain('version-commerce_orders');
  expect(await getOrder(f.owner,id)).toEqual(before);expect(await money()).toEqual(beforeMoney);
  expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE source_id=?').bind(id).first()).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),`${id} ${table}`).toBeNull();
 }
});

it('enforces selected price list item and tax before Commerce writes',async()=>{
 const otherProduct=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Unlisted service',price:'100',taxBps:700},key())).id);
 const {id:listId}=await seedPriceList({name:'Price oracle',currency:'THB',customerId,validFrom:'2026-09-01',validUntil:'2026-12-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700}]},{active:true});
 const beforeList=await db.prepare('SELECT to_jsonb(l)::text image FROM sales_price_lists l WHERE id=?').bind(listId).first<{image:string}>();
 const beforeItems=(await db.prepare('SELECT to_jsonb(i)::text image FROM sales_price_list_items i WHERE price_list_id=? ORDER BY id').bind(listId).all<{image:string}>()).results;
 const base={kind:'quote' as const,partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',priceListId:listId};
 for(const example of [{product:otherProduct,tax:700,code:'PRICE_LIST_ITEM_REQUIRED'},{product:productId,tax:0,code:'PRICE_LIST_TAX_REQUIRED'}]){
  const command=key(),orderId=`order-${command}`;
  await expect(saveOrder(f.owner,null,orderSchema.parse({...base,lines:[{productId:example.product,quantity:'1',price:'100',taxBps:example.tax}]}),command)).rejects.toMatchObject({status:422,code:example.code});
  expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(orderId).first(),example.code).toBeNull();
  expect(await db.prepare('SELECT 1 FROM commerce_order_lines WHERE order_id=?').bind(orderId).first(),example.code).toBeNull();
  expect(await db.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?)").bind(f.company,`${orderId}:%`,`${orderId}-line-%`).first(),example.code).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),`${example.code} ${table}`).toBeNull();
  expect(await db.prepare('SELECT to_jsonb(l)::text image FROM sales_price_lists l WHERE id=?').bind(listId).first()).toEqual(beforeList);
  expect((await db.prepare('SELECT to_jsonb(i)::text image FROM sales_price_list_items i WHERE price_list_id=? ORDER BY id').bind(listId).all()).results).toEqual(beforeItems);
 }
 const command=key(),created=await saveOrder(f.owner,null,orderSchema.parse({...base,lines:[{productId,quantity:'1',price:'100',taxBps:700}]}),command),id=String(created.id);
 expect(created).toMatchObject({id:`order-${command}`,version:1});
 const detail=await getOrder(f.owner,id);expect(detail.order).toMatchObject({price_list_id:listId,price_list_version:2,subtotal_minor:10000,tax_minor:700});expect(detail.lines).toHaveLength(1);expect(detail.lines[0]).toMatchObject({product_id:productId,price_minor:10000,list_price_minor:10000,tax_bps:700,subtotal_minor:10000,tax_minor:700,discount_bps:0});
 expect((await db.prepare("SELECT amount_kind FROM money_transaction_snapshots WHERE company_id=? AND (source_id=? OR source_id=?) ORDER BY source_id,amount_kind").bind(f.company,`${id}:1`,`${id}-line-0:1`).all()).results).toHaveLength(4);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),table).not.toBeNull();
});

it('orders warehouse and whole-list validation before product availability',async()=>{
 const listedSecond=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Second listed service',price:'100',taxBps:700},key())).id);
 const unlisted=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Unlisted service',price:'100',taxBps:700},key())).id);
 const {id:listId}=await seedPriceList({name:'Ordering oracle',currency:'THB',customerId,validFrom:'2026-09-01',validUntil:'2026-12-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700},{productId:listedSecond,price:'100',taxBps:700}]},{active:true});
 const version=Number(await db.prepare('SELECT version FROM commerce_products WHERE id=?').bind(productId).first('version'));
 await saveMaster(f.owner,'products',productId,{expectedVersion:version,sku:key(),name:'Archived first listed product',price:'100',taxBps:700,itemType:'service',status:'archived'},key());
 const beforeList=await db.prepare('SELECT to_jsonb(l)::text image FROM sales_price_lists l WHERE id=?').bind(listId).first();
 const beforeItems=(await db.prepare('SELECT to_jsonb(i)::text image FROM sales_price_list_items i WHERE price_list_id=? ORDER BY id').bind(listId).all()).results;
 const beforeProduct=await db.prepare('SELECT to_jsonb(p)::text image FROM commerce_products p WHERE id=?').bind(productId).first();
 const base={kind:'quote' as const,partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10'};
 const line=(id:string,taxBps:number)=>({productId:id,quantity:'1',price:'100',taxBps});
 const cases=[
  {label:'warehouse before nonexistent list',input:{...base,warehouseId:`missing-${key()}`,priceListId:`missing-${key()}`,lines:[line(productId,700)]},code:'WAREHOUSE_INVALID'},
  {label:'later item tax before first archived product',input:{...base,priceListId:listId,lines:[line(productId,700),line(listedSecond,0)]},code:'PRICE_LIST_TAX_REQUIRED'},
  {label:'later missing list item before first archived product',input:{...base,priceListId:listId,lines:[line(productId,700),line(unlisted,700)]},code:'PRICE_LIST_ITEM_REQUIRED'},
  {label:'first product unavailable when list is valid',input:{...base,priceListId:listId,lines:[line(productId,700),line(listedSecond,700)]},code:'PRODUCT_INVALID'},
 ] as const;
 for(const example of cases){
  const command=key(),id=`order-${command}`;
  await expect(saveOrder(f.owner,null,orderSchema.parse(example.input),command),example.label).rejects.toMatchObject({status:422,code:example.code});
  expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(id).first(),example.label).toBeNull();
  expect(await db.prepare('SELECT 1 FROM commerce_order_lines WHERE order_id=?').bind(id).first(),example.label).toBeNull();
  expect(await db.prepare('SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?)').bind(f.company,`${id}:%`,`${id}-line-%`).first(),example.label).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),`${example.label}: ${table}`).toBeNull();
  expect(await db.prepare('SELECT to_jsonb(l)::text image FROM sales_price_lists l WHERE id=?').bind(listId).first(),example.label).toEqual(beforeList);
  expect((await db.prepare('SELECT to_jsonb(i)::text image FROM sales_price_list_items i WHERE price_list_id=? ORDER BY id').bind(listId).all()).results,example.label).toEqual(beforeItems);
  expect(await db.prepare('SELECT to_jsonb(p)::text image FROM commerce_products p WHERE id=?').bind(productId).first(),example.label).toEqual(beforeProduct);
 }
});

it('checks saved order kind before warehouse and pricing validation on update',async()=>{
 const original=orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]});
 const created=await saveOrder(f.owner,null,original,key()),id=String(created.id),before=await getOrder(f.owner,id);
 const beforeMoney=(await db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${id}:%`,`${id}-line-%`).all()).results;
 const command=key(),input=orderSchema.parse({...original,kind:'sales',expectedVersion:99,warehouseId:`missing-${key()}`,priceListId:`missing-${key()}`});
 await expect(saveOrder(f.owner,id,input,command)).rejects.toMatchObject({status:422,code:'ORDER_KIND_LOCKED'});
 expect(await getOrder(f.owner,id)).toEqual(before);
 expect((await db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${id}:%`,`${id}-line-%`).all()).results).toEqual(beforeMoney);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),table).toBeNull();
 const cases=[
  {label:'changed kind and reversed dates',raw:{...original,kind:'sales',expectedVersion:99,orderDate:'2026-09-10',dueDate:'2026-09-02'}},
  {label:'changed kind and duplicate products',raw:{...original,kind:'sales',expectedVersion:99,lines:[original.lines[0],original.lines[0]]}},
 ] as const;
 for(const example of cases){
  // Parsing through the retained schema proves these are semantic, rather
  // than schema-level, errors.
  const input=orderSchema.parse(example.raw),attempt=key();
  await expect(saveOrder(f.owner,id,input,attempt),example.label).rejects.toMatchObject({status:422,code:'ORDER_KIND_LOCKED'});
  expect(await getOrder(f.owner,id),example.label).toEqual(before);
  expect((await db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${id}:%`,`${id}-line-%`).all()).results,example.label).toEqual(beforeMoney);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(attempt).first(),`${example.label}: ${table}`).toBeNull();
 }
});

it('orders zero quantity failures after kind warehouse list and product checks',async()=>{
 const original=orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]});
 const created=await saveOrder(f.owner,null,original,key()),existingId=String(created.id),before=await getOrder(f.owner,existingId);
 const moneyBefore=(await db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${existingId}:%`,`${existingId}-line-%`).all()).results;
 const zero={productId,quantity:'0',price:'120',taxBps:700};
 const cases=[
  {label:'changed kind',id:existingId,raw:{...original,kind:'sales',expectedVersion:99,lines:[zero]},code:'ORDER_KIND_LOCKED'},
  {label:'warehouse unavailable',id:null,raw:{...original,warehouseId:`missing-${key()}`,lines:[zero]},code:'WAREHOUSE_INVALID'},
  {label:'price list unavailable',id:null,raw:{...original,priceListId:`missing-${key()}`,lines:[zero]},code:'PRICE_LIST_INVALID'},
  {label:'product unavailable',id:null,raw:{...original,lines:[{...zero,productId:`missing-${key()}`}]},code:'PRODUCT_INVALID'},
  {label:'zero quantity after valid sources',id:null,raw:{...original,lines:[zero]},code:'INTERNAL_ERROR'},
 ] as const;
 const {errorResponse}=await import('@erpx/server-runtime/public/http');
 for(const example of cases){
  const parsed=orderSchema.parse(example.raw),command=key(),newId=`order-${command}`;let failure:unknown;
  try{await saveOrder(f.owner,example.id,parsed,command)}catch(error){failure=error;}
  expect(failure,example.label).toBeInstanceOf(Error);
  if(example.code==='INTERNAL_ERROR'){
   expect(failure).toMatchObject({message:'Quantity must be positive.'});
   const response=errorResponse(failure),body=await response.json();expect(response.status).toBe(500);expect(body.error).toMatchObject({code:'INTERNAL_ERROR',message:'The request could not be completed.'});
  }else expect(failure,example.label).toMatchObject({status:422,code:example.code});
  expect(await getOrder(f.owner,existingId),example.label).toEqual(before);
  expect((await db.prepare("SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?) ORDER BY source_type,source_id,amount_kind").bind(f.company,`${existingId}:%`,`${existingId}-line-%`).all()).results,example.label).toEqual(moneyBefore);
  expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(newId).first(),example.label).toBeNull();
  expect(await db.prepare('SELECT 1 FROM commerce_order_lines WHERE order_id=?').bind(newId).first(),example.label).toBeNull();
  expect(await db.prepare('SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND (source_id LIKE ? OR source_id LIKE ?)').bind(f.company,`${newId}:%`,`${newId}-line-%`).first(),example.label).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first(),`${example.label}: ${table}`).toBeNull();
 }
});
