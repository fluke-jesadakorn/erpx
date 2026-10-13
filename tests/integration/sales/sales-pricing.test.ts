import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,saveOrder,orderSchema,transitionOrder,getOrder} from "../../../src/server/modules/commerce/public/commerce";
import {decidePriceList,savePriceList,updatePriceList} from "../../../src/server/modules/sales/public/sales-pricing";
import {assignFlowPrimary,resolveFlow} from "../../../src/server/modules/workflow/public/flow-routing";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const lateFailure=vi.hoisted(()=>({enabled:false}));
vi.mock("../../../src/server/modules/workflow/public/commands",async importOriginal=>{
 const original=await importOriginal<typeof import("../../../src/server/modules/workflow/public/commands")>();
 return {...original,executeCommand:async(...args:Parameters<typeof original.executeCommand>)=>{
  if(lateFailure.enabled&&args[3]==='sales.price-list.update')args[8]=[...args[8],args[0].prepare('SELECT 1/0')];
  return original.executeCommand(...args);
 }};
});
let f:Awaited<ReturnType<typeof expenseFixture>>,productId:string,customerId:string,warehouseId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.manager.role='sales_manager';f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,'sales_manager');warehouseId=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Pricing warehouse'},key())).id);productId=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Controlled service kit',price:'120',taxBps:700},key())).id);customerId=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Pricing customer'},key())).id);});

it('captures an independently activated price list and requires manager approval above the discount limit',async()=>{
 const draft=await savePriceList(f.owner,{name:'2026 customer prices',currency:'THB',customerId,validFrom:'2026-09-01',validUntil:'2026-12-31',maxAutoDiscountBps:1000,items:[{productId,price:'100',taxBps:700}]},key()),priceListId=String(draft.id);
 const edit={expectedVersion:1,name:'2026 customer prices revised',currency:'THB',customerId,validFrom:'2026-09-01',validUntil:'2026-12-31',maxAutoDiscountBps:1000,items:[{productId,price:'105',taxBps:700}]},editKey=key();
 expect(await updatePriceList(f.owner,priceListId,edit,editKey)).toMatchObject({version:2});
 expect(await updatePriceList(f.owner,priceListId,edit,editKey)).toMatchObject({version:2});
 await expect(updatePriceList(f.owner,priceListId,{...edit,name:'Stale overwrite'},key())).rejects.toMatchObject({status:409});
 await expect(decidePriceList(f.owner,priceListId,{expectedVersion:2,action:'activate',reason:'Self approval must be rejected'},key())).rejects.toMatchObject({code:'INDEPENDENT_PRICING_APPROVER_REQUIRED'});
 await decidePriceList(f.manager,priceListId,{expectedVersion:2,action:'activate',reason:'Commercial terms and tax treatment independently reviewed'},key());
 const source={kind:'quote' as const,partyId:customerId,warehouseId,orderDate:'2026-09-02',dueDate:'2026-09-10',priceListId,lines:[{productId,quantity:'2',price:'80',taxBps:700}]};
 await expect(saveOrder(f.owner,null,orderSchema.parse(source),key())).rejects.toMatchObject({code:'DISCOUNT_REASON_REQUIRED'});
 const created=await saveOrder(f.owner,null,orderSchema.parse({...source,discountReason:'Strategic launch order approved for commercial review'}),key()),id=String(created.id);
 let detail=await getOrder(f.owner,id);expect(detail.order).toMatchObject({price_list_id:priceListId,price_list_version:3,discount_approval_required:1,discount_approved_by:null});expect(detail.lines[0]).toMatchObject({list_price_minor:10500,price_minor:8000,discount_bps:2380});
 await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());
 const flow=await resolveFlow(db,f.owner,'order',id);await assignFlowPrimary(db,f.owner,'order',id,{stepKey:'submitted',userId:f.manager.userId,reason:'Assign independent sales discount reviewer',saveDefault:false,expectedVersion:flow.version,recordVersion:2},key());
 await expect(transitionOrder(f.manager,id,{action:'approve',expectedVersion:2},key())).rejects.toMatchObject({code:'DISCOUNT_APPROVAL_REASON_REQUIRED'});
 await transitionOrder(f.manager,id,{action:'approve',expectedVersion:2,reason:'Margin and customer commitment independently verified'},key());
 detail=await getOrder(f.owner,id);expect(detail.order).toMatchObject({state:'confirmed',discount_approved_by:f.manager.userId});
 await expect(db.prepare('UPDATE sales_price_list_items SET price_minor=1 WHERE price_list_id=?').bind(priceListId).run()).rejects.toThrow('SALES_PRICE_LIST_IMMUTABLE');
 await expect(db.prepare('DELETE FROM sales_price_lists WHERE id=?').bind(priceListId).run()).rejects.toThrow('SALES_PRICE_LIST_IMMUTABLE');
});

it('automatically applies a current general list and permits discounts inside delegated authority',async()=>{
 const draft=await savePriceList(f.owner,{name:'General September prices',currency:'THB',customerId:null,validFrom:'2026-09-01',validUntil:'2026-09-30',maxAutoDiscountBps:500,items:[{productId,price:'100',taxBps:700}]},key()),priceListId=String(draft.id);await decidePriceList(f.manager,priceListId,{expectedVersion:1,action:'activate',reason:'General September price control reviewed'},key());
 const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customerId,warehouseId,orderDate:'2026-09-03',dueDate:'2026-09-10',lines:[{productId,quantity:'1',price:'96',taxBps:700}]}),key()),detail=await getOrder(f.owner,String(order.id));expect(detail.order).toMatchObject({price_list_id:priceListId,discount_approval_required:0});expect(detail.lines[0]).toMatchObject({discount_bps:400});
});

it('keeps an approved current list available when a later dated list is activated',async()=>{
 const current=await savePriceList(f.owner,{name:'Current annual prices',currency:'THB',customerId:null,validFrom:'2026-01-01',validUntil:'2026-12-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700}]},key());
 await decidePriceList(f.manager,String(current.id),{expectedVersion:1,action:'activate',reason:'Current annual prices independently reviewed'},key());
 const future=await savePriceList(f.owner,{name:'October prices',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-12-31',maxAutoDiscountBps:0,items:[{productId,price:'120',taxBps:700}]},key());
 await decidePriceList(f.manager,String(future.id),{expectedVersion:1,action:'activate',reason:'Future effective prices independently reviewed'},key());
 const september=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-09-20',dueDate:'2026-09-25',lines:[{productId,quantity:'1',price:'100',taxBps:700}]}),key());
 const october=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customerId,warehouseId,orderDate:'2026-10-02',dueDate:'2026-10-10',lines:[{productId,quantity:'1',price:'120',taxBps:700}]}),key());
 expect((await getOrder(f.owner,String(september.id))).order.price_list_id).toBe(current.id);
 expect((await getOrder(f.owner,String(october.id))).order.price_list_id).toBe(future.id);
});

it('serializes concurrent revision commands and replays the winning immutable command',async()=>{
 const draft=await savePriceList(f.owner,{name:'Concurrent price control',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-10-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700}]},key()),id=String(draft.id);
 const change={expectedVersion:1,name:'Concurrent revised price control',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-10-31',maxAutoDiscountBps:0,items:[{productId,price:'110',taxBps:700}]},keys=[key(),key()];
 const results=await Promise.allSettled(keys.map(command=>updatePriceList(f.owner,id,change,command)));
 expect(results.filter(result=>result.status==='fulfilled')).toHaveLength(1);
 const winner=results.findIndex(result=>result.status==='fulfilled');expect(await updatePriceList(f.owner,id,change,keys[winner])).toMatchObject({id,version:2});
 expect(await db.prepare('SELECT version FROM sales_price_lists WHERE id=?').bind(id).first()).toEqual({version:2});
 expect(await db.prepare('SELECT COUNT(*)::int count FROM sales_price_list_items WHERE price_list_id=?').bind(id).first()).toEqual({count:1});
});

it('rolls back item replacement, list revision, receipt, audit and outbox after a late PostgreSQL failure',async()=>{
 const draft=await savePriceList(f.owner,{name:'Atomic price control',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-10-31',maxAutoDiscountBps:0,items:[{productId,price:'100',taxBps:700}]},key()),id=String(draft.id),command=key();
 lateFailure.enabled=true;
 try{await expect(updatePriceList(f.owner,id,{expectedVersion:1,name:'Must roll back',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-10-31',maxAutoDiscountBps:0,items:[{productId,price:'1',taxBps:700}]},command)).rejects.toThrow();}finally{lateFailure.enabled=false;}
 expect(await db.prepare('SELECT name,version FROM sales_price_lists WHERE id=?').bind(id).first()).toEqual({name:'Atomic price control',version:1});
 expect(await db.prepare('SELECT price_minor FROM sales_price_list_items WHERE price_list_id=?').bind(id).first()).toEqual({price_minor:10000});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT COUNT(*)::int count FROM ${table} WHERE command_id=?`).bind(command).first()).toEqual({count:0});
});
