import {fulfillmentSchema} from '@erpx/server-commerce/public/commerce';
import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {deliveryEvidence} from "../../support/helpers/delivery-evidence";
import {orderSchema,saveMaster,saveOrder,transitionOrder} from '@erpx/server-commerce/public/commerce';
import {seedCommerceFulfillment} from '../../support/helpers/commerce-fulfillment-seed';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {queueDeliveryNotification,retryCustomerNotification,updateCustomerNotificationSettings} from "@erpx/server-sales/public/sales-customer-notifications";
import {packageGrantStore,readCompanyPackageGrants} from '@erpx/server-commercial/public/package-grants';
import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {migrateDatabase} from '@erpx/server-runtime/public/runtime/migrations';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='sales_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'sales_manager');});
afterEach(()=>vi.unstubAllEnvs());

async function delivered(){
  const warehouse=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Notification warehouse'},key())).id);
  const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Notification product',price:'10'},key())).id);
  const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Notification customer',notificationEmail:'receiving@example.invalid',notificationConsent:'granted'},key())).id);
  await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,1200,1)').bind(warehouse,product).run();
  const order=await saveOrder(f.owner,null,orderSchema.parse({kind:'sales',partyId:customer,warehouseId:warehouse,orderDate:'2026-09-21',dueDate:'2026-09-22',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),key()),orderId=String(order.id);
  await transitionOrder(f.owner,orderId,{action:'submit',expectedVersion:1},key());await transitionOrder(f.head,orderId,{action:'approve',expectedVersion:2},key());
  const lineId=String((await getOrder(f.owner,orderId)).lines[0].id),proof=await deliveryEvidence(db,f.owner,orderId);
  const delivery=await seedCommerceFulfillment(db,f.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-22',lines:[{lineId,quantity:'1'}],...proof}),key());
  return {orderId,customer,fulfillmentId:String(delivery.fulfillmentId)};
}

it('queues a consent-aware immutable snapshot for the native delivery worker',async()=>{
  const source=await delivered();
  await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},key())).rejects.toMatchObject({code:'CUSTOMER_NOTIFICATION_PROVIDER_REQUIRED'});
  await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());
  const command=key(),queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},command);
  expect(await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Carrier CR-101 confirmed the delivery window'},command)).toEqual(queued);
  const before=await db.prepare('SELECT * FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first<Record<string,unknown>>();
  expect(before).toMatchObject({recipient_email:'receiving@example.invalid',adapter:'sandbox',status:'pending',attempts:0});
  // Delivery is owned by the native worker; its own Postgres suite proves the
  // claim, sandbox identity, terminal state and replay.
  expect(await db.prepare('SELECT status,attempts,provider_message_id FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'pending',attempts:0,provider_message_id:null});
  await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Duplicate customer notice'},key())).rejects.toMatchObject({code:'CUSTOMER_ALREADY_NOTIFIED'});
  await expect(db.prepare('DELETE FROM commerce_customer_notifications WHERE id=?').bind(queued.id).run()).rejects.toThrow('CUSTOMER_NOTIFICATION_HISTORY_IMMUTABLE');
});

it('retries only a failed immutable notice through its original queued authority',async()=>{
  const source=await delivered();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());
  const queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Retained failed customer notice'},key());
  await expect(retryCustomerNotification(f.owner,String(queued.id),{expectedVersion:1},key())).rejects.toMatchObject({code:'CUSTOMER_NOTIFICATION_NOT_FAILED'});
  await db.prepare("UPDATE commerce_customer_notifications SET status='failed',version=version+1,last_error='CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED' WHERE id=?").bind(queued.id).run();
  await retryCustomerNotification(f.owner,String(queued.id),{expectedVersion:2},key());
  expect(await db.prepare('SELECT status,attempts FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'pending',attempts:0});
  // The retained disabled boundary writes no purchased authorization; it must
  // never invent a queued worker authority for the native worker.
  expect(await db.prepare('SELECT count(*) n FROM sales_notification_execution_authorizations WHERE notification_id=?').bind(queued.id).first('n')).toBe(0);
});

it('holds future customer delivery jobs without sending or consuming them in production',async()=>{
  const source=await delivered();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'resend'},key());
  const queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Deferred pilot notice'},key());
  vi.stubEnv('NODE_ENV','production');
  try {
    await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,{expectedVersion:1,note:'Another notice'},key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
    // The native worker's production release gate is proved in its own
    // Postgres suite; no TypeScript writer consumes the held notice.
    expect(await db.prepare('SELECT status,attempts FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'pending',attempts:0});
  } finally {vi.unstubAllEnvs();}
});

async function noticeSales(state:'active'|'draining'='active',endsAt:number|null=null){const prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId==='sales');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-customer-notice-provider'}),obligations:async()=>{throw Error('NOTICE_FIXTURE_CANNOT_CERTIFY_BASE_SALES_WITHDRAWAL');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'sales',packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Actual isolated Sales notice purchase',evidence:'test:sales-notification-source-authority'}});}

it('requires active purchased preparation and binds the original queued authority to the immutable notice',async()=>{
 const source=await delivered();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await noticeSales();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());const command=key(),input={expectedVersion:1,note:'Original approved customer delivery message'},queued=await queueDeliveryNotification(f.owner,source.fulfillmentId,input,command);
 expect(await db.prepare('SELECT actor_id,assignment_id,kind,notification_version FROM sales_notification_execution_authorizations WHERE command_id=?').bind(command).first()).toEqual({actor_id:f.owner.userId,assignment_id:f.owner.assignmentId,kind:'queue',notification_version:1});
 await noticeSales('draining');await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect(await queueDeliveryNotification(f.owner,source.fulfillmentId,input,command)).toEqual(queued);
 expect(await db.prepare('SELECT status,attempts,provider_message_id FROM commerce_customer_notifications WHERE id=?').bind(queued.id).first()).toEqual({status:'pending',attempts:0,provider_message_id:null});expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(['sales']);
 await updateCustomerNotificationSettings(f.head,{expectedVersion:1,adapter:'disabled'},key());await expect(updateCustomerNotificationSettings(f.head,{expectedVersion:2,adapter:'sandbox'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
});

it('rolls back the original queued notice, authority, receipt and deferred effects when the caller is revoked late',async()=>{
 const source=await delivered();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await noticeSales();await updateCustomerNotificationSettings(f.head,{expectedVersion:0,adapter:'sandbox'},key());const command=key(),name=`sales_notice_authority_${key().replaceAll('-','')}`,input={expectedVersion:1,note:'Original late-authority rollback must retain its exact retry'};await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END;$$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON sales_notification_execution_authorizations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}();`);
 try{await expect(queueDeliveryNotification(f.owner,source.fulfillmentId,input,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${name} ON sales_notification_execution_authorizations;DROP FUNCTION ${name}();`);}
 expect(await db.prepare('SELECT 1 FROM commerce_customer_notifications WHERE fulfillment_id=?').bind(source.fulfillmentId).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM sales_notification_execution_authorizations WHERE command_id=?').bind(command).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.owner.assignmentId).first('revoked_at')).toBeNull();expect(await queueDeliveryNotification(f.owner,source.fulfillmentId,input,command)).toMatchObject({status:'pending'});
});

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
