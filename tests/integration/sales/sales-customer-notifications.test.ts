import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {beforeAll,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,key} from "../../support/helpers/expense-fixture";
import {migrateDatabase} from '@erpx/server-runtime/public/runtime/migrations';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';

/** Notification settings/queue/retry commands are native-only. Their command
 * oracles live in the native Sales unit/Postgres/HTTP suites; this retained
 * suite proves the populated history upgrades keep old rows and receipts. */
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
it('upgrades populated 1049 notice history without rewriting prior rows or receipts and keeps disabled old writers compatible',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');const database=env.H002_DB,oldRoot=await mkdtemp(resolve('.runtime/tests/sales-notice-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/sales-notice-current-')),source=resolve('migrations/postgres');
 try{
  for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,'holding-write'),{recursive:true});
  for(const file of (await readdir(join(source,'holding-write'))).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1050)){await cp(join(source,'holding-write',file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1049)await cp(join(source,'holding-write',file),join(oldRoot,'holding-write',file));}
  await migrateDatabase(database,'holding-write',oldRoot);const old=await seedExpenseCompany(database,env.H002_FILES),customer=key(),warehouse=key(),order=key(),now=Date.now(),notices=[{id:key(),delivery:key()},{id:key(),delivery:key()}];
  await database.batch([
   database.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,notification_email,notification_consent,created_at,updated_at) VALUES(?,?,'customer',?,'Declared populated notice customer','THB','retained@example.invalid','granted',?,?)").bind(customer,old.company,customer,now,now),
   database.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,'Declared populated notice warehouse')").bind(warehouse,old.company,old.branch,warehouse),
   database.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,'sales',?,?,?,'THB','2026-10-16','2026-10-17','fulfilled',1000,0,?,?,?)").bind(order,old.company,old.branch,order,customer,warehouse,old.owner.userId,now,now),
   database.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES(?,'sandbox',1,?,?)").bind(old.company,old.owner.userId,now),
   ...notices.flatMap(notice=>[
    database.prepare('INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES(?,?,?,\'2026-10-16\',?,?)').bind(notice.delivery,order,notice.delivery,old.owner.userId,now),
    database.prepare("INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,next_attempt_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,'retained@example.invalid','Declared original old message','Retained populated immutable original snapshot','sandbox',?,?,?,?)").bind(notice.id,old.company,old.branch,order,notice.delivery,customer,now,old.owner.userId,now,now)
   ])
  ]);
  expect(await database.prepare('SELECT status FROM commerce_customer_notifications WHERE id=?').bind(notices[0].id).first('status')).toBe('pending');expect(await database.prepare("SELECT to_regclass('sales_notification_execution_authorizations') present").first('present')).toBeNull();
  const tables=(await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename),snapshot=async()=>{const result=[];for(const table of tables){if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_TABLE_REQUIRED');const extra=table==='work_cache_revision_baselines'?" WHERE table_name NOT IN ('sales_notification_execution_authorizations','sales_notification_execution_attempts')":'';const row=await database.prepare(`SELECT count(*) rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest FROM "${table}" t${extra}`).first();result.push({table,...row});}return result;},before=await snapshot(),receipts=(await database.prepare('SELECT name,checksum FROM erpx_migrations ORDER BY name').all()).results;
  await migrateDatabase(database,'holding-write',currentRoot);expect(await snapshot()).toEqual(before);expect((await database.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1050_%' ORDER BY name").all()).results).toEqual(receipts);for(const table of ['sales_notification_execution_authorizations','sales_notification_execution_attempts'])expect(await database.prepare(`SELECT count(*) count FROM ${table}`).first('count')).toBe(0);
  await migrateDatabase(database,'holding-write',oldRoot);expect(await database.prepare('SELECT status FROM commerce_customer_notifications WHERE id=?').bind(notices[1].id).first('status')).toBe('pending');expect(await database.prepare('SELECT count(*) count FROM company_package_grants').first('count')).toBe(0);expect(await database.prepare("SELECT count(*) count FROM erpx_migrations WHERE name LIKE '1050_%'").first('count')).toBe(1);
 }finally{await rm(oldRoot,{recursive:true,force:true});await rm(currentRoot,{recursive:true,force:true});}
},60000);

/** Seed the exact durable uncertain attempt a lost provider response leaves.
 * Delivery is native-only, so the reconciliation oracle starts from the owner
 * rows instead of a second TypeScript writer. */

it('upgrades populated 1050 delivery and immutable uncertain attempts without rewriting prior rows or receipts',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');const database=env.H003_DB,oldRoot=await mkdtemp(resolve('.runtime/tests/sales-reconcile-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/sales-reconcile-current-')),sourceRoot=resolve('migrations/postgres');
 try{
  for(const directory of [oldRoot,currentRoot])await mkdir(join(directory,'holding-write'),{recursive:true});for(const file of(await readdir(join(sourceRoot,'holding-write'))).filter(name=>name.endsWith('.sql')&&Number(name.slice(0,4))<=1051)){await cp(join(sourceRoot,'holding-write',file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1050)await cp(join(sourceRoot,'holding-write',file),join(oldRoot,'holding-write',file));}
  await migrateDatabase(database,'holding-write',oldRoot);const old=await seedExpenseCompany(database,env.H003_FILES),customer=key(),warehouse=key(),order=key(),delivery=key(),notice=key(),authorization=key(),attempt=key(),now=Date.now(),hash='1'.repeat(64);
  await database.batch([
   database.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,notification_email,notification_consent,created_at,updated_at) VALUES(?,?,'customer',?,'Declared populated reconciliation customer','THB','retained@example.invalid','granted',?,?)").bind(customer,old.company,customer,now,now),
   database.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,'Declared populated reconciliation warehouse')").bind(warehouse,old.company,old.branch,warehouse),
   database.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,'sales',?,?,?,'THB','2026-10-16','2026-10-17','fulfilled',1000,0,?,?,?)").bind(order,old.company,old.branch,order,customer,warehouse,old.owner.userId,now,now),
   database.prepare("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES(?,'sandbox',1,?,?)").bind(old.company,old.owner.userId,now),
   database.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at) VALUES(?,?,?,'2026-10-16',?,?)").bind(delivery,order,delivery,old.owner.userId,now),
   database.prepare("INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,status,attempts,next_attempt_at,provider_message_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,1,'retained@example.invalid','Original retained uncertain message','Original immutable message','sandbox','unknown',1,?,'captured-original-provider',?,?,?)").bind(notice,old.company,old.branch,order,delivery,customer,now,old.owner.userId,now,now),
   database.prepare("INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) VALUES(?,?,?,'queue',?,?,'{}',?,1,?)").bind(authorization,notice,old.company,old.owner.userId,old.owner.assignmentId,hash,now),
   database.prepare("INSERT INTO sales_notification_execution_attempts(id,notification_id,authorization_command_id,company_id,attempt,lease_until,source_hash,state,provider_message_id,result_code,created_at,updated_at) VALUES(?,?,?,?,1,?,?,'unknown','captured-original-provider','CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',?,?)").bind(attempt,notice,authorization,old.company,now,hash,now,now)
  ]);
  const tables=(await database.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename),snapshot=async()=>{const rows=[];for(const table of tables){if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_TABLE_REQUIRED');rows.push({table,...await database.prepare(`SELECT count(*) rows,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) digest FROM "${table}" t${table==='work_cache_revision_baselines'?" WHERE table_name<>'sales_notification_reconciliations'":''}`).first()});}return rows;},before=await snapshot(),receipts=(await database.prepare('SELECT name,checksum FROM erpx_migrations ORDER BY name').all()).results;
  await migrateDatabase(database,'holding-write',currentRoot);expect(await snapshot()).toEqual(before);expect((await database.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1051_%' ORDER BY name").all()).results).toEqual(receipts);expect(await database.prepare('SELECT count(*) count FROM sales_notification_reconciliations').first('count')).toBe(0);await migrateDatabase(database,'holding-write',oldRoot);expect(await snapshot()).toEqual(before);expect(await database.prepare('SELECT state,provider_message_id FROM sales_notification_execution_attempts WHERE id=?').bind(attempt).first()).toEqual({state:'unknown',provider_message_id:'captured-original-provider'});expect(await database.prepare('SELECT status FROM commerce_customer_notifications WHERE id=?').bind(notice).first('status')).toBe('unknown');expect(await database.prepare('SELECT count(*) count FROM company_package_grants').first('count')).toBe(0);
 }finally{await rm(oldRoot,{recursive:true,force:true});await rm(currentRoot,{recursive:true,force:true});}
},60000);
