import { expect, it, vi } from "vitest";
import { cp, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { migrateDatabase } from "@erpx/server-runtime/public/runtime/migrations";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

const db = env.H002_DB;
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H002_DB,files:env.H002_FILES}),runtimeEnv:async()=>env}));
type TableShape = { table: string; columns: string[] };
async function existingShapes(database: SqlDatabase): Promise<TableShape[]> {
  const rows = (await database.prepare(`SELECT t.tablename,c.column_name FROM pg_tables t
    JOIN information_schema.columns c ON c.table_schema=t.schemaname AND c.table_name=t.tablename
    WHERE t.schemaname='public' AND t.tablename<>'erpx_migrations' ORDER BY t.tablename,c.ordinal_position`)
    .all<{tablename:string;column_name:string}>()).results;
  const tables = new Map<string,string[]>();
  for(const row of rows){const columns=tables.get(row.tablename)??[];columns.push(row.column_name);tables.set(row.tablename,columns);}
  return [...tables].map(([table,columns])=>({table,columns}));
}
async function digests(shapes: TableShape[]) {
  const rows=[];
  for(const {table,columns} of shapes){
    if(![table,...columns].every(name=>/^[a-z][a-z0-9_]*$/.test(name)))throw new Error("DECLARED_SCHEMA_IDENTIFIER_REQUIRED");
    const row=await db.prepare(`SELECT count(*) rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest
      FROM (SELECT ${columns.map(name=>`"${name}"`).join(',')} FROM "${table}") t`).first();
    rows.push({table,...row});
  }
  return rows;
}

it("adds Service intake to populated 1047 data without changing any existing column or migration receipt and retains the old Service and Support write shapes", async () => {
  const oldRoot=await mkdtemp(resolve(".runtime/tests/service-intake-before-")), currentRoot=await mkdtemp(resolve(".runtime/tests/service-intake-current-")), sourceRoot=resolve("migrations/postgres");
  try{
    for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,"holding-write"),{recursive:true});
    for(const file of (await readdir(join(sourceRoot,"holding-write"))).filter(name=>name.endsWith(".sql")&&Number(name.slice(0,4))<=1048)){
      await cp(join(sourceRoot,"holding-write",file),join(currentRoot,"holding-write",file));
      if(Number(file.slice(0,4))<=1047)await cp(join(sourceRoot,"holding-write",file),join(oldRoot,"holding-write",file));
    }
    await migrateDatabase(db,"holding-write",oldRoot);
    const f=await seedExpenseCompany(db,env.H002_FILES),customer=randomUUID(),sales=randomUUID(),ticket=randomUUID(),agreement=randomUUID(),order=randomUUID(),warehouse=randomUUID(),now=Date.now();
    await db.batch([
      db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Retained Service customer','THB',?,?)").bind(customer,f.company,customer,now,now),
      db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,'Retained Service warehouse')").bind(warehouse,f.company,f.branch,warehouse),
      db.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,'sales',?,?,?,'THB','2026-10-15','2026-11-15',10000,0,?,?,?)").bind(sales,f.company,f.branch,sales,customer,warehouse,f.owner.userId,now,now),
      db.prepare("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'Retained source case','Original recorded issue',?,'2026-10-16',?,?,?)").bind(ticket,f.company,f.branch,sales,customer,f.claimant.userId,f.owner.userId,now,now),
      db.prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES(?,?,1,'create',?,'Declared populated migration fixture',?,'2026-10-16','open',?,'{}')").bind(randomUUID(),ticket,f.owner.userId,f.claimant.userId,now),
      db.prepare("INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at) VALUES(?,?,?,?,'Retained reviewed agreement','2026-01-01','2027-12-31','THB',10000,'active',?,?,?,2,?,?)").bind(agreement,f.company,f.branch,customer,f.owner.userId,f.tie.userId,now,now,now),
      ...[1,2].map(version=>db.prepare("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,?,?,?,'Declared populated migration fixture',?,?)").bind(randomUUID(),agreement,version,version===1?'create':'activate',version===1?f.owner.userId:f.tie.userId,version===1?'draft':'active',now)),
      db.prepare("INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,scheduled_at,status,created_by,created_at,updated_at) VALUES(?,?,?,?,1,?,2,'Original recorded issue','Retained customer site',?,?,'scheduled',?,?,?)").bind(order,f.company,f.branch,ticket,agreement,f.claimant.userId,now+60000,f.owner.userId,now,now),
      db.prepare("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES(?,?,1,'create',?,'Declared populated migration fixture','scheduled',?,?,'{}',?)").bind(randomUUID(),order,f.owner.userId,f.claimant.userId,now+60000,now),
    ]);
    const shapes=await existingShapes(db),before=await digests(shapes),receipts=(await db.prepare("SELECT name,checksum FROM erpx_migrations ORDER BY name").all()).results;
    await migrateDatabase(db,"holding-write",currentRoot);
    expect(await digests(shapes)).toEqual(before);
    expect((await db.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1048_%' ORDER BY name").all()).results).toEqual(receipts);
    expect(await db.prepare("SELECT service_agreement_id,service_order_id,service_order_version FROM support_tickets WHERE id=?").bind(ticket).first()).toEqual({service_agreement_id:null,service_order_id:null,service_order_version:null});
    // An older compatible source directory leaves the additive schema installed.
    await migrateDatabase(db,"holding-write",oldRoot);
    await db.batch([
      db.prepare("UPDATE field_service_orders SET status='dispatched',version=version+1,updated_at=? WHERE id=? AND version=1").bind(now+1,order),
      db.prepare("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES(?,?,2,'dispatch',?,'Unchanged pre-intake writer','dispatched',?,?,'{}',?)").bind(randomUUID(),order,f.claimant.userId,f.claimant.userId,now+60000,now+1),
      db.prepare("UPDATE support_tickets SET title='Retained source case amended',version=version+1,updated_at=? WHERE id=? AND version=1").bind(now+1,ticket),
      db.prepare("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES(?,?,2,'add-information',?,'Unchanged pre-intake writer',?,'2026-10-16','open',?,'{}')").bind(randomUUID(),ticket,f.owner.userId,f.claimant.userId,now+1),
    ]);
    expect(await db.prepare("SELECT status,version FROM field_service_orders WHERE id=?").bind(order).first()).toEqual({status:'dispatched',version:2});
    expect(await db.prepare("SELECT source_kind,status,version,service_order_id FROM support_tickets WHERE id=?").bind(ticket).first()).toEqual({source_kind:'sales_order',status:'open',version:2,service_order_id:null});
    expect(await db.prepare("SELECT count(*) total FROM company_package_grants").first()).toEqual({total:0});
    expect(await db.prepare("SELECT count(*) total FROM erpx_migrations WHERE name LIKE '1048_%'").first()).toEqual({total:1});
  }finally{await rm(oldRoot,{recursive:true,force:true});await rm(currentRoot,{recursive:true,force:true});}
},60000);

it("adds the no-charge decision to populated1054 without rewriting any original rows or receipts and keeps default-disabled old Service commands compatible",async()=>{
 const oldRoot=await mkdtemp(resolve('.runtime/tests/service-no-charge-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/service-no-charge-current-')),sourceRoot=resolve('migrations/postgres');
 try{
  for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,'holding-write'),{recursive:true});
  for(const file of (await readdir(join(sourceRoot,'holding-write'))).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1055)){await cp(join(sourceRoot,'holding-write',file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1054)await cp(join(sourceRoot,'holding-write',file),join(oldRoot,'holding-write',file));}
  await migrateDatabase(db,'holding-write',oldRoot);const f=await seedExpenseCompany(db,env.H002_FILES);for(const actor of [f.owner,f.tie,f.claimant])actor.modules=['field_service'];await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,'receivable',1,1)").bind(`${f.company}-receivable`,f.chart,f.company,'1200','Original populated trade receivables').run();
  const {createParty,partySchema}=await import('@erpx/server-finance/public/finance'),{createAgreement,changeAgreement,createServiceRequest,changeServiceOrder}=await import('@erpx/server-field_service/public/field-service'),{serviceNoChargeControls,confirmServiceNoCharge}=await import('@erpx/server-finance/public/service-no-charge');
  const customer=String((await createParty(f.owner,partySchema.parse({kind:'customer',code:randomUUID(),name:'Original old-schema customer',currency:'THB'}),randomUUID())).id),agreement=await createAgreement(f.owner,{branchId:f.branch,departmentId:null,customerId:customer,title:'Actual original non-warranty retained agreement',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:10000},randomUUID());
  await changeAgreement(f.tie,String(agreement.id),{action:'activate',expectedVersion:1,reason:'Original independent pre-migration agreement review'},randomUUID());const request={agreementId:String(agreement.id),agreementVersion:2,title:'Original current old-schema visit',issue:'Inspect actual non-warranty equipment',site:'Original customer site',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},visit=await createServiceRequest(f.owner,request,randomUUID());
  const asset=randomUUID(),coveredKey=randomUUID(),now=Date.now();await db.prepare("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at) VALUES(?,?,?,?,'Populated original covered equipment','equipment','THB',8000,'2026-01-01','2099-12-31',?,?,?)").bind(asset,f.company,f.branch,asset,f.owner.userId,now,now).run();const covered=await createAgreement(f.owner,{branchId:f.branch,departmentId:null,customerId:customer,assetId:asset,title:'Retained original fully covered agreement',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:10000,warrantyDiscountBps:10000},randomUUID());await changeAgreement(f.tie,String(covered.id),{action:'activate',expectedVersion:1,reason:'Original independent full warranty agreement review'},randomUUID());const coveredVisit=await createServiceRequest(f.owner,{...request,agreementId:String(covered.id)},randomUUID());for(const input of [{action:'dispatch',expectedVersion:1,reason:'Original covered dispatch'},{action:'start',expectedVersion:2,reason:'Original customer site access'},{action:'complete',expectedVersion:3,laborMinutes:20,evidence:'Original pre1055 completed inspection',customerConfirmation:'Original customer accepted covered visit',reason:'Actual completed pre1055 covered source'}])await changeServiceOrder(f.claimant,String(coveredVisit.id),input,randomUUID());await changeServiceOrder(f.tie,String(coveredVisit.id),{action:'release-billing',expectedVersion:4,reason:'Original independent pre1055 full warranty release'},coveredKey);const coveredInvoice=`field-invoice-${coveredKey}`;expect(await db.prepare('SELECT total_minor,status FROM finance_invoices WHERE id=?').bind(coveredInvoice).first()).toEqual({total_minor:0,status:'draft'});
  // Prepare the original invoice responsibility while still on1054; this is
  // ordinary record-only Flow, and its rows/receipt join the pre-upgrade image.
  const {resolveFlow,assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing');
  const invoiceFlow=await resolveFlow(db,f.owner,'invoice',coveredInvoice);
  await assignFlowPrimary(db,f.owner,'invoice',coveredInvoice,{stepKey:'draft',userId:f.manager.userId,expectedVersion:invoiceFlow.version,recordVersion:invoiceFlow.recordVersion,reason:'Original Company administrator assigns independent Finance review before the additive upgrade',saveDefault:false},randomUUID());
  expect(await serviceNoChargeControls(f.manager,coveredInvoice)).toBeNull();const unavailableKey=randomUUID();await expect(confirmServiceNoCharge(f.manager,coveredInvoice,{expectedVersion:1,contextKey:'a'.repeat(64),reason:'New operation cannot invent authority on the old schema'},unavailableKey)).rejects.toMatchObject({status:503,code:'SERVICE_NO_CHARGE_SCHEMA_REQUIRED'});expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(unavailableKey).first()).toBeNull();
  const shapes=await existingShapes(db),before=await digests(shapes),baselines=(await db.prepare('SELECT * FROM work_cache_revision_baselines ORDER BY table_name').all()).results,receipts=(await db.prepare('SELECT name,checksum FROM erpx_migrations ORDER BY name').all()).results;await migrateDatabase(db,'holding-write',currentRoot);const after=await digests(shapes);expect(after.filter(row=>row.table!=='work_cache_revision_baselines')).toEqual(before.filter(row=>row.table!=='work_cache_revision_baselines'));expect((await db.prepare("SELECT * FROM work_cache_revision_baselines WHERE table_name<>'finance_service_no_charge_confirmations' ORDER BY table_name").all()).results).toEqual(baselines);expect(await db.prepare("SELECT table_name,revision FROM work_cache_revision_baselines WHERE table_name='finance_service_no_charge_confirmations'").first()).toEqual({table_name:'finance_service_no_charge_confirmations',revision:2});expect((await db.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1055_%' ORDER BY name").all()).results).toEqual(receipts);expect(await db.prepare('SELECT count(*)::int n FROM finance_service_no_charge_confirmations').first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM finance_service_no_charge_completed').first('n')).toBe(0);expect(await serviceNoChargeControls(f.manager,coveredInvoice)).toMatchObject({invoiceId:coveredInvoice,version:1,confirmed:false,canConfirm:true});expect(await db.prepare('SELECT total_minor,status FROM finance_invoices WHERE id=?').bind(coveredInvoice).first()).toEqual({total_minor:0,status:'draft'});
  await migrateDatabase(db,'holding-write',oldRoot);const dispatch={action:'dispatch',expectedVersion:1,reason:'Unchanged original mode-disabled Service command after additive migration'},command=randomUUID(),updated=await changeServiceOrder(f.claimant,String(visit.id),dispatch,command);expect(updated).toMatchObject({id:visit.id,status:'dispatched',version:2});expect(await changeServiceOrder(f.claimant,String(visit.id),dispatch,command)).toEqual(updated);expect((await createServiceRequest(f.owner,{...request,title:'New ordinary visit after additive migration'},randomUUID())).version).toBe(1);expect(await db.prepare('SELECT count(*)::int n FROM company_package_grants WHERE company_id=?').bind(f.company).first('n')).toBe(0);expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1055_%'").first('n')).toBe(1);
 }finally{await rm(oldRoot,{recursive:true,force:true});await rm(currentRoot,{recursive:true,force:true});}
},60000);
