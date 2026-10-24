import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {expenseDb as db,expenseFixture,key} from "./expense-fixture";
import type {AttachmentOwnerTuple,AttachmentOwnerFixture} from "./engineering-finance-people-attachment-owners";
import {saveEmployeeRequest,actOnEmployeeRequest,employeeRequestSchema,requestActionSchema} from '@erpx/server-records/public/employee-requests';
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests-read';
import {saveServiceTeam,serviceTeamSchema} from '@erpx/server-support/public/service-teams';
import {createLeave,submitLeave,approveLeave,newLeaveSchema,leaveDraftSchema} from '@erpx/server-people_payroll/public/leave';
import {saveTask,transitionTask,taskSchema,taskActionSchema} from '@erpx/server-operations/public/staff-tasks';
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
import {postExpense} from '@erpx/server-people_payroll/public/expenses';
import {saveExpenseAccounting} from '@erpx/server-people_payroll/public/expense-workflow';
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet} from "@erpx/server-files/public/attachments/service";
import {env} from "../runtime-env";
import {createExpensePayment,confirmExpensePayment,expensePaymentSchema} from '@erpx/server-people_payroll/public/expense-payments';
import {createParty,partySchema,createInvoice,invoiceSchema,postInvoice} from '@erpx/server-finance/public/finance';
import {settleInvoice,settlementSchema} from '@erpx/server-commerce/public/commerce';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {posCommand} from '@erpx/server-pos/public/pos-core';
import {accountKeys} from "@erpx/server-pos/public/rules/pos";
import {pmsCommand,loadPms} from "../../../packages/server-pms/src/public/pms";
import {pmsAccountKeys,records,record,type Config} from "@erpx/server-pms/public/rules/pms";

const reason="Independent review of declared synthetic engineering evidence.";
type ServiceHospitalityOwner=AttachmentOwnerFixture&{pendingTargetId?:string};
type Fixture=Awaited<ReturnType<typeof expenseFixture>>;
export async function serviceHospitalityAttachmentOwner(tuple:AttachmentOwnerTuple):Promise<ServiceHospitalityOwner>{
 const f=await expenseFixture();
 const modules=["finance","pos","pms","people_payroll","organization","projects"];
 for(const actor of [f.owner,f.tie,f.claimant,f.head,f.reviewer,f.manager,f.payer,f.it])actor.modules=modules;
 await db.prepare("UPDATE assignments SET modules_json=? WHERE company_id=?").bind(JSON.stringify(modules),f.company).run();
 const standard=(targetId:string,table:string,actor:WorkContext=f.owner,version=1):AttachmentOwnerFixture=>({fixture:f,actor,targetId,version,table,versionColumn:"version",bindingType:"record_document"});
 if(tuple.targetType==="expense_claim")return standard(await f.claim(),"finance_expenses",f.claimant);
 if(tuple.targetType==="expense_payment"){
  const expense=await f.claim();await nativeReceipt(f,expense);await f.action(f.claimant,expense,"submit");await f.action(f.head,expense,"approve");const accounting=await getExpense(f.reviewer,expense);await saveExpenseAccounting(f.reviewer,expense,{expectedVersion:Number(accounting.version),categoryAccountId:f.accounts.expense,tax:"7",periodId:f.period,entryDate:"2026-09-02",note:reason},key());await f.action(f.reviewer,expense,"approve");await f.action(f.manager,expense,"approve");let current=await getExpense(f.manager,expense);
  await postExpense(f.manager,expense,{expectedVersion:Number(current.version),periodId:f.period,entryDate:"2026-09-02"},key());current=await getExpense(f.payer,expense);
  const draft=await createExpensePayment(f.payer,expense,Number(current.version),key()),pendingDraft=await createExpensePayment(f.payer,expense,Number(current.version),key());
  return {...standard(String(draft.draftId),"expense_payment_drafts",f.payer),pendingTargetId:String(pendingDraft.draftId),componentRef:"legacy-upload-adapter",bindingType:"expense_payment_confirmation",bindAction:(_set,command)=>confirmExpensePayment(f.payer,String(draft.draftId),expensePaymentSchema.parse({expectedVersion:1,date:"2026-09-05",amount:"107",reference:"SYNTHETIC-PAYMENT-001"}),command)};
 }
 if(tuple.targetType==="invoice_payment"){
  const party=await createParty(f.owner,partySchema.parse({kind:"supplier",code:key(),name:"Synthetic evidence supplier",currency:"THB"}),key());
  const invoice=await createInvoice(f.reviewer,invoiceSchema.parse({kind:"payable",partyId:party.id,invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-05",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable,taxAccountId:f.accounts.tax}),key());
  const draftFlow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,expectedVersion:draftFlow.version,recordVersion:draftFlow.recordVersion,reason,saveDefault:false},key());
  await postInvoice(f.manager,String(invoice.id),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-01"},key());
  const flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"posted",userId:f.payer.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason,saveDefault:false},key());
  return {...standard(String(invoice.id),"finance_invoices",f.payer,2),componentRef:"legacy-upload-adapter",bindingType:"invoice_payment_settlement",bindAction:(set,command)=>settleInvoice(f.payer,String(invoice.id),settlementSchema.parse({expectedVersion:2,date:"2026-09-05",amount:"100",reference:"SYNTHETIC-SUPPLIER-001",attachmentSetIds:[set]}),command)};
 }
 if(tuple.targetType==="staff_task"){
  const task=await saveTask(db,f.claimant,null,taskSchema.parse({title:"Synthetic task intake",assigneeUserId:f.claimant.userId}),key());await transitionTask(db,f.claimant,String(task.id),taskActionSchema.parse({action:"start",expectedVersion:1}),key());
  return {...standard(String(task.id),"staff_tasks",f.claimant,2),componentRef:"task:in_progress",bindingType:"task_action",bindAction:(set,command)=>transitionTask(db,f.claimant,String(task.id),taskActionSchema.parse({action:"complete",expectedVersion:2,note:reason,attachmentSetIds:[set]}),command)};
 }
 if(tuple.targetType==="leave_request"){
  const input=newLeaveSchema.parse({startDate:"2026-09-14",days:2,handoverNote:reason});
  await f.member("synthetic-leave-hr","hr_admin");
  const leave=await createLeave(f.claimant,input,key());await submitLeave("h001",String(leave.id),leaveDraftSchema.parse({...input,expectedVersion:1}),key(),f.claimant);
  return {...standard(String(leave.id),"leave_requests",f.claimant,2),componentRef:`leave-evidence:${tuple.purposeKey}`,bindingType:"leave_approval",bindAction:(_set,command)=>approveLeave("h001",String(leave.id),{expectedVersion:2,note:reason,override:false},command,f.head)};
 }
 if(tuple.targetType==="employee_request"){
  await saveServiceTeam(db,f.owner,null,serviceTeamSchema.parse({name:"Synthetic IT evidence team",members:[{userId:f.it.userId,responsibility:"lead"}],coverage:[{category:"it",scopeKind:"holding",scopeNodeId:f.company}]}),key());
  const request=await saveEmployeeRequest(db,f.claimant,null,employeeRequestSchema.parse({kind:"service",category:"it",title:"Synthetic service request",description:reason,location:"Synthetic office",branchId:f.branch,departmentId:f.department}),key());
  const requestId=String(request.id),action=async(actor:WorkContext,name:string,extra:Record<string,unknown>={})=>{const row=await getEmployeeRequest(db,actor,requestId);return actOnEmployeeRequest(db,actor,requestId,requestActionSchema.parse({action:name,expectedVersion:row.version,note:reason,...extra}),key());};
  if(tuple.purposeKey==="fulfillment"){
   await action(f.claimant,"submit");await action(f.it,"assign",{assigneeUserId:f.it.userId});await action(f.it,"start");
   const row=await getEmployeeRequest(db,f.it,requestId),base=standard(requestId,"employee_requests",f.it,row.version);
   return tuple.componentKind==="record"?base:{...base,componentRef:"request:resolve",bindingType:"request_fulfillment",bindAction:(set,command)=>actOnEmployeeRequest(db,f.it,requestId,requestActionSchema.parse({action:"resolve",expectedVersion:row.version,note:reason,attachmentSetIds:[set]}),command)};
  }
  const base=standard(requestId,"employee_requests",f.claimant);
  return tuple.componentKind==="record"?base:{...base,componentRef:"request:submit",bindingType:"request_submission",bindAction:(set,command)=>actOnEmployeeRequest(db,f.claimant,requestId,requestActionSchema.parse({action:"submit",expectedVersion:1,note:reason,attachmentSetIds:[set]}),command)};
 }
 if(tuple.targetType.startsWith("pos_"))return posOwner(f,tuple);
 if(tuple.targetType.startsWith("pms_"))return pmsOwner(f,tuple);
 throw new Error(`Unsupported service/hospitality owner ${tuple.id}`);
}

async function posOwner(f:Fixture,tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const cashier=await f.member("synthetic-cashier","pos_cashier");cashier.modules=["pos"];await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(cashier.modules),cashier.assignmentId).run();
 const warehouse=key(),product=key(),register=key(),shift=key(),accounts:Record<string,string>={chartId:f.chart};
 await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?)").bind(warehouse,f.company,f.branch,warehouse,"Synthetic POS warehouse").run();
 await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) VALUES(?,?,?,'Synthetic retail product','each',10000,700)").bind(product,f.company,product).run();
 await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,10000,30000,1)").bind(warehouse,product).run();
 for(const [i,name] of accountKeys.entries()){
  const id=key();accounts[name]=id;const category=name==="revenue"?"revenue":["advance","refund","outputTax"].includes(name)?"liability":["cogs","fees","waste","cashDifference"].includes(name)?"expense":"asset";
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES(?,?,?,?,?,?,1,1)").bind(id,f.chart,f.company,`POS-${i}`,name,category).run();
 }
 await db.prepare("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode) VALUES(?,?,?,?,?,'retail')").bind(register,f.company,f.branch,warehouse,"Synthetic till").run();
 await db.prepare("INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor) VALUES(?,?,?,?,?,1,10000)").bind(shift,register,f.company,f.branch,cashier.userId).run();
 const cmd=(actor:WorkContext,op:string,input:unknown,command=key())=>posCommand(db,actor,op,input,command);
 await cmd(f.owner,"settings.save",{enabled:true,currency:"THB",timezone:"Asia/Bangkok",cashierDiscountBps:0,accounts,expectedVersion:0});
 await cmd(f.manager,"accounting.approve",{expectedVersion:1});
 await cmd(f.owner,"catalog.save",{productId:product,barcode:key(),category:"Retail",kind:"stock",station:"Shop",recipe:[],modifiers:[],taxInclusive:false,registerIds:[register],expectedVersion:0});
 if(tuple.targetType==="pos_shift"){
  await cmd(cashier,"shift.close",{id:shift,expectedVersion:1,countedMinor:10025});
  return {fixture:f,actor:f.owner,targetId:shift,version:2,table:"pos_shifts",versionColumn:"version",componentRef:"variance-review",bindingType:"pos_cash_variance_review",bindAction:(set,command)=>cmd(f.owner,"shift.review",{id:shift,expectedVersion:2,reason,attachmentSetIds:[set]},command)};
 }
 const sale=String((await cmd(cashier,"sale.create",{shiftId:shift,serviceMode:"retail",currency:"THB"})).id);
 const version=async()=>Number(await db.prepare("SELECT version FROM pos_sales WHERE id=?").bind(sale).first("version"));
 await cmd(cashier,"sale.add",{id:sale,expectedVersion:await version(),productId:product,quantityMilli:1000,modifierIds:[],discountMinor:0,note:reason});
 if(tuple.targetType==="pos_sale"){
  const v=await version();return {fixture:f,actor:cashier,targetId:sale,version:v,table:"pos_sales",versionColumn:"version",componentRef:"void",bindingType:"pos_void",bindAction:(set,command)=>cmd(cashier,"sale.cancel",{id:sale,expectedVersion:v,reason,attachmentSetIds:[set]},command)};
 }
 await cmd(cashier,"payment.create",{id:sale,expectedVersion:await version(),method:"cash",amountMinor:10700,tenderedMinor:10700,reference:"SYNTHETIC-CASH"});
 await cmd(cashier,"sale.complete",{id:sale,expectedVersion:await version()});
 const line=String(await db.prepare("SELECT id FROM pos_lines WHERE sale_id=?").bind(sale).first("id"));
 const refund=await cmd(cashier,"refund.request",{id:sale,expectedVersion:await version(),reason,payoutShiftId:shift,lines:[{lineId:line,quantityMilli:1000,restock:true}]});
 return {fixture:f,actor:f.owner,targetId:String(refund.id),version:1,table:"pos_refunds",versionColumn:"version",componentRef:"refund-decision",bindingType:"pos_refund_decision",bindAction:(set,command)=>cmd(f.owner,"refund.approve",{id:refund.id,expectedVersion:1,reason,attachmentSetIds:[set]},command)};
}

async function pmsOwner(f:Fixture,tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const property=await pmsCommand(db,f.owner,{operation:"property.create",input:{name:"Synthetic File Sources Hotel",branchId:f.branch,businessDate:"2026-09-01"}},key()),propertyId=property.id;
 const snap=()=>loadPms(db,f.owner,propertyId),cmd=async(op:string,input:unknown={},actor=f.owner,setId?:string,command=key())=>{const s=await snap();return pmsCommand(db,actor,{propertyId,expectedVersion:s.property.version,operation:op,input,attachmentSetIds:setId?[setId]:[]},command);};
 const nativeBind=async(operation:string,input:unknown)=>{const expectedVersion=(await snap()).property.version;return (setId:string,command:string)=>pmsCommand(db,f.owner,{propertyId,expectedVersion,operation,input,attachmentSetIds:[setId]},command);};
 const accounts={} as Config["accounts"];
 for(const [i,name] of pmsAccountKeys.entries()){
  const id=key();accounts[name]=id;const category=["revenue","service"].includes(name)?"revenue":["advance","deposit","tax"].includes(name)?"liability":["consumption","commission","cashDifference"].includes(name)?"expense":"asset";
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(id,f.chart,f.company,`PMS-${i}`,name,category).run();
 }
 await cmd("accounting.save",{chartId:f.chart,accounts,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true});await cmd("accounting.approve",{},f.manager);
 const type=await cmd("room_type.save",{name:"Synthetic room",capacity:2,amenities:"Shower"}),room=await cmd("room.save",{number:"101",typeId:type.id,active:true}),rate=await cmd("rate.save",{name:"Synthetic nightly rate",typeId:type.id,from:"2026-09-01",to:"2026-12-31",price:10000});
 await cmd("property.enable");const guest=await cmd("guest.save",{name:"Synthetic test guest"});
 const base=(id:string,version:number):AttachmentOwnerFixture=>({fixture:f,actor:f.owner,targetId:id,version,table:"pms_records",versionColumn:"version",bindingType:"record_document"});
 if(tuple.targetType==="pms_guest")return base(guest.id,record(await snap(),"guest",guest.id).version);
 if(tuple.targetType==="pms_task"){
  const task=await cmd("task.create",{roomId:room.id,type:"arrival",date:"2026-09-01"});await cmd("task.start",{id:task.id});const current=record(await snap(),"task",task.id);
  return {...base(task.id,current.version),componentRef:"task.complete",bindingType:"pms_task_completion",bindAction:await nativeBind("task.complete",{id:task.id,checked:current.data.checklist})};
 }
 if(tuple.targetType==="pms_lost_property"){
  const lost=await cmd("lost.save",{description:"Synthetic guest umbrella",roomId:room.id,guestId:guest.id,location:"Synthetic locker",foundDate:"2026-09-01",notes:reason}),current=record(await snap(),"lost",lost.id);
  return tuple.componentKind==="record"?base(lost.id,current.version):{...base(lost.id,current.version),componentRef:"lost.release",bindingType:"pms_lost_property_release",bindAction:await nativeBind("lost.release",{id:lost.id,state:"returned",notes:reason})};
 }
 let creditId:string|undefined;
 if(tuple.targetType==="pms_property"){
  const party=await createParty(f.owner,partySchema.parse({kind:"customer",code:key(),name:"Declared manual settlement agent",currency:"THB"}),key());
  creditId=(await cmd("credit.request",{partyId:party.id,limit:100000,terms:30})).id;
  const creditReviewer=await f.member('synthetic-credit-reviewer','pms_manager');creditReviewer.modules=['pms'];
  await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(creditReviewer.modules),creditReviewer.assignmentId).run();
  await cmd("credit.approve",{id:creditId},creditReviewer);
 }
 const booking=await cmd("reservation.create",{guestId:guest.id,source:creditId?"Declared manual provider fixture":"direct",externalRef:creditId?"SYNTHETIC-MANUAL-001":"",rooms:[{typeId:type.id,rateId:rate.id,arrival:"2026-09-01",departure:"2026-09-02",adults:2,quantity:1}]}),state=await snap(),stay=records(state,"stay").find(row=>row.data.reservationId===booking.id)!;
 if(tuple.targetType==="pms_reservation")return base(booking.id,record(state,"reservation",booking.id).version);
 if(tuple.targetType==="pms_stay")return base(stay.id,stay.version);
 const task=await cmd("task.create",{roomId:room.id,type:"arrival",date:"2026-09-01"}),taskRow=record(await snap(),"task",task.id);await cmd("task.complete",{id:task.id,checked:taskRow.data.checklist});
 const inspector=await f.member(`synthetic-inspector-${key()}`,"pms_supervisor");await cmd("task.inspect",{id:task.id},inspector);
 await cmd("stay.assign",{id:stay.id,roomId:room.id});await cmd("stay.check_in",{id:stay.id});await cmd("stay.preview_checkout",{id:stay.id});await cmd("guest_account.credit",{id:stay.data.guestAccountId,creditId});await cmd("stay.check_out",{id:stay.id});await cmd("guest_account.close",{id:stay.data.guestAccountId});const shift=await cmd("shift.open",{name:"Synthetic reconciliation",opening:0}),current=await snap();
 return {...base(propertyId,current.property.version),table:"pms_properties",componentRef:"ota.settle",bindingType:"pms_ota_settlement",bindAction:await nativeBind("ota.settle",{guestAccountId:stay.data.guestAccountId,gross:10700,commission:1200,fees:100,reference:"SYNTHETIC-STATEMENT-001",shiftId:shift.id})};
}

async function nativeReceipt(f:Fixture,expense:string){
 const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64");
 const set=await createAttachmentSet(db,f.claimant,{targetType:"expense_claim",targetId:expense,purposeKey:"receipt",componentKind:"record",expectedTargetVersion:1},key());
 const reserved=await reserveAttachmentFile(db,f.claimant,set.id,{filename:"synthetic-prerequisite-receipt.png",mediaType:"image/png",bytes:png.byteLength},key());
 const uploaded=await completeAttachmentUpload(db,env.H001_FILES,f.claimant,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);await bindRecordAttachmentSet(db,f.claimant,set.id,1,key());
}
