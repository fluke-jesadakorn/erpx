import {expect,it,vi} from 'vitest';
import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {Pool} from 'pg';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {LocalFileStore} from '@erpx/server-runtime/adapters/runtime/filesystem';
import {join,resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {env} from '../../support/runtime-env';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';
import {mapSpendingOrganization} from '../../support/helpers/spending-organization';
import {migrateDatabase} from '@erpx/server-runtime/public/runtime/migrations';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {withFinanceFlowAssignment} from '@erpx/server-finance/public/document-purchases';
import {postInvoice,postJournal} from '@erpx/server-finance/public/finance';
import {completeCommerceReturn,decideCommerceReturn,fulfillOrder,getOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,returnRequestSchema,saveMaster,saveOrder,settleInvoice,transitionOrder,fulfillmentSchema} from '@erpx/server-commerce/public/commerce';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

let db:PostgresDatabase,files:LocalFileStore;
const key=()=>randomUUID(),run=promisify(execFile),pgDump='/opt/homebrew/opt/postgresql@18/bin/pg_dump',pgRestore='/opt/homebrew/opt/postgresql@18/bin/pg_restore';
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files}),runtimeEnv:async()=>env}));
async function snapshot(database:SqlDatabase){
 const names=(await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename);
 return Promise.all(names.map(async table=>{if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_IDENTIFIER_REQUIRED');return {table,...await database.prepare(`SELECT count(*)::int rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest FROM "${table}" t`).first<{rows:number;digest:string}>()};}));
}
function native(error:unknown):{code?:string;constraint?:string;message?:string}{
 let value=error;for(let count=0;count<6&&value&&typeof value==='object';count++){if('code' in value&&value.code==='23514')return {code:'23514',constraint:'constraint' in value&&typeof value.constraint==='string'?value.constraint:undefined,message:'message' in value&&typeof value.message==='string'?value.message:undefined};if(!('cause' in value))break;value=value.cause;}return {};
}

it('upgrades populated1055 without rewriting old rows and admits only original warehouse-free Company service refunds under1056',async()=>{
 const oldRoot=await mkdtemp(resolve('.runtime/tests/company-service-refund-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/company-service-refund-current-')),source=resolve('migrations/postgres'),directory=await mkdtemp(resolve('.runtime/tests/company-service-refund-data-'));
 const adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');if(!['postgresql:','postgres:'].includes(adminUrl.protocol)||adminUrl.search||adminUrl.hash||adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 const prefix=`erpx_test_refund_${key().replaceAll('-','')}`,beforeName=`${prefix}_before`,restoreName=`${prefix}_restore`,admin=new Pool({connectionString:adminUrl.href,max:1}),databases:PostgresDatabase[]=[],created:string[]=[];
 async function createOwned(name:string){if(!name.startsWith(prefix+'_'))throw Error('ISOLATED_DATABASE_REQUIRED');await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);created.push(name);const url=new URL(adminUrl);url.pathname=name;const value=new PostgresDatabase({connectionString:url.href,max:4});databases.push(value);return value;}
 try{
  const guard=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(guard?.name!=='erpx_test_admin'||guard.rolsuper||!guard.rolcreatedb||guard.rolcreaterole)throw Error('UNSAFE_TEST_DATABASE_ROLE');
  db=await createOwned(beforeName);files=new LocalFileStore(join(directory,'files'));expect(await db.prepare("SELECT to_regclass('public.erpx_migrations') name").first('name')).toBeNull();
  for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,'holding-write'),{recursive:true});
  for(const file of (await readdir(join(source,'holding-write'))).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1056)){
   await cp(join(source,'holding-write',file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1055)await cp(join(source,'holding-write',file),join(oldRoot,'holding-write',file));
  }
  await migrateDatabase(db,'holding-write',oldRoot);
  expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1056_%'").first('n')).toBe(0);expect(await db.prepare("SELECT to_regprocedure('finance_company_service_refund_source_valid(text)') name").first('name')).toBeNull();const oldShape=await db.prepare("SELECT pg_get_constraintdef(oid) shape FROM pg_constraint WHERE conrelid='finance_refund_obligations'::regclass AND conname='finance_refund_source_shape'").first('shape');
  // Original isolated configuration/actors only. Every service/financial source
  // below is created through the normal owner commands, never seeded as posted.
  const f=await seedExpenseCompany(db,files);await mapSpendingOrganization(f);
  f.head.role='sales_manager';f.head.assignmentId=await f.grant(f.head,'sales_manager');
  async function assign(kind:'order'|'invoice'|'journal',id:string,stepKey:string,userId:string){const flow=await resolveFlow(db,f.owner,kind,id),command=key(),input={stepKey,userId,reason:'Original independent Company service migration responsibility',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion};if(kind==='order')await assignFlowPrimary(db,f.owner,kind,id,input,command);else await withFinanceFlowAssignment(f.owner,command,tx=>assignFlowPrimary(tx??db,f.owner,kind,id,input,command),{kind,id});}
  const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Fictional original Company service customer'},key()),product=await saveMaster(f.owner,'products',null,{sku:key(),name:'Original warehouse-free service',price:'100',itemType:'service'},key());
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-03',lines:[{productId:product.id,quantity:'1',price:'100',taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());await assign('order',orderId,'confirmed',f.owner.userId);
  const detail=await getOrder(f.owner,orderId),fulfilled=await fulfillOrder(f.owner,orderId,fulfillmentSchema.parse({completionMode:'service',expectedVersion:3,date:'2026-09-02',evidenceNote:'Original fictional customer confirmed completed service',lines:[{lineId:String(detail.lines[0].id),quantity:'1'}]}),key());
  await assign('order',orderId,'invoicing',f.owner.userId);const invoice=await invoiceFulfillment(f.owner,String(fulfilled.fulfillmentId),{expectedVersion:1,date:'2026-09-02',dueDate:'2026-09-03'},key()),invoiceId=String(invoice.invoiceId);
  await assign('invoice',invoiceId,'draft',f.manager.userId);await postInvoice(f.manager,invoiceId,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());await assign('invoice',invoiceId,'posted',f.payer.userId);await settleInvoice(f.payer,invoiceId,{expectedVersion:2,date:'2026-09-03',amount:'50',reference:'Fictional original partial service receipt',attachmentSetIds:[]},key());
  const invoiced=await getOrder(f.owner,orderId),requested=await requestCommerceReturn(f.owner,String(fulfilled.fulfillmentId),returnRequestSchema.parse({expectedVersion:2,date:'2026-09-04',reason:'Customer independently accepted return of completed service',lines:[{lineId:String(invoiced.fulfillmentLines[0].id),quantity:'1'}]}),key()),returnId=String(requested.id),approved=await decideCommerceReturn(f.head,returnId,'approve',{expectedVersion:1,reason:'Different Sales manager reviewed original service source'},key());
  expect(approved.stockJournalId).toBeUndefined();await assign('journal',String(approved.creditJournalId),'draft',f.manager.userId);await postJournal(f.manager,String(approved.creditJournalId),key(),{expectedVersion:1});
  const command=key(),input={expectedVersion:2};let oldFailure:unknown;try{await completeCommerceReturn(f.manager,returnId,input,command);}catch(error){oldFailure=error;}expect(native(oldFailure)).toMatchObject({code:'23514',constraint:'finance_refund_source_shape'});
  expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();expect(await db.prepare('SELECT branch_id,warehouse_id,department_id FROM commerce_orders WHERE id=?').bind(orderId).first()).toEqual({branch_id:null,warehouse_id:null,department_id:null});
  const retained=await snapshot(db),receipts=(await db.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all()).results,archive=join(directory,'populated-pre1056.dump'),cliEnv={...process.env,PGHOST:adminUrl.hostname,PGPORT:adminUrl.port,PGUSER:adminUrl.username,PGPASSWORD:decodeURIComponent(adminUrl.password),PGDATABASE:beforeName};
  expect((await run(pgDump,['--version'],{env:cliEnv})).stdout).toContain('18.6');expect((await run(pgRestore,['--version'],{env:cliEnv})).stdout).toContain('18.6');
  await run(pgDump,['--format=custom','--no-owner','--no-privileges','--file',archive],{env:cliEnv,timeout:120000});const restored=await createOwned(restoreName);
  await run(pgRestore,['--exit-on-error','--transaction-size=64','--no-owner','--no-privileges','--dbname',restoreName,archive],{env:{...cliEnv,PGDATABASE:restoreName},timeout:120000});
  expect(await snapshot(restored)).toEqual(retained);expect((await restored.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all()).results).toEqual(receipts);expect(await restored.prepare("SELECT to_regprocedure('finance_company_service_refund_source_valid(text)') name").first('name')).toBeNull();expect(await restored.prepare("SELECT pg_get_constraintdef(oid) shape FROM pg_constraint WHERE conrelid='finance_refund_obligations'::regclass AND conname='finance_refund_source_shape'").first('shape')).toEqual(oldShape);
  await migrateDatabase(db,'holding-write',currentRoot);
  expect(await snapshot(db)).toEqual(retained);expect((await db.prepare("SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations WHERE name NOT LIKE '1056_%' ORDER BY name").all()).results).toEqual(receipts);expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1056_%'").first('n')).toBe(1);
  const completed=await completeCommerceReturn(f.manager,returnId,input,command),refundId=String(completed.refundId);expect(completed).toMatchObject({status:'posted',creditedMinor:5000,refundMinor:5000,refundId:`refund-${returnId}`});expect(await completeCommerceReturn(f.manager,returnId,input,command)).toEqual(completed);
  expect(await db.prepare('SELECT branch_id,department_id,amount_minor,base_amount_minor FROM finance_refund_obligations WHERE id=?').bind(refundId).first()).toEqual({branch_id:null,department_id:null,amount_minor:5000,base_amount_minor:5000});expect(await db.prepare('SELECT finance_company_service_refund_source_valid(?) valid').bind(refundId).first('valid')).toBe(true);
  // Ordinary catalogue description/status edits are not fresh source authority;
  // captured SERVICE semantics remain usable, while original type locking stays.
  await saveMaster(f.owner,'products',String(product.id),{expectedVersion:Number(await db.prepare('SELECT version FROM commerce_products WHERE id=?').bind(product.id).first('version')),sku:key(),name:'Retained service now discontinued',price:'125',itemType:'service',status:'archived'},key());expect(await db.prepare('SELECT finance_company_service_refund_source_valid(?) valid').bind(refundId).first('valid')).toBe(true);
  await expect(db.prepare("UPDATE commerce_products SET item_type='stock' WHERE id=?").bind(product.id).run()).rejects.toThrow('ITEM_TYPE_LOCKED');
  await expect(db.prepare('UPDATE finance_refund_obligations SET branch_id=? WHERE id=?').bind(f.branch,refundId).run()).rejects.toThrow('FINANCE_COMPANY_SERVICE_REFUND_SCOPE_IMMUTABLE');
  // Probe real final database checks, rolling every adverse source edit back.
  const wrongWarehouse=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Different real warehouse not owned by this service source'},key()),wrongProduct=await saveMaster(f.owner,'products',null,{sku:key(),name:'Different real service item',itemType:'service',price:'25'},key()),foreign=await seedExpenseCompany(db,files);
  const adverse=[
   {sql:"UPDATE commerce_fulfillments SET completion_kind='goods' WHERE id=?",args:[fulfilled.fulfillmentId]},
   {sql:"UPDATE commerce_fulfillments SET completion_kind='mixed' WHERE id=?",args:[fulfilled.fulfillmentId]},
   {sql:'UPDATE commerce_orders SET warehouse_id=? WHERE id=?',args:[wrongWarehouse.id,orderId]},
   {sql:'UPDATE commerce_orders SET company_id=? WHERE id=?',args:[foreign.company,orderId]},
   {sql:'UPDATE commerce_fulfillment_lines SET value_minor=1 WHERE fulfillment_id=?',args:[fulfilled.fulfillmentId]},
   {sql:'UPDATE commerce_return_lines SET product_id=? WHERE return_id=?',args:[wrongProduct.id,returnId]},
   {sql:'DELETE FROM commerce_return_lines WHERE return_id=?',args:[returnId]},
   {sql:"UPDATE command_receipts SET result_json=jsonb_set(result_json::jsonb,'{refundId}',to_jsonb(?::text))::text WHERE command_id=?",args:['not-the-original-refund',command]},
   {sql:'UPDATE finance_refund_obligations SET created_by=? WHERE id=?',args:[f.owner.userId,refundId]},
  ];
  const intact=await snapshot(db);
  for(const mutation of adverse){let failure:unknown;try{await db.transaction(async tx=>{await tx.batch([tx.prepare(mutation.sql).bind(...mutation.args),tx.prepare('UPDATE finance_refund_obligations SET version=version+1 WHERE id=?').bind(refundId)]);});}catch(error){failure=error;}expect(native(failure)).toMatchObject({code:'23514',constraint:'finance_refund_source_shape',message:'FINANCE_COMPANY_SERVICE_REFUND_SOURCE_REQUIRED'});expect(await snapshot(db)).toEqual(intact);}
  // This is the original captured-item rule, not the new1056 proof trigger.
  await expect(db.prepare("UPDATE commerce_order_lines SET item_type='stock' WHERE order_id=?").bind(orderId).run()).rejects.toMatchObject({code:'P0001',message:'ITEM_TYPE_CHANGED: Refresh the catalogue item before preparing the document'});
  // The original exclusive owner check still rejects neither-source/both-source;
  // NOT NULL-Branch Goods path is not replaced by a broad branchless allowance.
  for(const sourceShape of [{return_id:null,billing_correction_id:null},{billing_correction_id:key()}]){let failure:unknown;try{await db.prepare('INSERT INTO finance_refund_obligations SELECT (jsonb_populate_record(NULL::finance_refund_obligations,to_jsonb(original)||?::jsonb)).* FROM finance_refund_obligations original WHERE id=?').bind(JSON.stringify({id:key(),...sourceShape}),refundId).run();}catch(error){failure=error;}expect(native(failure)).toMatchObject({code:'23514',constraint:'finance_refund_source_shape',message:expect.stringContaining('violates check constraint')});expect(await snapshot(db)).toEqual(intact);}
  await expect(db.prepare('UPDATE finance_refund_obligations SET return_id=NULL WHERE id=?').bind(refundId).run()).rejects.toThrow('FINANCE_REFUND_SOURCE_IMMUTABLE');
  await migrateDatabase(db,'holding-write',oldRoot);expect(await completeCommerceReturn(f.manager,returnId,input,command)).toEqual(completed);expect(await db.prepare('SELECT count(*)::int n FROM company_package_grants WHERE company_id=?').bind(f.company).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock_moves WHERE fulfillment_id=?').bind(fulfilled.fulfillmentId).first('n')).toBe(0);
  // Real restored pre-upgrade database still fails the original constraint;
  // only its additive upgrade permits that exact uncommitted original command.
  db=restored;let restoredFailure:unknown;try{await completeCommerceReturn(f.manager,returnId,input,command);}catch(error){restoredFailure=error;}expect(native(restoredFailure)).toMatchObject({code:'23514',constraint:'finance_refund_source_shape'});expect(await snapshot(db)).toEqual(retained);await migrateDatabase(db,'holding-write',currentRoot);expect(await completeCommerceReturn(f.manager,returnId,input,command)).toEqual(completed);expect(await completeCommerceReturn(f.manager,returnId,input,command)).toEqual(completed);
 }finally{
  await Promise.all(databases.map(value=>value.close()));for(const name of [...created].reverse()){const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[name])).rows[0]?.owner;if(owner!=='erpx_test_admin')throw Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${name}"`);}await admin.end();await Promise.all([oldRoot,currentRoot,directory].map(path=>rm(path,{recursive:true,force:true})));
 }
},180000);
