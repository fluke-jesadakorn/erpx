import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { pmsImportSchema,type PmsImportDefinition,type PmsImportReport } from "@/lib/pms-import";
import { executePms,PmsError } from "@/lib/pms-engine";
import { record,records,type Kind,type Snapshot } from "@/lib/pms";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "../authorization";
import type { SqlDatabase,TransactionalSqlDatabase } from "../runtime/types";
import { executeCommand,replayCommand } from "../commands";
import { ApiError } from "../http";
import { requirePermission } from "../iam-permissions";
import { loadPms,persistPmsRecords } from "../pms";

type Batch={id:string;property_id:string;cutover_date:string;property_version:number;definition_json:string;report_json:string;
  state:"preview"|"approved"|"applied";created_by:string;approved_by:string|null;opening_journal_id:string|null;version:number;created_at:number};
const id=(...parts:string[])=>"pms-import-"+createHash("sha256").update(JSON.stringify(parts)).digest("hex").slice(0,40);
const failed=(code:string,message:string)=>new ApiError(409,code,message);
function simulate(initial:Snapshot,definition:PmsImportDefinition,batchId:string,userId:string,now:number){
  let snapshot=structuredClone(initial);
  const report:PmsImportReport={errors:[],counts:{},totals:{due:0,advance:0,advanceTax:0,deposit:0},canApprove:false};
  const maps=new Map<string,Map<string,string>>();
  const links:{sourceKey:string;folioId:string;reservationId:string;openingInvoiceId?:string}[]=[];
  for(const category of ["roomTypes","rooms","rates","guests","reservations"] as const){
    report.counts[category]=definition[category].length;maps.set(category,new Map());
  }
  const resolve=(category:string,key:string)=>{const value=maps.get(category)?.get(key);if(!value)throw failed("PMS_IMPORT_REFERENCE",`Unknown ${category} key: ${key}`);return value;};
  function command(operation:string,input:unknown,category:string,key:string){
    let ordinal=0;
    const result=executePms(snapshot,operation,input,{userId,now,independent:()=>true,uuid:()=>id(batchId,category,key,String(ordinal++))});
    if(result.effects.postings.length||result.effects.documents.length)throw failed("PMS_IMPORT_POSTING","Import must not create historical receipts or revenue.");
    return result.id;
  }
  function row(category:string,key:string,run:()=>string){
    const prior=structuredClone(snapshot);
    try{if(maps.get(category)!.has(key))throw failed("PMS_IMPORT_DUPLICATE",`Duplicate source key: ${key}`);maps.get(category)!.set(key,run());}
    catch(error){snapshot=prior;if(!(error instanceof PmsError||error instanceof ApiError))throw error;report.errors.push({row:`${category}.${key}`,code:error.code,message:error.message});}
  }
  function existing(kind:Kind,value:string|undefined,create:()=>string){if(value){record(snapshot,kind,value);return value;}return create();}
  for(const value of definition.roomTypes)row("roomTypes",value.key,()=>existing("room_type",value.existingId,()=>command("room_type.save",value,"roomTypes",value.key)));
  for(const value of definition.rooms)row("rooms",value.key,()=>existing("room",value.existingId,()=>command("room.save",{...value,typeId:resolve("roomTypes",value.typeKey)},"rooms",value.key)));
  for(const value of definition.rates)row("rates",value.key,()=>existing("rate",value.existingId,()=>command("rate.save",{...value,typeId:resolve("roomTypes",value.typeKey),price:value.priceMinor},"rates",value.key)));
  for(const value of definition.guests)row("guests",value.key,()=>existing("guest",value.existingId,()=>command("guest.save",value,"guests",value.key)));
  for(const value of definition.reservations){
    for(const pocket of ["due","advance","advanceTax","deposit"] as const)report.totals[pocket]+=value.opening[pocket];
    row("reservations",value.key,()=>{
      if(value.departure<=definition.cutoverDate||value.departure<=value.arrival)throw failed("PMS_IMPORT_DATES","Import only open stays departing after the cutover date.");
      if(value.state==="in_house"&&(!value.roomKey||value.arrival>definition.cutoverDate))throw failed("PMS_IMPORT_ROOM","An in-house stay needs an assigned room and an arrival on or before cutover.");
      if(value.opening.advanceTax>value.opening.advance)throw failed("PMS_IMPORT_TAX","Prepaid tax cannot exceed the advance balance.");
      if(value.opening.due&&!value.openingInvoiceId)throw failed("PMS_IMPORT_RECEIVABLE","Opening receivables need the original Finance opening invoice ID.");
      const reservationId=command("reservation.create",{guestId:resolve("guests",value.guestKey),source:definition.source,externalRef:value.key,
        rooms:[{typeId:resolve("roomTypes",value.typeKey),rateId:resolve("rates",value.rateKey),arrival:value.arrival<definition.cutoverDate?definition.cutoverDate:value.arrival,departure:value.departure,adults:value.adults,quantity:1}]},"reservations",value.key);
      const stay=records(snapshot,"stay").find(item=>item.data.reservationId===reservationId)!;
      if(value.roomKey)command("stay.assign",{id:stay.id,roomId:resolve("rooms",value.roomKey)},"assign",value.key);
      // Prices begin at cutover. Earlier room charges belong to the approved opening ledger.
      stay.data.arrival=value.arrival;
      if(value.state==="in_house"){stay.data.state="in_house";stay.data.checkedInAt=now;for(const segment of stay.data.segments)segment.from=value.arrival;}
      const reservation=record(snapshot,"reservation",reservationId);
      reservation.data.history=[{at:now,actor:userId,action:`Imported at cutover ${definition.cutoverDate}; historical charges remain in opening journal`}];
      const folio=record(snapshot,"folio",stay.data.folioId);
      const importedGuest=definition.guests.find(guest=>guest.key===value.guestKey)!;
      if(importedGuest.partyId)folio.data.partyId=importedGuest.partyId;
      Object.assign(folio.data,value.opening);
      for(const pocket of ["due","advance","deposit"] as const){
        const total=value.opening[pocket];if(!total)continue;
        snapshot.records.push({id:id(batchId,value.key,pocket),kind:"entry",version:1,updatedAt:now,data:{folioId:folio.id,stayId:stay.id,date:definition.cutoverDate,
          kind:pocket==="due"?"charge":pocket==="deposit"?"deposit":"payment",description:`Opening ${pocket} · ${definition.source} ${value.key}`,
          total,net:0,service:0,tax:0,method:"opening",reference:batchId,sourceKey:`opening:${batchId}:${value.key}:${pocket}`,originalId:"",shiftId:"",actorId:userId,at:now}});
      }
      links.push({sourceKey:value.key,folioId:folio.id,reservationId,openingInvoiceId:value.openingInvoiceId});return reservationId;
    });
  }
  report.canApprove=report.errors.length===0;
  return {snapshot,report,links};
}
function checkCutover(snapshot:Snapshot,definition:PmsImportDefinition){
  if(snapshot.property.version!==definition.expectedVersion)throw failed("PMS_IMPORT_STALE","Hotel records changed. Preview a fresh batch.");
  if(snapshot.property.businessDate!==definition.cutoverDate||snapshot.property.state!=="open")throw failed("PMS_IMPORT_CUTOVER","Use the hotel's open business date as the cutover date.");
  if(snapshot.property.config.currency!=="THB"||!snapshot.property.config.approvedBy)throw failed("PMS_IMPORT_ACCOUNTING","Finance must approve the hotel's THB accounts before importing.");
}
export async function pmsImportBatches(db:SqlDatabase,actor:WorkContext,propertyId:string){
  requirePermission(actor,"pms.read");await loadPms(db,actor,propertyId);
  requirePermission(actor,"pms.reports");
  return (await db.prepare(`SELECT id,cutover_date,state,version,created_by,approved_by,opening_journal_id,report_json,created_at FROM pms_import_batches WHERE company_id=? AND property_id=? ORDER BY created_at DESC,id LIMIT 30`)
    .bind(actor.companyId,propertyId).all<Batch>()).results.map(batch=>({...batch,report:JSON.parse(batch.report_json) as PmsImportReport,report_json:undefined}));
}
export async function previewPmsImport(db:TransactionalSqlDatabase,actor:WorkContext,raw:unknown,key:string){
  requirePermission(actor,"pms.configure");requirePermission(actor,"pms.reservations");
  const definition=pmsImportSchema.parse(raw),batchId=id(actor.companyId,key);
  return db.transaction(async tx=>{
    const old=await replayCommand(tx,actor,key,"pms.import.preview",batchId,definition);if(old)return old;
    const snapshot=await loadPms(tx,actor,definition.propertyId,true);checkCutover(snapshot,definition);
    const now=Date.now(),{report}=simulate(snapshot,definition,batchId,actor.userId,now),result={id:batchId,version:1,state:"preview",report};
    await executeCommand(tx,actor,key,"pms.import.preview",definition,batchId,1,result,[tx.prepare(`INSERT INTO pms_import_batches(id,company_id,property_id,cutover_date,property_version,definition_json,report_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)`)
      .bind(batchId,actor.companyId,definition.propertyId,definition.cutoverDate,definition.expectedVersion,JSON.stringify(definition),JSON.stringify(report),actor.userId,now,now)],{companyId:actor.companyId,branchId:snapshot.property.branchId,departmentId:null});
    return result;
  });
}
async function readBatch(db:SqlDatabase,actor:WorkContext,batchId:string){
  const batch=await db.prepare("SELECT * FROM pms_import_batches WHERE id=? AND company_id=? FOR UPDATE").bind(batchId,actor.companyId).first<Batch>();
  if(!batch)throw new ApiError(404,"PMS_IMPORT_NOT_FOUND","Import batch not found.");
  const snapshot=await loadPms(db,actor,batch.property_id,true);
  return {batch,snapshot,definition:pmsImportSchema.parse(JSON.parse(batch.definition_json))};
}
async function verifyOpening(db:SqlDatabase,actor:WorkContext,snapshot:Snapshot,report:PmsImportReport,journalId:string|null,definition:PmsImportDefinition,batchId:string){
  const total=report.totals;
  for(const guest of definition.guests.filter(guest=>guest.partyId)){
    if(!await db.prepare("SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND currency='THB' AND status='active' FOR SHARE").bind(guest.partyId,actor.companyId).first())throw failed("PMS_IMPORT_CUSTOMER","Use an active THB Finance customer in this company.");
  }
  if(![total.due,total.advance,total.deposit,total.advanceTax].some(Boolean)){if(journalId)throw failed("PMS_IMPORT_JOURNAL","This batch has no opening balances.");return;}
  if(!journalId)throw failed("PMS_IMPORT_JOURNAL","Select an independently posted opening journal for the balances.");
  const journal=await db.prepare(`SELECT id FROM finance_journals WHERE id=? AND company_id=? AND chart_id=? AND status='posted' AND is_opening=1 AND entry_date=? AND posted_by IS NOT NULL AND NOT iam_same_person(created_by,posted_by,company_id) FOR SHARE`)
    .bind(journalId,actor.companyId,snapshot.property.config.chartId,snapshot.property.businessDate).first();
  if(!journal)throw failed("PMS_IMPORT_JOURNAL","The journal must be posted, independently approved, in this chart and on the cutover date.");
  const batchReferences=records(snapshot,"entry").filter(entry=>entry.data.reference===batchId&&entry.data.kind==="charge");
  const invoiceIds=new Set<string>();
  for(const row of definition.reservations.filter(row=>row.opening.due>0)){
    if(!row.openingInvoiceId||invoiceIds.has(row.openingInvoiceId))throw failed("PMS_IMPORT_RECEIVABLE","Each opening receivable must have a separate original Finance invoice.");
    invoiceIds.add(row.openingInvoiceId);
    const entry=batchReferences.find(entry=>entry.id===id(batchId,row.key,"due"))!,folio=record(snapshot,"folio",entry.data.folioId);
    const invoice=await db.prepare(`SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND journal_id=? AND source_type='opening_import'
      AND status='posted' AND outstanding_minor=? AND total_minor=? AND currency='THB' AND debit_account_id=? AND party_id=? FOR SHARE`)
      .bind(row.openingInvoiceId,actor.companyId,journalId,row.opening.due,row.opening.due,snapshot.property.config.accounts.receivable,folio.data.partyId).first();
    if(!invoice)throw failed("PMS_IMPORT_RECEIVABLE","The opening invoice must match the Finance journal, customer, account and guest balance exactly.");
  }
  const schedules=await db.prepare("SELECT count(*) count FROM finance_invoices WHERE company_id=? AND journal_id=? AND kind='receivable' AND outstanding_minor>0").bind(actor.companyId,journalId).first<number>("count");
  if(Number(schedules)!==invoiceIds.size)throw failed("PMS_IMPORT_RECEIVABLE","Include every receivable schedule from this hotel opening journal exactly once.");
  const accounts=snapshot.property.config.accounts;
  for(const [account,amount] of [[accounts.receivable,total.due],[accounts.advance,-total.advance],[accounts.deposit,-total.deposit],[accounts.prepaidTax,total.advanceTax],[accounts.tax,-total.advanceTax]] as const){
    const value=await db.prepare("SELECT COALESCE(sum(debit_minor-credit_minor),0) value FROM finance_journal_lines WHERE journal_id=? AND account_id=?").bind(journalId,account).first<number>("value");
    if(Number(value)!==amount)throw failed("PMS_IMPORT_GL_DIFFERENCE","Guest opening totals do not match the posted opening control accounts.");
  }
}
export const pmsImportDecisionSchema=z.object({id:z.string().min(1).max(160),expectedVersion:z.number().int().positive(),openingJournalId:z.string().min(1).max(160).nullable().default(null)}).strict();
export async function approvePmsImport(db:TransactionalSqlDatabase,actor:WorkContext,raw:unknown,key:string){
  requirePermission(actor,"pms.accounting.approve");requirePermission(actor,"finance.read");
  const input=pmsImportDecisionSchema.parse(raw);
  return db.transaction(async tx=>{
    const old=await replayCommand(tx,actor,key,"pms.import.approve",input.id,input);if(old)return old;
    const {batch,snapshot,definition}=await readBatch(tx,actor,input.id);checkCutover(snapshot,definition);
    if(batch.version!==input.expectedVersion||batch.state!=="preview")throw failed("PMS_IMPORT_STALE","Import batch changed. Refresh it.");
    if(samePerson(actor,batch.created_by))throw new ApiError(403,"PMS_IMPORT_INDEPENDENT","Finance approval must be independent of the preparer.");
    const simulated=simulate(snapshot,definition,batch.id,batch.created_by,Date.now()),{report}=simulated;
    if(!report.canApprove)throw failed("PMS_IMPORT_ERRORS","Correct all preview errors before approval.");
    await verifyOpening(tx,actor,simulated.snapshot,report,input.openingJournalId,definition,batch.id);
    const result={id:batch.id,state:"approved",version:batch.version+1};
    await executeCommand(tx,actor,key,"pms.import.approve",input,batch.id,result.version,result,[tx.prepare("UPDATE pms_import_batches SET state='approved',approved_by=?,opening_journal_id=?,version=version+1,updated_at=? WHERE id=?")
      .bind(actor.userId,input.openingJournalId,Date.now(),batch.id)],{companyId:actor.companyId,branchId:snapshot.property.branchId,departmentId:null});return result;
  });
}
export async function applyPmsImport(db:TransactionalSqlDatabase,actor:WorkContext,raw:unknown,key:string){
  requirePermission(actor,"pms.configure");requirePermission(actor,"pms.reservations");
  const input=z.object({id:z.string().min(1).max(160),expectedVersion:z.number().int().positive()}).strict().parse(raw);
  return db.transaction(async tx=>{
    const old=await replayCommand(tx,actor,key,"pms.import.apply",input.id,input);if(old)return old;
    const {batch,snapshot:initial,definition}=await readBatch(tx,actor,input.id);checkCutover(initial,definition);
    if(batch.version!==input.expectedVersion||batch.state!=="approved"||!batch.approved_by)throw failed("PMS_IMPORT_APPROVAL","Finance must approve this unchanged preview before applying it.");
    const now=Date.now(),{snapshot,report,links}=simulate(initial,definition,batch.id,batch.created_by,now);
    if(!report.canApprove)throw failed("PMS_IMPORT_ERRORS","Preview contains errors.");
    await verifyOpening(tx,actor,snapshot,report,batch.opening_journal_id,definition,batch.id);
    await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
    for(const entry of records(snapshot,"entry").filter(entry=>entry.data.reference===batch.id))entry.data.originalId=batch.opening_journal_id??"";
    await persistPmsRecords(tx,snapshot,new Map(initial.records.map(item=>[item.id,JSON.stringify(item)])));
    for(const link of links){
      const folio=record(snapshot,"folio",link.folioId),guest=record(snapshot,"guest",folio.data.guestId),partyId=folio.data.partyId||`pms-guest-${guest.id}`;
      await tx.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,'THB','active',?,?) ON CONFLICT(id) DO NOTHING`).bind(partyId,actor.companyId,`PMS-${guest.id}`,guest.data.name,now,now).run();
      for(const entry of records(snapshot,"entry").filter(entry=>entry.data.folioId===folio.id&&entry.data.reference===batch.id)){
        const pocket=entry.data.kind==="charge"?"due":entry.data.kind==="deposit"?"deposit":"advance",paymentId=pocket==="due"?null:id(batch.id,link.sourceKey,pocket,"credit");
        if(paymentId)await tx.prepare(`INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,'THB',?,0,'settled',?,?,?,?,?)`)
          .bind(paymentId,actor.companyId,partyId,`OPENING-${entry.id}`,definition.cutoverDate,entry.data.total,batch.opening_journal_id,batch.created_by,batch.approved_by,now,now).run();
        await tx.prepare(`INSERT INTO pms_import_balances(batch_id,source_key,property_id,folio_id,entry_id,pocket,amount_minor,finance_payment_id,finance_invoice_id) VALUES(?,?,?,?,?,?,?,?,?)`).bind(batch.id,link.sourceKey,batch.property_id,folio.id,entry.id,pocket,entry.data.total,paymentId,pocket==="due"?link.openingInvoiceId??null:null).run();
        await tx.prepare(`INSERT INTO pms_documents(id,property_id,folio_id,kind,finance_id,snapshot_json,created_at) VALUES(?,?,?,'opening',?,?,?)`).bind(id(entry.id,"document"),batch.property_id,folio.id,paymentId??link.openingInvoiceId??null,
          JSON.stringify({property:{name:snapshot.property.name,country:"TH",currency:"THB",branchId:snapshot.property.branchId},guest:guest.data,folio:folio.data,entries:[entry],amount:entry.data.total,businessDate:definition.cutoverDate,issuedBy:batch.created_by,accountingRevision:snapshot.property.config.revision,openingJournalId:batch.opening_journal_id}),now).run();
      }
    }
    const result={id:batch.id,state:"applied",version:batch.version+1,propertyId:batch.property_id,propertyVersion:initial.property.version+1};
    await executeCommand(tx,actor,key,"pms.import.apply",input,batch.id,result.version,result,[
      tx.prepare("UPDATE pms_properties SET version=version+1,updated_at=? WHERE id=?").bind(now,batch.property_id),
      tx.prepare("UPDATE pms_import_batches SET state='applied',version=version+1,updated_at=? WHERE id=?").bind(now,batch.id)],{companyId:actor.companyId,branchId:snapshot.property.branchId,departmentId:null});return result;
  });
}
