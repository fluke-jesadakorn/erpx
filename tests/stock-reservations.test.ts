import { fulfillmentSchema } from '../src/server/commerce';
import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {deliveryEvidence} from './helpers/delivery-evidence';
import {changeOrderReservation,fulfillOrder,getOrder,orderSchema,saveMaster,saveOrder,transitionOrder} from '../src/server/commerce';
import {escalateInventoryReservations} from '../src/server/inventory-reservation-alerts';
import {listStaffNotifications} from '../src/server/staff-notifications';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,warehouse:string,product:string,party:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');
 warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Reservation test warehouse'},key())).id);
 product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Reservation test product',price:'10'},key())).id);
 party=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Reservation test customer'},key())).id);
 // Opening stock fixture, not a fabricated delivery or reservation.
 await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();
});
async function order(){
 const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party,warehouseId:warehouse,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product,quantity:'2',price:'10',taxBps:0}]}),key());
 const id=String(created.id);await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,id,{action:'approve',expectedVersion:2},key());
 return getOrder(f.owner,id);
}
const reserve=(id:string,lineId:string,quantity='2',expectedVersion=3,expiresAt=Date.now()+3600000,command=key())=>changeOrderReservation(f.owner,id,{expectedVersion,action:'reserve',lineId,quantity,expiresAt,reason:'Customer dispatch commitment'},command);
it('reserves the last stock once under competing orders and preserves source scope and replay',async()=>{
 const a=await order(),b=await order();
 const results=await Promise.allSettled([reserve(String(a.order.id),String(a.lines[0].id)),reserve(String(b.order.id),String(b.lines[0].id))]);
 expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 expect(await db.prepare('SELECT SUM(quantity_milli) n FROM inventory_active_reservations WHERE warehouse_id=?').bind(warehouse).first('n')).toBe(2000);
 const winner=results[0].status==='fulfilled'?a:b;
 await expect(changeOrderReservation({...f.owner,branchId:'another-branch'},String(winner.order.id),{expectedVersion:4,action:'release',lineId:winner.lines[0].id,quantity:'1',reason:'Wrong branch attempt'},key())).rejects.toMatchObject({status:404});
 const releaseKey=key(),input={expectedVersion:4,action:'release',lineId:winner.lines[0].id,quantity:'1',reason:'Customer changed dispatch quantity'};
 const first=await changeOrderReservation(f.owner,String(winner.order.id),input,releaseKey);expect(await changeOrderReservation(f.owner,String(winner.order.id),input,releaseKey)).toEqual(first);
 await expect(changeOrderReservation(f.owner,String(winner.order.id),{...input,quantity:'2'},releaseKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 expect(await db.prepare('SELECT SUM(quantity_milli) n FROM inventory_active_reservations WHERE warehouse_id=?').bind(warehouse).first('n')).toBe(1000);
 await expect(reserve(String(winner.order.id),String(winner.lines[0].id),'1',4)).rejects.toMatchObject({code:'CONFLICT'});
});
it('consumes only its source reservation with partial deliveries and leaves quantities and costs consistent',async()=>{
 const a=await order(),b=await order();await reserve(String(a.order.id),String(a.lines[0].id));
 await expect(fulfillOrder(f.owner,String(b.order.id),fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-02',lines:[{lineId:String(b.lines[0].id),quantity:'1'}],...(await deliveryEvidence(db,f.owner,String(b.order.id)))}),key())).rejects.toMatchObject({code:'INSUFFICIENT_STOCK'});
 await fulfillOrder(f.owner,String(a.order.id),fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-02',lines:[{lineId:String(a.lines[0].id),quantity:'1'}],...(await deliveryEvidence(db,f.owner,String(a.order.id)))}),key());
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:1000,value_minor:600});
 expect(await db.prepare('SELECT quantity_milli,consumed_milli,released_milli FROM inventory_order_reservations WHERE order_line_id=?').bind(a.lines[0].id).first()).toEqual({quantity_milli:2000,consumed_milli:1000,released_milli:0});
 await fulfillOrder(f.owner,String(a.order.id),fulfillmentSchema.parse({expectedVersion:5,date:'2026-09-02',lines:[{lineId:String(a.lines[0].id),quantity:'1'}],...(await deliveryEvidence(db,f.owner,String(a.order.id)))}),key());
 expect((await getOrder(f.owner,String(a.order.id))).order.state).toBe('fulfilled');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});
 expect(await db.prepare('SELECT COUNT(*) n FROM inventory_active_reservations WHERE warehouse_id=?').bind(warehouse).first('n')).toBe(0);
});
it('expires without requiring a worker, cancels atomically, and prevents direct stock reductions below reservations',async()=>{
 const a=await order();await reserve(String(a.order.id),String(a.lines[0].id));
 await expect(db.prepare('UPDATE commerce_stock SET quantity_milli=0 WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).run()).rejects.toMatchObject({message:'STOCK_RESERVED'});
 // Simulate elapsed time in persisted fixture, leaving original quantity/history intact.
 await db.prepare('UPDATE inventory_order_reservations SET created_at=1,expires_at=2 WHERE order_line_id=?').bind(a.lines[0].id).run();
 expect((await getOrder(f.owner,String(a.order.id))).lines[0].reserved_milli).toBe(0);
 await expect(changeOrderReservation(f.owner,String(a.order.id),{expectedVersion:4,action:'release',lineId:a.lines[0].id,quantity:'1',reason:'Cannot release expired availability'},key())).rejects.toMatchObject({code:'OVER_RELEASE'});
 await reserve(String(a.order.id),String(a.lines[0].id),'1',4);
 await changeOrderReservation(f.owner,String(a.order.id),{expectedVersion:5,action:'release',lineId:a.lines[0].id,quantity:'1',reason:'Release current reservation, not expired history'},key());
 expect((await getOrder(f.owner,String(a.order.id))).lines[0].reserved_milli).toBe(0);
 expect(await db.prepare('SELECT released_milli FROM inventory_order_reservations WHERE order_line_id=? AND expires_at=2').bind(a.lines[0].id).first('released_milli')).toBe(0);

 const b=await order();await reserve(String(b.order.id),String(b.lines[0].id));
 await transitionOrder(f.head,String(b.order.id),{action:'cancel',expectedVersion:4},key());
 expect(await db.prepare('SELECT released_milli,release_reason FROM inventory_order_reservations WHERE order_line_id=?').bind(b.lines[0].id).first()).toEqual({released_milli:2000,release_reason:'Source order cancelled'});
 expect(await db.prepare('SELECT SUM(quantity_milli) n FROM inventory_active_reservations WHERE warehouse_id=?').bind(warehouse).first('n')).toBeNull();
});
it('warns the sales owner before expiry, escalates after expiry, and deduplicates every alert',async()=>{
 const now=Date.now(),a=await order();await reserve(String(a.order.id),String(a.lines[0].id),'2',3,now+3600000);
 expect((await escalateInventoryReservations(db,now)).expiring).toBeGreaterThanOrEqual(1);
 expect(await escalateInventoryReservations(db,now)).toEqual({expiring:0,expired:0});
 expect((await listStaffNotifications(db,f.owner)).filter(n=>n.record_id===a.order.id)).toEqual([expect.objectContaining({record_type:'commerce_order',href:`/commerce/orders/${a.order.id}`,message:expect.stringContaining('expires within 24 hours')})]);
 await db.prepare('UPDATE inventory_order_reservations SET created_at=?,expires_at=? WHERE order_line_id=?').bind(now-2,now-1,a.lines[0].id).run();
 expect((await escalateInventoryReservations(db,now)).expired).toBeGreaterThanOrEqual(1);
 expect((await listStaffNotifications(db,f.owner)).filter(n=>n.record_id===a.order.id)).toHaveLength(2);
});
