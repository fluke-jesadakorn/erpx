import { transferActionSchema } from "@erpx/server-inventory/public/inventory-transfers";
import { transitReceiveSchema } from "@erpx/server-inventory/public/inventory-transit";
import { transitLossSchema } from "@erpx/server-inventory/public/inventory-transit";
import {beforeAll,beforeEach,afterEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {getDomainWork} from "@erpx/server-work_projection/public/domain-work";
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {saveMaster,commercePage} from "@erpx/server-commerce/public/commerce";
import {saveTransfer,transitionTransfer,transferSchema} from "@erpx/server-inventory/public/inventory-transfers";
import {getTransit,transitDetail,dispatchTransit,receiveTransit,requestTransitLoss,decideTransitLoss,cancelTransit,transitWork} from "@erpx/server-inventory/public/inventory-transit";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,source:string,target:string,product:string;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='warehouse_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'warehouse_manager');f.manager.role='finance_approver';f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,'finance_approver');source=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Transit source'},key())).id);target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Transit destination'},key())).id);product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Transit product',price:'10'},key())).id);await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,1000,1)').bind(source,product).run();});
afterEach(()=>{vi.unstubAllEnvs();});
async function approved(beforePost?:()=>Promise<void>){const t=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:source,targetWarehouseId:target,transferDate:'2026-09-02',deliveryMode:'transit',arrivalDueDate:'2026-09-04',reason:'Replenishment with separate transport custody',lines:[{productId:product,quantity:'3'}]}),key()),id=String(t.id);await transitionTransfer(f.owner,id,'submit',transferActionSchema.parse({expectedVersion:1}),key());const flow=await resolveFlow(db,f.owner,'transfer',id);await assignFlowPrimary(db,f.owner,'transfer',id,{stepKey:'submitted',userId:f.head.userId,expectedVersion:flow.version,recordVersion:2,reason:'Assign independent receiving reviewer for simulation',saveDefault:false},key());if(beforePost)await beforePost();await transitionTransfer(f.head,id,'post',transferActionSchema.parse({expectedVersion:2,reason:'Reviewed source stock and authorized dispatch'}),key());return `transit-${id}`;}
const dispatch=async(id:string,attachmentSetIds:string[]=[] )=>{const command=key(),receipt=await dispatchTransit(f.owner,id,{expectedVersion:1,date:'2026-09-02',trackingReference:'SIM-TRACK-001',evidence:'Carrier collected all three units with sealed packaging',attachmentSetIds},command);return {receipt,command};};
const stock=(w:string)=>db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(w,product).first();
async function rawSupplyInventory(){
 const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),input={companyId:f.company,packageId:'supply',packageVersion:1};
 const result=await withOwnerPackageObligations(db,input,database=>readOwnerPackageObligations(db,database,{...input,now:Date.now()}));
 return {...result.observations.find(row=>row.moduleId==='inventory')!,finance:result.observations.find(row=>row.moduleId==='finance')!};
}

it('rejects transit evidence staged for another dispatch, receipt line, or loss line without changing transit state',async()=>{
 const id=await approved();
 const wrongDispatch=await seedAttachmentSet(db,f.owner,{targetType:'inventory_transit',targetId:id,purposeKey:'dispatch_evidence',componentRef:'another-dispatch',expectedTargetVersion:1});
 await expect(dispatch(id,[wrongDispatch.setId])).rejects.toMatchObject({code:'ATTACHMENT_BINDING_CONTEXT'});
 expect(await attachmentBindingState(db,wrongDispatch.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 expect(await db.prepare('SELECT status,version FROM inventory_transit WHERE id=?').bind(id).first()).toEqual({status:'awaiting_dispatch',version:1});
 const dispatched=await seedAttachmentSet(db,f.owner,{targetType:'inventory_transit',targetId:id,purposeKey:'dispatch_evidence',componentRef:'dispatch',expectedTargetVersion:1});
 await dispatch(id,[dispatched.setId]);
 const lineId=String((await transitDetail(f.head,id)).lines[0].id);
 const wrongReceipt=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'receipt_evidence',componentKind:'event',componentRef:'receive:another-line',expectedTargetVersion:2});
 await expect(receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'Wrong receipt line context must be rejected',attachmentSetIds:[wrongReceipt.setId],lines:[{lineId,quantity:'1'}]}),key())).rejects.toMatchObject({code:'ATTACHMENT_BINDING_CONTEXT'});
 expect(await attachmentBindingState(db,wrongReceipt.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 const wrongLoss=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'loss_evidence',componentKind:'event',componentRef:'loss:another-line',expectedTargetVersion:2});
 await expect(requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'1',evidence:'Wrong loss line context must be rejected',attachmentSetIds:[wrongLoss.setId]}),key())).rejects.toMatchObject({code:'ATTACHMENT_BINDING_CONTEXT'});
 expect(await attachmentBindingState(db,wrongLoss.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 expect(await db.prepare('SELECT status,version FROM inventory_transit WHERE id=?').bind(id).first()).toEqual({status:'in_transit',version:2});
 const loss=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'loss_evidence',componentKind:'event',componentRef:`loss:${lineId}`,expectedTargetVersion:2}),command=key();
 const requested=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'1',evidence:'Carrier loss report bound to this exact line',attachmentSetIds:[loss.setId]}),command);
 expect(await attachmentBindingState(db,loss.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'inventory_transit_loss',binding_id:String(requested.proposalId)});
});
it('preserves stock through approval, dispatch, partial receipts and final rounding with replay and scope guards',async()=>{const id=await approved();expect(await stock(source)).toEqual({quantity_milli:3000,value_minor:1000});expect(await stock(target)).toBeNull();const dispatchFiles=await seedAttachmentSet(db,f.owner,{targetType:'inventory_transit',targetId:id,purposeKey:'dispatch_evidence',componentRef:'dispatch',expectedTargetVersion:1}),dispatched=await dispatch(id,[dispatchFiles.setId]);expect(await attachmentBindingState(db,dispatchFiles.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'inventory_transit_dispatch',binding_id:`transit-dispatch-${dispatched.command}`});expect(await stock(source)).toEqual({quantity_milli:0,value_minor:0});expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_in_transit WHERE transit_id=?').bind(id).first()).toEqual({quantity_milli:3000,value_minor:1000});const lineId=String((await transitDetail(f.head,id)).lines[0].id);await expect(receiveTransit(f.owner,id,transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'Dispatcher cannot confirm own receipt',lines:[{lineId,quantity:'1'}]}),key())).rejects.toMatchObject({code:'INDEPENDENT_RECEIVER_REQUIRED'});const receiptFiles=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'receipt_evidence',componentKind:'event',componentRef:`receive:${lineId}`,expectedTargetVersion:2}),command=key(),input={expectedVersion:2,date:'2026-09-03',evidence:'First carton physically counted at receiving bay',attachmentSetIds:[receiptFiles.setId],lines:[{lineId,quantity:'1'}]};const receipt=await receiveTransit(f.head,id,transitReceiveSchema.parse(input),command);expect(await attachmentBindingState(db,receiptFiles.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'inventory_transit_receipt',binding_id:`transit-receipt-${command}`});expect(await receiveTransit(f.head,id,transitReceiveSchema.parse(input),command)).toEqual(receipt);expect(await stock(target)).toEqual({quantity_milli:1000,value_minor:333});expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_in_transit WHERE transit_id=?').bind(id).first()).toEqual({quantity_milli:2000,value_minor:667});await expect(receiveTransit(f.head,id,transitReceiveSchema.parse({...input,lines:[{lineId,quantity:'1.5'}]}),command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(receiveTransit(f.head,id,transitReceiveSchema.parse({...input,attachmentSetIds:[]}),key())).rejects.toMatchObject({code:'CONFLICT'});await expect(getTransit({...f.owner,branchId:'wrong-branch'},id)).rejects.toMatchObject({status:404});await expect(getTransit({...f.owner,companyId:'wrong-company'},id)).rejects.toMatchObject({status:404});await receiveTransit(f.head,id,transitReceiveSchema.parse({...input,attachmentSetIds:[],expectedVersion:3,lines:[{lineId,quantity:'2'}]}),key());expect((await getTransit(f.owner,id)).status).toBe('completed');expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});expect(await db.prepare('SELECT sum(quantity_milli) quantity,sum(value_minor) value FROM inventory_ledger WHERE source_id=?').bind(id).first()).toEqual({quantity:0,value:0});expect((await transitWork(f.head)).find(w=>w.id===id)?.queue).toBe('completed');});
it('reserves reported loss, rejects it without stock changes and independently posts confirmed loss to GL',async()=>{const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id),first=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'1',evidence:'One carton missing; carrier incident reference SIM-2'}),key());await expect(receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:3,date:'2026-09-03',evidence:'Cannot receive quantity reserved by discrepancy',lines:[{lineId,quantity:'3'}]}),key())).rejects.toMatchObject({code:'TRANSIT_QUANTITY_EXCEEDED'});await decideTransitLoss(f.manager,id,{expectedVersion:3,date:'2026-09-03',proposalId:String(first.proposalId),decision:'reject',evidence:'Carrier found carton; recheck physical receiving count'},key());expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_in_transit WHERE transit_id=?').bind(id).first()).toEqual({quantity_milli:3000,value_minor:1000});await receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:4,date:'2026-09-03',evidence:'Two cartons received; third confirmed lost by carrier',lines:[{lineId,quantity:'2'}]}),key());const loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:5,date:'2026-09-03',lineId,quantity:'1',evidence:'Carrier signed loss report SIM-3 for remaining carton'}),key());await expect(decideTransitLoss(f.owner,id,{expectedVersion:6,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Dispatcher must not approve own loss'},key())).rejects.toMatchObject({code:'INDEPENDENT_FINANCE_REQUIRED'});const result=await decideTransitLoss(f.manager,id,{expectedVersion:6,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance verified carrier loss and cost basis'},key());expect((await getTransit(f.owner,id)).status).toBe('completed');expect(await stock(target)).toEqual({quantity_milli:2000,value_minor:667});expect(await db.prepare('SELECT sum(debit_minor) debit,sum(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(result.journalId).first()).toEqual({debit:333,credit:333});expect(await db.prepare('SELECT count(*) n FROM inventory_in_transit WHERE transit_id=?').bind(id).first('n')).toBe(0);await expect(db.prepare('DELETE FROM inventory_transit_events WHERE transit_id=?').bind(id).run()).rejects.toThrow('TRANSIT_EVENT_IMMUTABLE');});
it('blocks closed periods and quarantine, cancels only before dispatch and prevents competing over-receipts',async()=>{const id=await approved();const {createQualityHold}=await import("@erpx/server-quality/public/quality");await createQualityHold(f.owner,{warehouseId:source,productId:product,expectedStockVersion:1,dueDate:'2026-09-04',reason:'Source packaging defect requires inspection'},key());await expect(dispatch(id)).rejects.toMatchObject({code:'CONFLICT'});expect(await stock(source)).toEqual({quantity_milli:3000,value_minor:1000});await cancelTransit(f.owner,id,{expectedVersion:1,reason:'Cancel unshipped transport pending source quality inspection'},key());expect((await getTransit(f.owner,id)).status).toBe('cancelled');});
it('allows only one competing final receipt and leaves closed-period quantities untouched',async()=>{const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id),input={expectedVersion:2,date:'2026-09-03',evidence:'Final physical count recorded with delivery evidence',lines:[{lineId,quantity:'3'}]};await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();await expect(receiveTransit(f.head,id,transitReceiveSchema.parse(input),key())).rejects.toMatchObject({code:'CONFLICT'});expect(await stock(target)).toBeNull();await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();const results=await Promise.allSettled([receiveTransit(f.head,id,transitReceiveSchema.parse(input),key()),receiveTransit(f.head,id,transitReceiveSchema.parse(input),key())]);expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});await expect(cancelTransit(f.owner,id,{expectedVersion:3,reason:'Cannot erase completed transport evidence'},key())).rejects.toMatchObject({code:'DISPATCHED_TRANSPORT'});});

it('preserves the legacy direct transfer contract without inventing transport history',async()=>{
 const created=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:source,targetWarehouseId:target,transferDate:'2026-09-02',reason:'Completed direct warehouse movement',lines:[{productId:product,quantity:'3'}]}),key()),id=String(created.id);
 await transitionTransfer(f.owner,id,'submit',transferActionSchema.parse({expectedVersion:1}),key());const flow=await resolveFlow(db,f.owner,'transfer',id);await assignFlowPrimary(db,f.owner,'transfer',id,{stepKey:'submitted',userId:f.head.userId,expectedVersion:flow.version,recordVersion:2,reason:'Assign independent receipt reviewer',saveDefault:false},key());await transitionTransfer(f.head,id,'post',transferActionSchema.parse({expectedVersion:2,reason:'All three units physically received'}),key());
 expect(await stock(source)).toEqual({quantity_milli:0,value_minor:0});expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});expect(await db.prepare('SELECT count(*) n FROM inventory_transit WHERE transfer_id=?').bind(id).first('n')).toBe(0);
});
it('exposes source handoffs and valuation without leaking another branch and retains quality hold on incoming goods',async()=>{
 const {createQualityHold}=await import("@erpx/server-quality/public/quality"),{listStaffNotifications}=await import("@erpx/server-collaboration/public/staff-notifications"),{calendarData}=await import("@erpx/server-work_projection/public/calendar/index");
 const id=await approved();expect((await transitWork(f.owner)).find(w=>w.id===id)?.canAct).toBe(true);await dispatch(id);
 expect((await listStaffNotifications(db,f.head)).some(n=>n.record_id===id)).toBe(true);expect((await listStaffNotifications(db,{...f.head,branchId:'other-branch'})).some(n=>n.record_id===id)).toBe(false);
 expect((await calendarData(db,f.owner,{date:'2026-09-04',view:'day',mode:'team',layers:'stock_transit',timezone:'Asia/Bangkok'})).items.some(i=>i.sourceId===id)).toBe(true);
 expect((await commercePage(f.owner,{view:'stock'})).inTransit).toEqual([{currency:'THB',line_count:1,value_minor:1000}]);
 await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)').bind(target,product).run();const v=Number(await db.prepare('SELECT version FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(target,product).first('version'));
 await createQualityHold(f.owner,{warehouseId:target,productId:product,expectedStockVersion:v,dueDate:'2026-09-04',reason:'Incoming transport requires destination inspection'},key());
 const lineId=String((await transitDetail(f.head,id)).lines[0].id);await receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'Three incoming units received in quarantine area',lines:[{lineId,quantity:'3'}]}),key());expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});
 await expect(db.prepare('UPDATE commerce_stock SET quantity_milli=0 WHERE warehouse_id=? AND product_id=?').bind(target,product).run()).rejects.toThrow('QUALITY_HOLD');
});
it('rejects revoked receivers and keeps an unresolved loss on the asset balance while its accounting period is closed',async()=>{
 const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id);
 const loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'3',evidence:'Carrier reports all cartons missing in transport'}),key());
 await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();await expect(decideTransitLoss(f.manager,id,{expectedVersion:3,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Cannot expense loss into a closed accounting period'},key())).rejects.toMatchObject({code:'PERIOD_CLOSED'});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_in_transit WHERE transit_id=?').bind(id).first()).toEqual({quantity_milli:3000,value_minor:1000});
 await decideTransitLoss(f.manager,id,{expectedVersion:3,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'reject',evidence:'Carrier located goods; confirm receiving instead'},key());await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();await expect(receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:4,date:'2026-09-03',evidence:'Attempt by revoked receiving operator',lines:[{lineId,quantity:'3'}]}),key())).rejects.toMatchObject({code:'CONFLICT'});expect(await stock(target)).toBeNull();
});


it('rolls back receiving stock, file binding and transport evidence on late outbox failure, then retries and replays',async()=>{
 const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id);
 const evidence=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'receipt_evidence',componentKind:'event',componentRef:`receive:${lineId}`,expectedTargetVersion:2});
 const command=key(),input={expectedVersion:2,date:'2026-09-03',evidence:'Independent receiver confirms all units with retained receipt files',attachmentSetIds:[evidence.setId],lines:[{lineId,quantity:'3'}]};
 const suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`inventory_receive_fault_${suffix}`,trigger=`inventory_receive_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'INVENTORY_RECEIVE_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(receiveTransit(f.head,id,transitReceiveSchema.parse(input),command)).rejects.toThrow('INVENTORY_RECEIVE_LATE_FAILURE');
  expect(await stock(target)).toBeNull();expect(await db.prepare('SELECT status,version FROM inventory_transit WHERE id=?').bind(id).first()).toEqual({status:'in_transit',version:2});
  expect(await db.prepare('SELECT received_milli,settled_value_minor FROM inventory_transit_lines WHERE id=?').bind(lineId).first()).toEqual({received_milli:0,settled_value_minor:0});
  expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(command).first('count')).toBe(0);
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const receipt=await receiveTransit(f.head,id,transitReceiveSchema.parse(input),command);expect(await receiveTransit(f.head,id,transitReceiveSchema.parse(input),command)).toEqual(receipt);
 expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
 expect(await db.prepare("SELECT count(*)::int count FROM inventory_transit_events WHERE transit_id=? AND kind='receipt'").bind(id).first('count')).toBe(1);
});

it('rolls back an independently approved transit loss and its Finance journal on late outbox failure, then retries and replays',async()=>{
 const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id);
 const loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'3',evidence:'Carrier confirms all cartons missing with independent evidence'}),key()),proposalId=String(loss.proposalId);
 const command=key(),input={expectedVersion:3,date:'2026-09-03',proposalId,decision:'approve' as const,periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance authorizes the reviewed loss and cost basis'};
 const suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`inventory_loss_fault_${suffix}`,trigger=`inventory_loss_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN RAISE EXCEPTION 'INVENTORY_LOSS_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(decideTransitLoss(f.manager,id,input,command)).rejects.toThrow('INVENTORY_LOSS_LATE_FAILURE');
  expect(await db.prepare('SELECT status,version FROM inventory_transit WHERE id=?').bind(id).first()).toEqual({status:'in_transit',version:3});
  expect(await db.prepare('SELECT quantity_milli,value_minor FROM inventory_in_transit WHERE transit_id=?').bind(id).first()).toEqual({quantity_milli:3000,value_minor:1000});
  expect(await db.prepare("SELECT count(*)::int count FROM finance_journals WHERE source_type='inventory_transit_loss' AND source_id=?").bind(proposalId).first('count')).toBe(0);
  expect(await db.prepare('SELECT count(*)::int count FROM inventory_transit_events WHERE proposal_id=?').bind(proposalId).first('count')).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int count FROM ${table} WHERE command_id=?`).bind(command).first('count')).toBe(0);
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const receipt=await decideTransitLoss(f.manager,id,input,command);expect(await decideTransitLoss(f.manager,id,input,command)).toEqual(receipt);
 expect(await db.prepare('SELECT sum(debit_minor) debit,sum(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(receipt.journalId).first()).toEqual({debit:1000,credit:1000});
 expect(await db.prepare('SELECT count(*)::int count FROM inventory_transit_events WHERE proposal_id=?').bind(proposalId).first('count')).toBe(1);
});


async function supplyGrant(state:'active'|'draining'='active',endsAt:number|null=null){
 const previous=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='supply');
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-transfer-provider'}),obligations:async()=>{throw new Error('TRANSFER_FIXTURE_CANNOT_INACTIVATE');}});
 return store.apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'supply',packageVersion:1,state,startsAt:0,endsAt,version:(previous?.version??0)+1,reason:'Isolated Supply transfer owner workflow',evidence:'test:transfer-purchased-boundary'}});
}
it('requires Supply for transfer creation, direct Flow and Work while keeping exact history replay and retained settlement',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 const input=transferSchema.parse({sourceWarehouseId:source,targetWarehouseId:target,transferDate:'2026-09-02',reason:'Transfer purchase and current authority boundary',lines:[{productId:product,quantity:'3'}]}),command=key();
 await expect(saveTransfer(f.owner,null,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await getDomainWork(f.owner,'transfer')).toEqual([]);
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','shadow');const shadow=await saveTransfer(f.owner,null,input,key()),shadowId=String(shadow.id);
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(resolveFlow(db,f.owner,'transfer',shadowId)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await supplyGrant();const created=await saveTransfer(f.owner,null,input,command),id=String(created.id);
 await transitionTransfer(f.owner,id,'submit',transferActionSchema.parse({expectedVersion:1}),key());
 await supplyGrant('draining');expect(await saveTransfer(f.owner,null,input,command)).toEqual(created);
 await expect(saveTransfer(f.owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(transitionTransfer(f.owner,shadowId,'submit',transferActionSchema.parse({expectedVersion:1}),key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 expect(await resolveFlow(db,f.owner,'transfer',shadowId)).toMatchObject({canAct:false,canManage:false});
 expect((await getDomainWork(f.owner,'transfer')).find(row=>row.id===shadowId)).toMatchObject({canAct:false});
 const flow=await resolveFlow(db,f.owner,'transfer',id);await assignFlowPrimary(db,f.owner,'transfer',id,{stepKey:'submitted',userId:f.head.userId,expectedVersion:flow.version,recordVersion:2,reason:'Retain independent warehouse receipt review',saveDefault:false},key());
 await transitionTransfer(f.head,id,'post',transferActionSchema.parse({expectedVersion:2,reason:'Complete the retained direct movement'}),key());
 expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();
 await expect(saveTransfer(f.owner,null,input,command)).rejects.toMatchObject({code:'TRANSFER_ACCESS_CHANGED'});
});
it('settles retained transit approval while draining but prevents first physical dispatch and permits unshipped cancellation',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await supplyGrant();
 const id=await approved(()=>supplyGrant('draining').then(()=>undefined));
 expect((await getTransit(f.owner,id)).status).toBe('awaiting_dispatch');
 await expect(dispatch(id)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 expect(await stock(source)).toEqual({quantity_milli:3000,value_minor:1000});
 expect((await transitWork(f.owner)).find(item=>item.id===id)).toMatchObject({canAct:false});
 const input={expectedVersion:1,reason:'Cancel the retained unshipped transport during draining'},command=key(),result=await cancelTransit(f.owner,id,input,command);
 expect(await cancelTransit(f.owner,id,input,command)).toEqual(result);expect((await getTransit(f.owner,id)).status).toBe('cancelled');
});
it('completes partial receipt and independently valued loss using only Supply under both enforced gates and draining',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await supplyGrant();
 const id=await approved(),dispatched=await dispatch(id),lineId=String((await transitDetail(f.head,id)).lines[0].id);
 await supplyGrant('draining');
 expect(await dispatchTransit(f.owner,id,{expectedVersion:1,date:'2026-09-02',trackingReference:'SIM-TRACK-001',evidence:'Carrier collected all three units with sealed packaging',attachmentSetIds:[]},dispatched.command)).toEqual(dispatched.receipt);
 const input=transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'First carton received independently during retained settlement',lines:[{lineId,quantity:'1'}]}),command=key(),receipt=await receiveTransit(f.head,id,input,command);
 expect(await stock(target)).toEqual({quantity_milli:1000,value_minor:333});
 const loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:3,date:'2026-09-03',lineId,quantity:'1',evidence:'Carrier signed a retained one carton loss report'}),key());
 await expect(decideTransitLoss(f.owner,id,{expectedVersion:4,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Dispatcher must not approve the retained loss'},key())).rejects.toMatchObject({code:'INDEPENDENT_FINANCE_REQUIRED'});
 const result=await decideTransitLoss(f.manager,id,{expectedVersion:4,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance validates original Supply valuation and loss'},key());
 await receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:5,date:'2026-09-03',evidence:'Final remaining carton independently received',lines:[{lineId,quantity:'1'}]}),key());
 expect(await receiveTransit(f.head,id,input,command)).toEqual(receipt);
 expect((await getTransit(f.owner,id)).status).toBe('completed');expect(await stock(target)).toEqual({quantity_milli:2000,value_minor:666});
 expect(await db.prepare('SELECT sum(debit_minor) debit,sum(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(result.journalId).first()).toEqual({debit:334,credit:334});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>({packageId:row.packageId,state:row.state}))).toEqual([{packageId:'supply',state:'draining'}]);
});
it('rolls back first dispatch on late purchase expiry and receiving on late assignment revocation, then reuses original commands',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await supplyGrant();const id=await approved();
 const dispatchEvidence=await seedAttachmentSet(db,f.owner,{targetType:'inventory_transit',targetId:id,purposeKey:'dispatch_evidence',componentRef:'dispatch',expectedTargetVersion:1});
 const dispatchInput={expectedVersion:1,date:'2026-09-02',trackingReference:'LATE-EXPIRY',evidence:'Physical dispatch retains atomic active purchase',attachmentSetIds:[dispatchEvidence.setId]},dispatchKey=key(),name=`transfer_guard_${key().replaceAll('-','')}`;
 await supplyGrant('active',Date.now()+3000);
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3.15); RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN (NEW.command_id='${dispatchKey}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(dispatchTransit(f.owner,id,dispatchInput,dispatchKey)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await stock(source)).toEqual({quantity_milli:3000,value_minor:1000});expect(await db.prepare('SELECT status,version FROM inventory_transit WHERE id=?').bind(id).first()).toEqual({status:'awaiting_dispatch',version:1});
 expect(await attachmentBindingState(db,dispatchEvidence.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(dispatchKey).first()).toBeNull();
 await supplyGrant();const dispatched=await dispatchTransit(f.owner,id,dispatchInput,dispatchKey);expect(await dispatchTransit(f.owner,id,dispatchInput,dispatchKey)).toEqual(dispatched);
 const lineId=String((await transitDetail(f.head,id)).lines[0].id),evidence=await seedAttachmentSet(db,f.head,{targetType:'inventory_transit',targetId:id,purposeKey:'receipt_evidence',componentKind:'event',componentRef:`receive:${lineId}`,expectedTargetVersion:2});
 const input=transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'Independent physical receipt retains current authority',attachmentSetIds:[evidence.setId],lines:[{lineId,quantity:'3'}]}),command=key();
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.head.assignmentId}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(receiveTransit(f.head,id,input,command)).rejects.toMatchObject({status:409,code:'CONFLICT'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.head.assignmentId).first('revoked_at')).toBeNull();expect(await stock(target)).toBeNull();
 expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 const result=await receiveTransit(f.head,id,input,command);expect(await receiveTransit(f.head,id,input,command)).toEqual(result);expect(await stock(target)).toEqual({quantity_milli:3000,value_minor:1000});
});

it('filters actual Transfer and Transit projections before limits while retaining shared Stock and durable notifications',async()=>{
 const id=await approved(),transferId=String((await getTransit(f.owner,id)).transfer_id),shared=key(),now=Date.now();
 for(const [recordId,title,at] of [[shared,'Shared Stock reference',now],[transferId,'Private approved warehouse transfer',now+1],[id,'Private warehouse transit',now+2]] as const)await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('inventory',?,'transfers',?,'','draft','{}',?,?,?,?,?)").bind(recordId,title,f.company,f.owner.userId,f.owner.userId,at,at).run();
 for(let index=0;index<103;index++)await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'inventory_transit',?,?,?,?)").bind(key(),f.company,f.owner.userId,id,index+10,'Retained private warehouse transit',now+index+3).run();
 const {transferSourceReadScope}=await import('@erpx/server-inventory/public/transfer-record-purchases'),{listStaffNotifications}=await import('@erpx/server-collaboration/public/staff-notifications'),{publishInboxEvent,inboxPage}=await import('@erpx/server-collaboration/public/inbox');
 await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'inventory_transit',sourceId:id,sourceVersion:1000,category:'work',preview:'Retained private warehouse transit',recipientIds:[f.owner.userId]});
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 const {calendarData}=await import('@erpx/server-work_projection/public/calendar/index');
 const calendar=()=>calendarData(db,f.owner,{date:'2026-09-04',view:'day',mode:'team',layers:'stock_transit',timezone:'Asia/Bangkok'});
 const query=async()=>{const scope=await transferSourceReadScope(db,f.owner,'records');return (await db.prepare(`SELECT id FROM workspace_records WHERE company_id=? AND ${scope.where} ORDER BY updated_at DESC LIMIT 1`).bind(f.company,...scope.args).all<{id:string}>()).results;};
 expect(await query()).toEqual([{id:shared}]);expect((await calendar()).items).toEqual([]);expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:'all'})).items).toEqual([]);
 await supplyGrant();await supplyGrant('draining');expect(await query()).toEqual([{id}]);expect((await calendar()).items.some(item=>item.sourceId===id)).toBe(true);expect((await listStaffNotifications(db,f.owner)).length).toBeGreaterThan(0);expect((await inboxPage(db,f.owner,{view:'all'})).items.length).toBeGreaterThan(0);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();expect(await query()).toEqual([{id:shared}]);expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:'all'})).items).toEqual([]);
 expect(await db.prepare('SELECT count(*)::int n FROM staff_notifications WHERE company_id=? AND message=?').bind(f.company,'Retained private warehouse transit').first('n')).toBe(103);
});
it('requires actual Transfer purchase for Files and rolls back a file set when its retained source changes late',async()=>{
 const id=await approved(),{createAttachmentSet,listAttachmentSets}=await import('@erpx/server-files/public/attachments/service'),input={targetType:'inventory_transit',targetId:id,componentKind:'action' as const,componentRef:'dispatch',purposeKey:'dispatch_evidence',expectedTargetVersion:1};
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(createAttachmentSet(db,f.owner,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await supplyGrant();const name=`transfer_file_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE inventory_transit SET due_date='2026-10-20' WHERE id='${id}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON attachment_sets FOR EACH ROW WHEN (NEW.target_id='${id}') EXECUTE FUNCTION ${name}()`).run();
 const command=key();try{await expect(createAttachmentSet(db,f.owner,input,command)).rejects.toMatchObject({code:'TRANSFER_ATTACHMENT_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON attachment_sets`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT 1 FROM attachment_sets WHERE target_id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT due_date FROM inventory_transit WHERE id=?').bind(id).first('due_date')).toBe('2026-09-04');
 const created=await createAttachmentSet(db,f.owner,input,command);expect(await createAttachmentSet(db,f.owner,input,command)).toMatchObject({id:created.id});await supplyGrant('draining');expect(await listAttachmentSets(db,f.owner,{targetType:'inventory_transit',targetId:id})).toHaveLength(1);
 await expect(createAttachmentSet(db,f.owner,{...input,componentRef:'another-dispatch'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
});

it('opens only actual posted transit-loss Finance history and its real secondary projection with the Supply foundation',async()=>{
 const {bookRuleSchema,prepareBookRule,decideBookRule}=await import('@erpx/server-finance/public/finance-books'),{createJournal,createJournalSchema,postJournal,reverseJournal}=await import('@erpx/server-finance/public/finance'),{journalDetail,ledgerWorkspace}=await import('@erpx/server-finance/public/ledger'),{financeDocumentPresentationAccess,financeDocumentFlow}=await import('@erpx/server-finance/public/record-presentation'),{financeDocumentPostAccess,financeProductDocumentEntry}=await import('@erpx/server-finance/public/document-purchases');
 // Declared isolated same-currency book fixture; original review/owner posting
 // builds its real primary→secondary relation before enforcement is exercised.
 const secondary=key();await db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,?,?)").bind(secondary,f.company,'Retained loss management book',f.owner.userId,Date.now(),Date.now()).run();
 const accounts=(await db.prepare('SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY code').bind(f.chart).all<{id:string}>()).results,mappings:{primaryAccountId:string;secondaryAccountId:string}[]=[];
 for(const account of accounts){const id=key();await db.prepare('INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?').bind(id,secondary,account.id).run();mappings.push({primaryAccountId:account.id,secondaryAccountId:id});}
 const rule=await prepareBookRule(f.owner,bookRuleSchema.parse({primaryChartId:f.chart,secondaryChartId:secondary,effectiveDate:'2026-09-02',openingPeriodId:f.period,reason:'Independently retained same-currency loss book',mappings}),key());await decideBookRule(f.manager,String(rule.id),{expectedVersion:1,reason:'Independent opening and complete account mappings reviewed'},'approve',key());
 const id=await approved();await dispatch(id);const lineId=String((await transitDetail(f.head,id)).lines[0].id),loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:2,date:'2026-09-03',lineId,quantity:'3',evidence:'Carrier confirms all three original cartons are missing'}),key()),proposalId=String(loss.proposalId);
 const forged=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-03',currency:'THB',sourceType:'inventory_transit_loss',sourceId:proposalId,memo:'Declared adversarial label without owner event link',lines:[{accountId:f.accounts.expense,debit:'10',credit:'0'},{accountId:f.accounts.cash,debit:'0',credit:'10'}]}),key());await postJournal(f.manager,String(forged.id),key(),{expectedVersion:1});
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');await supplyGrant();
 const approvedLoss=await decideTransitLoss(f.manager,id,{expectedVersion:3,date:'2026-09-03',proposalId,decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Independent Finance validates actual retained carrier loss'},key()),journalId=String(approvedLoss.journalId),mirror=await db.prepare('SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=?').bind(journalId).first<{secondary_journal_id:string}>();expect(mirror).not.toBeNull();
 await supplyGrant('draining');
 const assignmentVersion=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first('version')),entryScope={companyId:f.company,branchId:f.owner.branchId,departmentId:f.owner.departmentId,userId:f.owner.userId,assignmentId:f.owner.assignmentId,assignmentVersion,role:f.owner.role};
 for(const actual of [journalId,mirror!.secondary_journal_id]){
  expect((await journalDetail(f.owner,actual,db)).journal).toMatchObject({id:actual,status:'posted'});
  expect(await financeProductDocumentEntry(db,entryScope,{kind:'journal',id:actual})).toEqual({capability:'finance.inventory-valuation',purchasePurpose:'history'});
  expect(await financeDocumentPresentationAccess(f.owner,'journal',actual)).toEqual({sourceOnly:true,historyOnly:true});expect(await financeDocumentPostAccess(f.owner,'journal',actual)).toEqual({canPost:false});expect(await financeDocumentFlow(f.owner,'journal',actual)).toMatchObject({canAct:false,canManage:false});
 }
 await expect(journalDetail(f.owner,String(forged.id),db)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await financeProductDocumentEntry(db,entryScope,{kind:'journal',id:String(forged.id)})).toBeNull();
 await expect(ledgerWorkspace(f.owner,f.chart)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(postJournal(f.owner,journalId,key(),{expectedVersion:1})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(reverseJournal(f.owner,journalId,key(),{periodId:f.period,entryDate:'2026-09-04'})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const {bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),journalVersion=Number(await db.prepare('SELECT version FROM finance_journals WHERE id=?').bind(journalId).first('version')),files=await seedAttachmentSet(db,f.owner,{targetType:'finance_journal',targetId:journalId,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:journalVersion});
 await bindRecordAttachmentSet(db,f.owner,files.setId,journalVersion,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Actual transit loss retained under Legal hold',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(files.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,f.owner,files.linkId,'original')).object.text()).toBe('Actual transit loss retained under Legal hold');
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(['supply']);
});

it('retains raw Supply transport duty through partial receipt and independently reviewed loss until original quantity and captured value agree',async()=>{
 expect((await rawSupplyInventory()).outstanding).toBe(0);
 const id=await approved();expect((await rawSupplyInventory()).outstanding).toBe(1);
 await dispatch(id);expect((await rawSupplyInventory()).outstanding).toBe(1);
 const lineId=String((await transitDetail(f.head,id)).lines[0].id);
 await receiveTransit(f.head,id,transitReceiveSchema.parse({expectedVersion:2,date:'2026-09-03',evidence:'Actual independent receiving observes two original cartons',lines:[{lineId,quantity:'2'}]}),key());
 expect((await rawSupplyInventory()).outstanding).toBe(1);
 const loss=await requestTransitLoss(f.head,id,transitLossSchema.parse({expectedVersion:3,date:'2026-09-03',lineId,quantity:'1',evidence:'Actual carrier evidence confirms one original missing carton'}),key());
 expect((await rawSupplyInventory()).outstanding).toBe(1);
 await decideTransitLoss(f.manager,id,{expectedVersion:4,date:'2026-09-03',proposalId:String(loss.proposalId),decision:'approve',periodId:f.period,expenseAccountId:f.accounts.expense,evidence:'Actual independent Finance approves original captured lost value'},key());
 const settled=await rawSupplyInventory();expect(settled.outstanding).toBe(0);expect(settled.finance.outstanding).toBe(0);
 expect(await stock(source)).toEqual({quantity_milli:0,value_minor:0});expect(await stock(target)).toEqual({quantity_milli:2000,value_minor:667});
 expect((await transitDetail(f.owner,id)).transit).toMatchObject({status:'completed'});
});
it('discharges raw Supply unshipped transit only through original cancellation evidence and keeps an unsupported transfer terminal label pending',async()=>{
 const id=await approved();expect((await rawSupplyInventory()).outstanding).toBe(1);
 await cancelTransit(f.owner,id,{expectedVersion:1,reason:'Original unshipped transfer is no longer required'},key());
 expect((await rawSupplyInventory()).outstanding).toBe(0);expect(await stock(source)).toEqual({quantity_milli:3000,value_minor:1000});
 const draft=await saveTransfer(f.owner,null,transferSchema.parse({sourceWarehouseId:source,targetWarehouseId:target,transferDate:'2026-09-02',reason:'Original draft imported without final decision proof',lines:[{productId:product,quantity:'1'}]}),key());
 await db.prepare("UPDATE inventory_transfers SET status='cancelled',version=version+1 WHERE id=?").bind(String(draft.id)).run();
 expect((await rawSupplyInventory()).outstanding).toBe(1);
});

it.each(['stock_transfer','inventory_transfer','inventory_transit'] as const)('fresh %s private transport bytes keep the exact original dispatch source and warehouse custody policy during draining',async(targetType)=>{
 const id=await approved(),transferId=String(await db.prepare('SELECT transfer_id FROM inventory_transit WHERE id=?').bind(id).first('transfer_id')),targetId=targetType==='inventory_transit'?id:transferId,version=targetType==='inventory_transit'?1:3,file=await seedAttachmentSet(db,f.owner,{targetType,targetId,purposeKey:'dispatch_evidence',componentRef:'dispatch',expectedTargetVersion:version});await dispatchTransit(f.owner,id,{expectedVersion:1,expectedTransferVersion:3,date:'2026-09-02',trackingReference:'PRIVATE-TRACK-001',evidence:'Actual native dispatch with retained original source bytes',attachmentSetIds:[file.setId]},key());const {attachmentContent}=await import('@erpx/server-files/public/attachments/service'),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes=`Original ${targetType} carrier private evidence`;await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();await supplyGrant();await supplyGrant('draining');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');
 let consumed=0;const store:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};expect(await(await attachmentContent(db,store,f.owner,file.linkId,'original')).object.text()).toBe(bytes);await expect(db.transaction(tx=>attachmentContent(tx,store,f.owner,file.linkId,'original'))).rejects.toMatchObject({code:'INVENTORY_FILE_READ_TRANSACTION_REQUIRED'});
 for(const fault of ['transfer','transit','warehouse','iam'] as const){const trigger=`transit_byte_${key().replaceAll('-','')}`,effect=fault==='transfer'?`UPDATE inventory_transfers SET reason=reason||' unexpected source edit' WHERE id='${transferId}'`:fault==='transit'?`UPDATE inventory_transit SET due_date='2026-12-31' WHERE id='${id}'`:fault==='warehouse'?`UPDATE commerce_warehouses SET name=name||' unexpected source edit' WHERE id='${target}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,store,f.owner,file.linkId,'original')).rejects.toMatchObject({code:fault==='iam'?'ATTACHMENT_FILE_READ_CHANGED':'INVENTORY_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare('SELECT reason FROM inventory_transfers WHERE id=?').bind(transferId).first('reason')).toBe('Replenishment with separate transport custody');expect(await db.prepare('SELECT due_date FROM inventory_transit WHERE id=?').bind(id).first('due_date')).toBe('2026-09-04');expect(await db.prepare('SELECT name FROM commerce_warehouses WHERE id=?').bind(target).first('name')).toBe('Transit destination');expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});}
 expect(consumed).toBe(5);expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(1);expect(await(await attachmentContent(db,store,f.owner,file.linkId,'original')).object.text()).toBe(bytes);await expect(attachmentContent(db,store,{...f.owner,branchId:'unrelated-receiving-branch'},file.linkId,'original')).rejects.toMatchObject({status:403,code:'ATTACHMENT_SCOPE_FORBIDDEN'});expect(consumed).toBe(6);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['supply']);
});
