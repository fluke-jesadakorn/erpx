import {seedCommerceFulfillment} from "../../support/helpers/commerce-fulfillment-seed";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {beforeAll,beforeEach,afterEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {deliveryEvidence} from "../../support/helpers/delivery-evidence";
import {getQualityHold,qualityWork} from "@erpx/server-quality/public/quality";
import {seedNativeQualityHold,seedNativeQualityRelease} from "../../../fixtures/quality/hold";
import {seedNativeQualitySpec} from "../../../fixtures/quality/specification";
import {commercePage,changeOrderReservation,fulfillmentSchema,orderSchema,saveMaster,saveOrder,transitionOrder} from '@erpx/server-commerce/public/commerce';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {mergeWorkItems} from "@erpx/server-work_projection/public/work-items";
import {filterWork} from "@erpx/feature-work_projection/lib/work-item";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:qualityDatabase??env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>{qualityDatabase=undefined;vi.unstubAllEnvs();});
let qualityDatabase:SqlDatabase|undefined;
let f:Awaited<ReturnType<typeof expenseFixture>>,warehouse:string,product:string,party:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();for(const [actor,role] of [[f.head,'sales_manager'],[f.reviewer,'quality_inspector'],[f.manager,'quality_manager']] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}await db.prepare("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,status,created_at,updated_at) VALUES(?,?,?,'5400','Stock variance','expense','THB','active',1,1)").bind(`variance-${key()}`,f.company,f.chart).run();warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Quality test warehouse'},key())).id);product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Quality test stock',price:'10'},key())).id);party=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Quality test customer'},key())).id);await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();});
async function hold(){return seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:product,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first('version')),dueDate:'2026-09-22',reason:'Inspect damaged packaging before dispatch'},key());}
async function inspect(holdId:string,expectedVersion:number,result:'pass'|'fail',evidence:string,at:number){const id=`inspection-${key()}`;await db.prepare('INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(id,holdId,expectedVersion,result,evidence,f.reviewer.userId,at).run();await db.prepare("UPDATE inventory_quality_holds SET status=?,last_result=?,last_inspector=?,version=version+1,updated_at=? WHERE id=?").bind(result==='pass'?'awaiting_release':'held',result,f.reviewer.userId,at,holdId).run();return id;}

it('captures an independently activated measurement specification and derives immutable results',async()=>{
 const seed=await seedNativeQualitySpec(db,f.owner,{productId:product,name:'Receiving dimensional acceptance',status:'active',approvedBy:f.manager.userId,approvalReason:'Limits checked against the approved receiving plan',metrics:[{code:'weight',label:'Package weight',unit:'kg',minimum:'9.9',maximum:'10.1'},{code:'temperature',label:'Surface temperature',unit:'°C',minimum:'',maximum:'30'}]},key()),specId=String(seed.id);
 const created=await hold(),id=String(created.id),held=await getQualityHold(f.reviewer,id,db),metrics=(await db.prepare('SELECT * FROM inventory_quality_spec_metrics WHERE spec_id=? ORDER BY ordinal').bind(specId).all<Record<string,string|number|null>>()).results;
 expect(held).toMatchObject({spec_id:specId,spec_version:2});expect(metrics).toHaveLength(2);
 const failed=await inspect(id,1,'fail','Weight exceeds the approved receiving tolerance',Date.now()),passed=await inspect(id,2,'pass','Repacked item meets every captured acceptance limit',Date.now()+1);
 const snapshot=[[failed,['10.2','25']],[passed,['10.0','25']]] as const;
 for(const [inspectionId,values] of snapshot)for(const [index,metric] of metrics.entries()){
  const value=values[index],measured=Number(value),minimum=metric.minimum_value===null?null:Number(metric.minimum_value),maximum=metric.maximum_value===null?null:Number(metric.maximum_value),ok=(minimum===null||measured>=minimum)&&(maximum===null||measured<=maximum);
  await db.prepare('INSERT INTO inventory_quality_measurements(id,inspection_id,metric_id,code,label,unit,minimum_value,maximum_value,measured_value,passed,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(`${inspectionId}-metric-${index}`,inspectionId,metric.id,metric.code,metric.label,metric.unit,metric.minimum_value,metric.maximum_value,value,ok?1:0,metric.ordinal).run();
 }
 const measurements=(await db.prepare('SELECT m.* FROM inventory_quality_measurements m JOIN inventory_quality_inspections i ON i.id=m.inspection_id WHERE i.hold_id=? ORDER BY i.created_at,m.ordinal').bind(id).all<Record<string,string|number|null>>()).results,firstInspection=String(await db.prepare('SELECT id FROM inventory_quality_inspections WHERE hold_id=? ORDER BY created_at,id LIMIT 1').bind(id).first('id'));
 expect(measurements).toHaveLength(4);expect(measurements.filter(m=>Number(m.passed)===0)).toHaveLength(1);
 await expect(db.prepare('UPDATE inventory_quality_measurements SET measured_value=10 WHERE inspection_id=?').bind(firstInspection).run()).rejects.toThrow('QUALITY_MEASUREMENT_IMMUTABLE');
 await expect(db.prepare("UPDATE inventory_quality_specs SET name='Changed' WHERE id=?").bind(specId).run()).rejects.toThrow('QUALITY_SPEC_IMMUTABLE');
 await expect(db.prepare('DELETE FROM inventory_quality_specs WHERE id=?').bind(specId).run()).rejects.toThrow('QUALITY_SPEC_IMMUTABLE');
});

it('keeps failed inspections and requires a third person to release a passing inspection',async()=>{
 const h=await hold(),id=String(h.id);expect((await commercePage(f.owner,{view:'stock'})).stock.find(s=>s.product_id===product)?.quality_held).toBe(1);
 await inspect(id,1,'fail','Packaging damaged; contents require reinspection',Date.now());
 expect(await getQualityHold(f.owner,id,db)).toMatchObject({status:'held',last_result:'fail',version:2});
 await seedNativeQualityRelease(db,{companyId:f.company,holdId:id,releasedBy:f.manager.userId,inspectorId:f.reviewer.userId,reason:'Reviewed both inspections and approved release'});
 expect((await getQualityHold(f.owner,id,db)).status).toBe('released');
 expect(await db.prepare('SELECT count(*) n FROM inventory_quality_inspections WHERE hold_id=?').bind(id).first('n')).toBe(2);
 await expect(db.prepare('DELETE FROM inventory_quality_inspections WHERE hold_id=?').bind(id).run()).rejects.toMatchObject({message:'QUALITY_INSPECTION_IMMUTABLE'});
});

it('blocks already reserved Sales delivery atomically until all overlapping holds are released',async()=>{const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party,warehouseId:warehouse,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product,quantity:'2',price:'10',taxBps:0}]}),key()),id=String(created.id);await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,id,{action:'approve',expectedVersion:2},key());const source=await getOrder(f.owner,id),lineId=String(source.lines[0].id);await changeOrderReservation(f.owner,id,{expectedVersion:3,action:'reserve',lineId,quantity:'2',expiresAt:Date.now()+3600000,reason:'Customer delivery promise'},key());const a=await hold(),b=await hold(),proof=await deliveryEvidence(db,f.owner,id);const deliver=()=>seedCommerceFulfillment(db, f.owner,id,fulfillmentSchema.parse({expectedVersion:4,date:'2026-09-02',lines:[{lineId,quantity:'2'}],...proof}),key());await expect(deliver()).rejects.toThrow('QUALITY_HOLD');expect((await getOrder(f.owner,id)).lines[0].reserved_milli).toBe(2000);expect((await getOrder(f.owner,id)).order.version).toBe(4);
 await seedNativeQualityRelease(db,{companyId:f.company,holdId:String(a.id),releasedBy:f.manager.userId,inspectorId:f.reviewer.userId,reason:'Independent release review complete'});
 await expect(deliver()).rejects.toThrow('QUALITY_HOLD');
 await seedNativeQualityRelease(db,{companyId:f.company,holdId:String(b.id),releasedBy:f.manager.userId,inspectorId:f.reviewer.userId,reason:'Independent release review complete'});
 await deliver();expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:0,value_minor:0});});

it('cancels an incorrect uninspected hold only after independent review, retaining other holds and evidence',async()=>{
 const a=await hold(),b=await hold(),id=String(a.id);
 async function cancellation(version:number,action:'request'|'reject'|'approve',status:'cancel_pending'|'held'|'cancelled',actor:string,reason:string){
  await db.prepare('INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES(?,?,?,?,?,?,?)').bind(`quality-cancel-${key()}`,id,version,action,reason,actor,Date.now()).run();
  await db.prepare("UPDATE inventory_quality_holds SET status=?,cancel_requested_by=CASE WHEN ?='request' THEN ? ELSE cancel_requested_by END,cancelled_by=CASE WHEN ?='approve' THEN ? ELSE cancelled_by END,cancelled_at=CASE WHEN ?='approve' THEN ? ELSE cancelled_at END,version=version+1,updated_at=? WHERE id=?").bind(status,action,actor,action,actor,action,Date.now(),Date.now(),id).run();
 }
 await cancellation(1,'request','cancel_pending',f.owner.userId,'Duplicate hold created for the wrong receiving review');
 expect(await getQualityHold(f.owner,id,db)).toMatchObject({status:'cancel_pending',last_result:null,version:2});
 expect((await qualityWork(f.manager)).find(x=>x.id===id)).toMatchObject({canAct:true,nextAction:'Review cancellation of uninspected hold'});
 await cancellation(2,'reject','held',f.manager.userId,'Verify supplier receipt before removing hold');expect((await getQualityHold(f.owner,id,db)).status).toBe('held');
 await cancellation(3,'request','cancel_pending',f.owner.userId,'Duplicate hold created for the wrong receiving review');
 await cancellation(4,'approve','cancelled',f.manager.userId,'Confirmed duplicate of the other active hold');
 expect((await getQualityHold(f.owner,id,db))).toMatchObject({status:'cancelled',last_result:null});
 expect((await getQualityHold(f.owner,String(b.id),db)).status).toBe('held');
 expect(await db.prepare('SELECT count(*) n FROM inventory_quality_inspections WHERE hold_id=?').bind(id).first('n')).toBe(0);
 expect(Number(await db.prepare('SELECT count(*) n FROM inventory_quality_cancellations WHERE hold_id=?').bind(id).first('n'))).toBe(4);
 await expect(db.prepare('DELETE FROM inventory_quality_cancellations WHERE hold_id=?').bind(id).run()).rejects.toThrow('QUALITY_CANCELLATION_IMMUTABLE');
 expect((await qualityWork(f.owner)).find(x=>x.id===id)?.queue).toBe('completed');
});

it('recovers quality due-soon and overdue escalations with live routing and persisted acknowledgement',async()=>{
 const {qualityAlerts,qualityAlertWork,getQualityAlert}=await import("@erpx/server-quality/public/quality-alerts");
 const {routeQualityAlertNotifications}=await import("@erpx/server-collaboration/public/quality-notifications");
 const due=await seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:product,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first('version')),dueDate:'2026-09-22',reason:'Prepare a quality inspection before the receiving deadline'},key());
 const overdue=await seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:product,expectedStockVersion:Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first('version')),dueDate:'2026-09-20',reason:'Recover a missed quality inspection and preserve the quarantine evidence'},key());
 const now=Date.parse('2026-09-21T12:00:00Z'),today=new Date(now).toISOString().slice(0,10),horizon=new Date(now+86_400_000).toISOString().slice(0,10);
 // The scheduled sweep is native (erpx_server_quality::public::quality_alerts
 // drain_quality_alerts, proved by rust-quality-alert-sweep-postgres). This case
 // seeds its exact idempotent identities so the retained read, route, work and
 // acknowledgement surfaces stay covered.
 for(const [phase,id,from,to] of [['due_soon','due-soon',today,horizon],['overdue','overdue',null,today]] as [string,string,string|null,string][])
  await db.prepare(`INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
   SELECT 'quality-alert-${id}-'||h.id,h.id,'${phase}',h.version,h.due_date::date,?
   FROM inventory_quality_holds h
   WHERE h.status NOT IN ('released','cancelled','disposed') AND ${from?'h.due_date>=? AND h.due_date<=?':'h.due_date<?'}
   ON CONFLICT DO NOTHING`).bind(now,...(from?[from,to]:[to])).run();
 await routeQualityAlertNotifications(db,now);
 expect((await db.prepare("SELECT id,phase,source_version,due_date::text due_date FROM inventory_quality_alerts WHERE hold_id IN (?,?) ORDER BY id").bind(String(due.id),String(overdue.id)).all<{id:string;phase:string;source_version:number;due_date:string}>()).results).toEqual([{id:`quality-alert-due-soon-${due.id}`,phase:'due_soon',source_version:1,due_date:'2026-09-22'},{id:`quality-alert-overdue-${overdue.id}`,phase:'overdue',source_version:1,due_date:'2026-09-20'}]);
 const alerts=await qualityAlerts(f.manager),dueAlert=alerts.find(a=>a.hold_id===String(due.id)&&a.phase==='due_soon'),overdueAlert=alerts.find(a=>a.hold_id===String(overdue.id)&&a.phase==='overdue');
 expect(dueAlert).toMatchObject({source_version:1,version:1,can_ack:1,recipients:2});
 expect(overdueAlert).toMatchObject({source_version:1,version:1,can_ack:1,recipients:2});
 expect((await qualityAlertWork(f.manager)).filter(w=>w.type==='quality_alert')).toEqual(expect.arrayContaining([expect.objectContaining({id:dueAlert?.id,queue:'todo',canAct:true,nextAction:'Prepare the next inspection or release action'}),expect.objectContaining({id:overdueAlert?.id,queue:'todo',canAct:true})]));
 const holdIds=[String(due.id),String(overdue.id)],work=mergeWorkItems([...(await qualityWork(f.manager)),...(await qualityAlertWork(f.manager))]);
 const escalations=filterWork(work,'all','quality_alert','').filter(row=>holdIds.includes(row.id));
 expect(escalations).toHaveLength(2);
 expect(escalations.every(row=>row.type==='quality_hold'&&row.source?.kind==='quality_hold'&&row.source.id===row.id&&row.source.version===1)).toBe(true);
 expect(escalations.find(row=>row.id===String(due.id))?.relatedAlerts).toEqual([expect.objectContaining({id:dueAlert?.id,title:expect.stringContaining('Quality hold due soon'),completed:false})]);
 expect(escalations.find(row=>row.id===String(overdue.id))?.relatedAlerts).toEqual([expect.objectContaining({id:overdueAlert?.id,title:expect.stringContaining('Overdue quality hold'),completed:false})]);
 expect(filterWork(work,'todo','quality_alert','Overdue quality hold').map(row=>row.id)).toEqual([String(overdue.id)]);
 const outside={...f.manager,branchId:'unrelated-branch'};
 expect(filterWork(mergeWorkItems([...(await qualityWork(outside)),...(await qualityAlertWork(outside))]),'all','quality_alert','')).toEqual([]);
 const {listStaffNotifications}=await import("@erpx/server-collaboration/public/staff-notifications");
 expect((await listStaffNotifications(db,f.manager)).filter(n=>n.record_type==='quality_alert'&&[dueAlert?.id,overdueAlert?.id].includes(n.record_id))).toHaveLength(2);
 await expect(getQualityAlert({...f.manager,branchId:'unrelated-branch'},String(dueAlert?.id))).rejects.toMatchObject({status:404});
 const acknowledgement={expectedVersion:1,reason:'Quality manager accepted the escalation and will coordinate the inspection owner'},acknowledgedAt=Date.now();
 await db.prepare("UPDATE inventory_quality_alerts SET acknowledged_by=?,acknowledged_at=?,reason=?,version=version+1 WHERE id=? AND version=? AND acknowledged_by IS NULL").bind(f.manager.userId,acknowledgedAt,acknowledgement.reason,String(dueAlert?.id),acknowledgement.expectedVersion).run();
 expect(await db.prepare('SELECT acknowledged_by,acknowledged_at,reason,version FROM inventory_quality_alerts WHERE id=?').bind(String(dueAlert?.id)).first()).toEqual({acknowledged_by:f.manager.userId,acknowledged_at:acknowledgedAt,reason:acknowledgement.reason,version:2});
 expect((await qualityAlertWork(f.manager)).find(w=>w.id===String(dueAlert?.id))).toMatchObject({queue:'completed',canAct:false});
 const reloaded=filterWork(mergeWorkItems([...(await qualityWork(f.manager)),...(await qualityAlertWork(f.manager))]),'all','quality_alert','');
 expect(reloaded.find(row=>row.id===String(due.id))?.relatedAlerts).toEqual([expect.objectContaining({id:dueAlert?.id,title:expect.stringContaining('Quality hold due soon'),completed:true})]);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id IN (?,?)').bind(Date.now(),f.manager.assignmentId,f.reviewer.assignmentId).run();
 expect((await qualityAlertWork(f.owner)).find(w=>w.id===String(overdueAlert?.id))?.blocker).toContain('No eligible Quality recipient');
 await expect(db.prepare('DELETE FROM inventory_quality_alerts WHERE id=?').bind(String(dueAlert?.id)).run()).rejects.toThrow('QUALITY_ALERT_IMMUTABLE');
});

it('serializes quarantine against a competing reservation and keeps either outcome safe',async()=>{
 const created=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:party,warehouseId:warehouse,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product,quantity:'2',price:'10',taxBps:0}]}),key()),id=String(created.id);
 await transitionOrder(f.owner,id,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,id,{action:'approve',expectedVersion:2},key());const lineId=String((await getOrder(f.owner,id)).lines[0].id);
 const outcomes=await Promise.allSettled([seedNativeQualityHold(db,f.owner,{warehouseId:warehouse,productId:product,expectedStockVersion:1,dueDate:'2026-09-22',reason:'Concurrent quarantine review'},key()),changeOrderReservation(f.owner,id,{expectedVersion:3,action:'reserve',lineId,quantity:'2',expiresAt:Date.now()+3600000,reason:'Concurrent customer dispatch'},key())]);
 expect(outcomes.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 if(outcomes[0].status==='rejected')await hold();
 expect(await db.prepare("SELECT count(*) n FROM inventory_quality_holds WHERE warehouse_id=? AND status<>'released'").bind(warehouse).first('n')).toBe(1);
 await expect(db.prepare('UPDATE commerce_stock SET quantity_milli=0 WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).run()).rejects.toThrow('QUALITY_HOLD');
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse,product).first()).toEqual({quantity_milli:2000,value_minor:1200});
});
