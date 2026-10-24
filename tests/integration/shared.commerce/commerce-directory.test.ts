import {resolveFlow,assignFlowPrimary} from '@erpx/server-workflow/public/flow-routing';
import { afterEach,beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import {commerceOwnerTransaction} from "@erpx/server-commerce/public/purchase-boundary";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import { commercePage, saveMaster,saveOrder,getOrder,transitionOrder,orderSchema } from "@erpx/server-commerce/public/commerce";
import {workOrderRows} from "@erpx/server-commerce/public/commerce";
import {enrichFlowWork,otherFlowWork} from "@erpx/server-workflow/public/flow-work";
import {commerceSourceReadScope} from "@erpx/server-commerce/public/record-purchases";
import {listStaffNotifications} from "@erpx/server-collaboration/public/staff-notifications";
import {publishInboxEvent,inboxPage,inboxSummary} from "@erpx/server-collaboration/public/inbox";
import {committedSourceVisibility} from "../../support/helpers/source-writer-visibility";
import {measureReads} from "@erpx/server-runtime/public/runtime/read-metrics";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

afterEach(()=>vi.unstubAllEnvs());
let fixture: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { fixture = await expenseFixture(); });

it("keeps customer and supplier directories separate over the shared party records", async () => {
  const customer = await saveMaster(fixture.owner, "parties", null, { kind: "customer", code: key(), name: "Directory customer" }, key());
  const supplier = await saveMaster(fixture.owner, "parties", null, { kind: "supplier", code: key(), name: "Directory supplier" }, key());

  const customers = await commercePage(fixture.owner, { view: "parties", kind: "customer" });
  const suppliers = await commercePage(fixture.owner, { view: "parties", kind: "supplier" });
  const customerIds = customers.parties.map(row => row.id);
  const supplierIds = suppliers.parties.map(row => row.id);

  expect(customerIds).toContain(customer.id);
  expect(customerIds).not.toContain(supplier.id);
  expect(supplierIds).toContain(supplier.id);
  expect(supplierIds).not.toContain(customer.id);
  expect(customers.parties.every(row => row.kind === "customer")).toBe(true);
  expect(suppliers.parties.every(row => row.kind === "supplier")).toBe(true);
});


async function purchased(packageId:string,state:'active'|'draining'='active',endsAt:number|null=null){
 const store=packageGrantStore(db,{authorize:async(_tx,company)=>{if(company!==fixture.company)throw new Error('FICTIONAL_PROVIDER_SCOPE');return {userId:'fictional-commerce-provider'};},obligations:async()=>{throw new Error('NO_INACTIVE_CUTOVER_IN_COMMERCE_FIXTURE');}});
 const previous=(await readCompanyPackageGrants(db,fixture.company)).find(row=>row.packageId===packageId);
 return store.apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:fixture.company,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(previous?.version??0)+1,reason:'Isolated Commerce owner purchase fixture',evidence:'test:commerce-purchase-owner-boundary'}});
}
async function orderInput(kind:'quote'|'sales'|'purchase'='sales'){
 const warehouse=await saveMaster(fixture.owner,'warehouses',null,{branchId:fixture.branch,code:key(),name:'Purchased Commerce warehouse'},key()),product=await saveMaster(fixture.owner,'products',null,{sku:key(),name:'Purchased Commerce stock item',price:'10'},key()),party=await saveMaster(fixture.owner,'parties',null,{kind:kind==='purchase'?'supplier':'customer',code:key(),name:'Purchased Commerce party'},key());
 return orderSchema.parse({kind,partyId:String(party.id),warehouseId:String(warehouse.id),orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:String(product.id),quantity:'1',price:'10',taxBps:0}]});
}
it('guards actual order and collection history including supplied readers, while shared references and shadow preserve their normal owner semantics',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const input=await orderInput(),command=key();
 await expect(saveOrder(fixture.owner,null,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(commercePage(fixture.owner,{view:'sales'})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','shadow');const shadow=await saveOrder(fixture.owner,null,input,command);expect((await getOrder(fixture.owner,String(shadow.id),db)).order.id).toBe(shadow.id);const purchaseInput=await orderInput('purchase'),purchase=await saveOrder(fixture.owner,null,purchaseInput,key());
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('supply');await expect(getOrder(fixture.owner,String(shadow.id),db)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});for(const view of ['purchase','products']){const page=await commercePage(fixture.owner,{view,new:'1'});expect(page.options?.orders.map(row=>row.id)).toContain(purchase.id);expect(page.options?.orders.map(row=>row.id)).not.toContain(shadow.id);expect(page.options?.products.length).toBeGreaterThan(0);}await purchased('sales');expect((await getOrder(fixture.owner,String(shadow.id),db)).order.kind).toBe('sales');
 await expect(getOrder({...fixture.owner,branchId:'foreign-branch'},String(shadow.id),db)).rejects.toMatchObject({status:404});await expect(saveOrder(fixture.owner,String(shadow.id),{...input,kind:'purchase',expectedVersion:1},key())).rejects.toMatchObject({code:'ORDER_KIND_LOCKED'});
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),fixture.owner.assignmentId).run();await expect(getOrder(fixture.owner,String(shadow.id),db)).rejects.toMatchObject({code:'COMMERCE_ACCESS_CHANGED'});
});
it('requires active actual quote conversion and order amendments, retains exact replay in draining, and does not rewrite original receipts or assignments',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('sales');const input=await orderInput('quote'),command=key(),created=await saveOrder(fixture.owner,null,input,command),id=String(created.id),manager={...fixture.head,branchId:null,departmentId:null,role:'sales_manager',assignmentId:await fixture.grant(fixture.head,'sales_manager')};
 await transitionOrder(fixture.owner,id,{action:'submit',expectedVersion:1},key());await transitionOrder(manager,id,{action:'approve',expectedVersion:2},key());const flow=await resolveFlow(db,fixture.owner,'order',id);await assignFlowPrimary(db,fixture.owner,'order',id,{stepKey:'confirmed',userId:fixture.owner.userId,reason:'Select the actual quotation conversion owner',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());const converted=await transitionOrder(fixture.owner,id,{action:'convert',expectedVersion:3},key());expect((await getOrder(fixture.owner,String(converted.salesId))).order).toMatchObject({kind:'sales',source_id:id});
 const receipt=await db.prepare('SELECT * FROM command_receipts WHERE command_id=?').bind(command).first(),assignments=(await db.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(fixture.company).all()).results;
 await purchased('sales','draining');expect(await saveOrder(fixture.owner,null,input,command)).toEqual(created);await expect(saveOrder(fixture.owner,null,{...input,note:'Changed historical input'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(saveOrder(fixture.owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(saveOrder(fixture.owner,String(converted.salesId),{...input,kind:'sales',expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await transitionOrder(fixture.owner,String(converted.salesId),{action:'cancel',expectedVersion:1},key());expect((await getOrder(fixture.owner,String(converted.salesId))).order.state).toBe('cancelled');expect(await db.prepare('SELECT * FROM command_receipts WHERE command_id=?').bind(command).first()).toEqual(receipt);expect((await db.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(fixture.company).all()).results).toEqual(assignments);
});
it('composes only registered owned Ecommerce-style transactions and rolls back late authority changes after all caller effects',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('sales');const input=await orderInput(),unknown=key();await expect(db.transaction(tx=>saveOrder(fixture.owner,null,input,unknown,tx))).rejects.toMatchObject({code:'COMMERCE_COMPOSITION_REQUIRED'});expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(unknown).first()).toBeNull();
 const command=key();await expect(commerceOwnerTransaction(db,fixture.owner,async tx=>{await saveOrder(fixture.owner,null,input,command,tx);await tx.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),fixture.owner.assignmentId).run();})).rejects.toMatchObject({code:'COMMERCE_ACCESS_CHANGED'});
 expect(await db.prepare('SELECT 1 FROM commerce_orders WHERE id=?').bind(`order-${command}`).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(fixture.owner.assignmentId).first('revoked_at')).toBeNull();
 const accepted=await commerceOwnerTransaction(db,fixture.owner,tx=>saveOrder(fixture.owner,null,input,key(),tx));expect((await getOrder(fixture.owner,String(accepted.id))).order.state).toBe('draft');
});
it('checks purchase expiry at the complete caller boundary and rolls back the order, receipt, audit and outbox together',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');const input=await orderInput(),command=key();await purchased('sales','active',Date.now()+3000);
 await expect(commerceOwnerTransaction(db,fixture.owner,async tx=>{await saveOrder(fixture.owner,null,input,command,tx);await tx.prepare('SELECT pg_sleep(3.15)').run();})).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});
 for(const [table,column,value] of [['commerce_orders','id',`order-${command}`],['command_receipts','command_id',command],['audit_events','command_id',command],['outbox_events','command_id',command]])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE ${column}=?`).bind(value).first()).toBeNull();
});
it('keeps Commerce Work on actual purchased order kinds and preserves draining no-action after flow enrichment',async()=>{
 const sales=await saveOrder(fixture.owner,null,await orderInput(),key()),purchase=await saveOrder(fixture.owner,null,await orderInput('purchase'),key()),salesId=String(sales.id),purchaseId=String(purchase.id);
 const quote=await saveOrder(fixture.owner,null,await orderInput('quote'),key()),quoteId=String(quote.id),manager={...fixture.head,branchId:null,departmentId:null,role:'sales_manager',assignmentId:await fixture.grant(fixture.head,'sales_manager')};await transitionOrder(fixture.owner,quoteId,{action:'submit',expectedVersion:1},key());await transitionOrder(manager,quoteId,{action:'approve',expectedVersion:2},key());
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');expect(await workOrderRows(fixture.owner,db)).toHaveLength(0);await purchased('supply');
 expect((await workOrderRows(fixture.owner,db)).map(row=>row.id)).toEqual([purchaseId]);for(const reference of [false,true]){const work=await otherFlowWork(fixture.owner,reference);expect(work.some(row=>row.id===salesId)).toBe(false);expect(work.some(row=>row.id===purchaseId)).toBe(true);}
 await purchased('sales');await purchased('sales','draining');const flow=await resolveFlow(db,fixture.owner,'order',salesId),item={id:salesId,type:'order' as const,title:'Retained Sales source',href:`/commerce/orders/${salesId}`,detail:'Review',status:'draft',updatedAt:Date.now(),queue:'todo' as const,canAct:true,relationships:['assigned' as const,'approval' as const],flow:{...flow,canAct:true,canManage:true}};
 const [enriched]=await enrichFlowWork(fixture.owner,[item]);expect(enriched).toMatchObject({id:salesId,canAct:false,queue:'waiting',actionKind:'work',flow:{canAct:false,canManage:false}});expect(enriched.relationships).not.toEqual(expect.arrayContaining(['assigned','approval']));
 const quoteFlow=await resolveFlow(db,fixture.owner,'order',quoteId);expect((await enrichFlowWork(fixture.owner,[{...item,id:quoteId,flow:{...quoteFlow,canAct:true,canManage:true}}]))[0]).toMatchObject({id:quoteId,canAct:false,flow:{canAct:false,canManage:false}});
 const narrowed={...fixture.owner,iam:{permissions:[],grantablePermissions:[],principalId:'fictional-request-narrowing',personId:null,roleRevision:1,identityVersion:0,freshUntil:null}};expect(await enrichFlowWork(narrowed,[item])).toEqual([]);
 expect(await workOrderRows({...fixture.owner,branchId:key()},db)).toHaveLength(0);await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),fixture.owner.assignmentId).run();expect(await enrichFlowWork(fixture.owner,[item])).toEqual([]);
});
it('filters actual Commerce notifications and Records before limits without changing durable events or shared references',async()=>{
 const sales=await saveOrder(fixture.owner,null,await orderInput(),key()),purchase=await saveOrder(fixture.owner,null,await orderInput('purchase'),key()),salesId=String(sales.id),purchaseId=String(purchase.id),now=Date.now();
 for(let index=0;index<106;index++){
  const id=index===0?purchaseId:salesId,message=index===0?'Visible Supply source':'Hidden Sales source';
  await db.prepare('INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,\'commerce_order\',?,?,?,?)').bind(key(),fixture.company,fixture.owner.userId,id,index+1,message,now+index).run();
  await publishInboxEvent(db,{key:key(),companyId:fixture.company,sourceType:'commerce_order',sourceId:id,sourceVersion:1,category:'work',preview:message,recipientIds:[fixture.owner.userId]});
 }
 // Native owner IDs intersect generic Records only when they really exist;
 // a non-order Sales reference remains its original owner's responsibility.
 for(const [id,module,title] of [[purchaseId,'procurement','Owner source'],[salesId,'sales','Owner source'],[key(),'sales','Shared owner reference']])await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES(?,?, 'overview',?,'','draft','{}',?,?,?,?,?)").bind(module,id,title,fixture.company,fixture.owner.userId,fixture.owner.userId,now,now).run();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('supply');
 const notices=await listStaffNotifications(db,fixture.owner);expect(notices.map(row=>row.message)).toEqual(['Visible Supply source']);const inbox=await inboxPage(db,fixture.owner,{view:'all',category:'work'});expect(inbox.items.map(row=>row.message)).toEqual(['Visible Supply source','Visible Supply source']);expect((await inboxSummary(db,fixture.owner)).categories.work).toBe(2);
 const scope=await commerceSourceReadScope(db,fixture.owner,'records'),rows=(await db.prepare(`SELECT id,title FROM workspace_records WHERE company_id=? AND ${scope.where} ORDER BY id LIMIT 12`).bind(fixture.company,...scope.args).all<{id:string;title:string}>()).results;expect(rows.map(row=>row.id)).toContain(purchaseId);expect(rows.map(row=>row.id)).not.toContain(salesId);expect(rows.some(row=>row.title==='Shared owner reference')).toBe(true);
 expect(await db.prepare('SELECT COUNT(*) n FROM staff_notifications WHERE company_id=?').bind(fixture.company).first('n')).toBe(106);expect(await db.prepare('SELECT COUNT(*) n FROM inbox_events WHERE company_id=?').bind(fixture.company).first('n')).toBe(212);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),fixture.owner.assignmentId).run();expect(await listStaffNotifications(db,fixture.owner)).toEqual([]);expect((await inboxPage(db,fixture.owner,{view:'all'})).items).toEqual([]);
});

it('guards direct Commerce Flow history and assignment with actual purchases, current authority and exact draining replay',async()=>{
 const order=await saveOrder(fixture.owner,null,await orderInput(),key()),id=String(order.id);
 // The generic Flow actor intentionally has no product/Holding context; its
 // supplied owner pool and original Company assignment remain authoritative.
 const {userId,assignmentId,role,companyId,branchId,departmentId,authority,iam}=fixture.owner;
 const actor={userId,assignmentId,role,companyId,branchId,departmentId,authority,iam};
 const view=await resolveFlow(db,actor,'order',id),input={stepKey:'draft',userId:actor.userId,expectedVersion:view.version,recordVersion:view.recordVersion,reason:'Keep the original Company workflow owner',saveDefault:false},command=key();
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 await expect(resolveFlow(db,actor,'order',id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(assignFlowPrimary(db,actor,'order',id,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
 await purchased('sales');
 await expect(db.transaction(tx=>assignFlowPrimary(tx,actor,'order',id,input,key()))).rejects.toMatchObject({code:'COMMERCE_TRANSACTION_REQUIRED'});
 const result=await assignFlowPrimary(db,actor,'order',id,input,command);
 await purchased('sales','draining');
 expect(await resolveFlow(db,actor,'order',id)).toMatchObject({canAct:false,canManage:false});
 expect(await assignFlowPrimary(db,actor,'order',id,input,command)).toEqual(result);
 await expect(assignFlowPrimary(db,actor,'order',id,{...input,expectedVersion:Number(result.version)},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(assignFlowPrimary(db,actor,'order',id,{...input,stepKey:'invoicing',expectedVersion:Number(result.version)},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(resolveFlow(db,{...actor,companyId:key()},'order',id)).rejects.toMatchObject({status:404});
 await expect(resolveFlow(db,{...actor,iam:{permissions:[],grantablePermissions:[],principalId:'fictional-flow-narrowing',personId:null,roleRevision:1,identityVersion:0,freshUntil:null}},'order',id)).rejects.toMatchObject({code:'COMMERCE_ACCESS_CHANGED'});
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();
 await expect(assignFlowPrimary(db,actor,'order',id,input,command)).rejects.toMatchObject({code:'COMMERCE_ACCESS_CHANGED'});
});

it('rolls back Commerce Flow assignment, defaults, events and receipt when authority changes after its original effects',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('sales');
 const order=await saveOrder(fixture.owner,null,await orderInput(),key()),id=String(order.id),view=await resolveFlow(db,fixture.owner,'order',id);
 const input={stepKey:'draft',userId:fixture.owner.userId,expectedVersion:view.version,recordVersion:view.recordVersion,reason:'Assign with current authority in the same transaction',saveDefault:true},command=key(),name=`flow_guard_${key().replaceAll('-','')}`;
 const snapshot=async()=>({routes:(await db.prepare('SELECT * FROM flow_routes WHERE company_id=? ORDER BY record_id').bind(fixture.company).all()).results,defaults:(await db.prepare('SELECT * FROM flow_defaults WHERE company_id=? ORDER BY revision').bind(fixture.company).all()).results,events:(await db.prepare('SELECT * FROM flow_events WHERE company_id=? ORDER BY id').bind(fixture.company).all()).results,notifications:(await db.prepare('SELECT * FROM staff_notifications WHERE company_id=? ORDER BY id').bind(fixture.company).all()).results});
 const before=await snapshot();
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.owner.assignmentId}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON flow_events FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`).run();
 try{
  await expect(assignFlowPrimary(db,fixture.owner,'order',id,input,command)).rejects.toMatchObject({code:'COMMERCE_ACCESS_CHANGED'});
  expect(await snapshot()).toEqual(before);
  expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
  expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(fixture.owner.assignmentId).first('revoked_at')).toBeNull();
 }finally{await db.prepare(`DROP TRIGGER ${name} ON flow_events`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 const result=await assignFlowPrimary(db,fixture.owner,'order',id,input,command);
 expect(await assignFlowPrimary(db,fixture.owner,'order',id,input,command)).toEqual(result);
});


it.each(['product','customer'] as const)('leases the actual %s reference before its first Commerce command snapshot without transaction retry',async reference=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchased('sales');const input=await orderInput(),command=key(),changed=`Committed ${reference} before native source lease`;
 const observed=await committedSourceVisibility(db,async transaction=>{
  if(reference==='product')await transaction.prepare('UPDATE commerce_products SET name=?,version=version+1 WHERE id=?').bind(changed,input.lines[0].productId).run();
  else await transaction.prepare('UPDATE finance_parties SET name=?,version=version+1 WHERE id=?').bind(changed,input.partyId).run();
 },()=>measureReads(()=>saveOrder(fixture.owner,null,input,command)));
 expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);
 const detail=await getOrder(fixture.owner,String(observed.result.id));
 if(reference==='product')expect(detail.lines[0].description).toBe(changed);else expect(detail.order.party_name).toBe(changed);
 expect(await saveOrder(fixture.owner,null,input,command)).toEqual(observed.result);
});
