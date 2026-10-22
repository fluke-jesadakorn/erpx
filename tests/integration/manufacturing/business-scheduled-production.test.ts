import {afterAll,afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {mkdir,rm,writeFile} from 'node:fs/promises';
import {createHash,randomBytes} from 'node:crypto';
import {join,resolve} from 'node:path';
import {Pool} from 'pg';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import {LocalFileStore} from "@erpx/server-runtime/adapters/runtime/filesystem";
import {applyMigrations,env} from "../../support/runtime-env";
import {saveMaster} from "@erpx/server-commerce/public/commerce";
import {qualityWork} from "@erpx/server-quality/public/quality";
import {acknowledgeQualityAlert,drainQualityAlerts,qualityAlertWork} from "@erpx/server-quality/public/quality-alerts";
import {escalateInventoryReservations} from "@erpx/server-inventory/public/inventory-reservation-alerts";
import {deliverCustomerNotification,drainCustomerNotifications,queueDeliveryNotification,updateCustomerNotificationSettings} from "@erpx/server-sales/public/sales-customer-notifications";
import {enrollEngineeringVerification} from "@erpx/server-platform_administration/public/engineering-verification";
import {filterWork} from "@erpx/feature-work_projection/lib/work-item";
import {mergeWorkItems} from "@erpx/server-work_projection/public/work-items";
import {seedBusinessWorkerSources} from "../../../fixtures/shared/builders/business-worker-sources";

const source='e'.repeat(64),version=`release-${source}`,run=randomBytes(6).toString('hex'),directory=resolve('.runtime/browser',run),databaseName=`erpx_browser_${run}_h001_db`,key=()=>crypto.randomUUID();
vi.mock("@erpx/catalogue/release-artifact.json",()=>({default:{softwareVersion:`release-${'e'.repeat(64)}`}}));
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files}),runtimeEnv:async()=>({...env,H001_DB:db,H001_FILES:files})}));
let db:PostgresDatabase,files:LocalFileStore,admin:Pool,manifestSha:string;
let enabled:Awaited<ReturnType<typeof sources>>,disabled:Awaited<ReturnType<typeof sources>>,now:number;
beforeAll(async()=>{
 const url=new URL(process.env.ERPX_TEST_ADMIN_URL!);if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw new Error('LOCAL_TEST_ADMIN_REQUIRED');
 admin=new Pool({connectionString:url.href,max:1});await admin.query(`CREATE DATABASE "${databaseName}" TEMPLATE template0`);url.pathname=databaseName;db=new PostgresDatabase({connectionString:url.href,max:4});
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);files=new LocalFileStore(join(directory,'files/h001'),db);await mkdir(join(directory,'release'),{recursive:true});
 const manifest=JSON.stringify({'server.js':'f'.repeat(64)});manifestSha=createHash('sha256').update(manifest).digest('hex');
 await writeFile(join(directory,'release/release-files.json'),manifest);await writeFile(join(directory,'release/release.json'),JSON.stringify({sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
 await writeFile(join(directory,'engineering-attestation.json'),JSON.stringify({scope:'isolated-engineering',run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha}));
});
afterEach(()=>vi.unstubAllEnvs());
afterAll(async()=>{
 await db?.close();if(admin){const row=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[databaseName])).rows[0];if(row?.owner!=='erpx_test_admin')throw new Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${databaseName}"`);await admin.end();}
 await rm(directory,{recursive:true,force:true});
});
beforeEach(async()=>{
 vi.stubEnv('NODE_ENV','development');now=Date.now();
 enabled=await sources();disabled=await sources();
 await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE run_id=?').bind(run).run();
 await enrollEngineeringVerification(db,{run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha,companyIds:[enabled.f.company],disabledCompanyIds:[disabled.f.company]});
 vi.stubEnv('NODE_ENV','production');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('PORT','3104');vi.stubEnv('ERPX_BROWSER_SIMULATION_PORT','3104');vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN',run);vi.stubEnv('ERPX_DATA_DIR',directory);
});
async function sources(adapter:'sandbox'|'resend'='sandbox'){return seedBusinessWorkerSources(db,files,{now,label:'Native production-mode isolated fixture',proof:'declared-source-fixture',nativeOpening:true,adapter});}
async function counts(s:typeof enabled){return {alerts:Number(await db.prepare('SELECT count(*) n FROM inventory_quality_alerts WHERE hold_id IN (?,?)').bind(...s.holdIds).first('n')),notifications:Number(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE company_id=? AND (record_type=\'quality_alert\' OR message LIKE \'Stock reservation%\')').bind(s.f.company).first('n')),mail:await db.prepare('SELECT status,attempts,provider_message_id FROM commerce_customer_notifications WHERE id=?').bind(s.messageId).first()};}
async function stockAndDeliveryJournal(s:typeof enabled){return {
 physicalStockMinor:Number(await db.prepare('SELECT COALESCE(SUM(s.value_minor),0) value FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?').bind(s.f.company).first('value')),
 postedInventoryMinor:Number(await db.prepare("SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN commerce_settings c ON c.company_id=j.company_id AND c.inventory_account_id=l.account_id WHERE j.company_id=? AND j.status IN ('posted','reversed')").bind(s.f.company).first('value')),
 journal:await db.prepare('SELECT j.status,j.version,j.source_type,j.source_id,j.created_by,j.posted_by FROM commerce_fulfillments d JOIN commerce_orders o ON o.id=d.order_id JOIN finance_journals j ON j.id=d.journal_id AND j.company_id=o.company_id WHERE d.id=? AND o.company_id=?').bind(s.fulfillmentId,s.f.company).first(),
 movements:(await db.prepare('SELECT id,source_type,source_id,quantity_milli,value_minor,movement_date,created_at FROM inventory_ledger WHERE warehouse_id=? ORDER BY id').bind(s.warehouse).all()).results,
};}

it('blocks nonattested production alert creation and mail claims without consuming native sources',async()=>{
 vi.stubEnv('ERPX_ENGINEERING_VERIFICATION_RUN','');
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:0,overdue:0});expect(await escalateInventoryReservations(db,now)).toEqual({expiring:0,expired:0});
 const send=vi.fn<typeof fetch>();await deliverCustomerNotification(db,{},enabled.messageId,send,now);await drainCustomerNotifications(db,{},now);expect(send).not.toHaveBeenCalled();
 expect(await counts(enabled)).toEqual({alerts:0,notifications:0,mail:{status:'pending',attempts:0,provider_message_id:null}});
 await expect(updateCustomerNotificationSettings(enabled.reviewer,{expectedVersion:1,adapter:'resend'},key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
});
it('delivers only the attested Company Quality and all three reservation sources once while disabled sources and certification remain unchanged',async()=>{
 const stockBefore=await Promise.all([enabled,disabled].map(stockAndDeliveryJournal));
 for(const [index,s] of [enabled,disabled].entries()){
  expect(stockBefore[index]).toMatchObject({physicalStockMinor:5400,postedInventoryMinor:5400,journal:{status:'posted',version:2,source_type:'delivery',source_id:s.fulfillmentId,created_by:s.f.owner.userId,posted_by:s.reviewer.userId}});
  expect(s.reviewer.userId).not.toBe(s.f.owner.userId);
  expect(stockBefore[index].movements).toHaveLength(3);
 }
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:1,overdue:1});expect(await escalateInventoryReservations(db,now)).toEqual({expiring:5,expired:0});await drainCustomerNotifications(db,{},now+10000);
 const reservations=(await db.prepare("SELECT record_type,record_id,user_id FROM staff_notifications WHERE company_id=? AND message LIKE 'Stock reservation%' ORDER BY record_type,user_id").bind(enabled.f.company).all<{record_type:string;record_id:string;user_id:string}>()).results;
 expect(reservations).toHaveLength(5);expect(reservations.filter(row=>row.record_type==='field_service_order').map(row=>row.user_id).sort()).toEqual([enabled.f.owner.userId,enabled.f.claimant.userId].sort());
 expect(reservations.filter(row=>row.record_type==='commerce_order').map(row=>row.user_id)).toEqual([enabled.f.owner.userId]);
 expect(reservations.filter(row=>row.record_type==='manufacturing_order').map(row=>row.user_id).sort()).toEqual([enabled.reviewer.userId,enabled.f.owner.userId].sort());
 expect(reservations.map(row=>row.record_id)).toEqual(expect.arrayContaining([enabled.salesId,enabled.fieldId,enabled.productionId]));
 expect((await counts(enabled)).mail).toEqual({status:'sent',attempts:1,provider_message_id:`sandbox-${enabled.messageId}`});
 expect(await counts(disabled)).toEqual({alerts:0,notifications:0,mail:{status:'pending',attempts:0,provider_message_id:null}});
 const before=await counts(enabled);expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:0,overdue:0});expect(await escalateInventoryReservations(db,now)).toEqual({expiring:0,expired:0});await drainCustomerNotifications(db,{},now+10000);expect(await counts(enabled)).toEqual(before);
 await expect(queueDeliveryNotification(disabled.f.owner,disabled.fulfillmentId,{expectedVersion:1,note:'Disabled Company cannot replay delivery authority'},key())).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
 expect(await db.prepare('SELECT count(*) n FROM company_release_acceptances').first('n')).toBe(0);
 expect(await Promise.all([enabled,disabled].map(stockAndDeliveryJournal))).toEqual(stockBefore);
});
it('rechecks Company withdrawal and expiry before each worker write and resumes the preserved native sources without duplicates',async()=>{
 await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind(enabled.f.company,run).run();
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:0,overdue:0});expect(await escalateInventoryReservations(db,now)).toEqual({expiring:0,expired:0});await drainCustomerNotifications(db,{},now+10000);
 expect(await counts(enabled)).toEqual({alerts:0,notifications:0,mail:{status:'pending',attempts:0,provider_message_id:null}});
 await db.prepare('UPDATE engineering_verification_contexts SET enabled=true WHERE company_id=? AND run_id=?').bind(enabled.f.company,run).run();
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:1,overdue:1});expect(await escalateInventoryReservations(db,now)).toEqual({expiring:5,expired:0});await drainCustomerNotifications(db,{},now+10000);
 const before=await counts(enabled);await db.prepare('UPDATE engineering_verification_contexts SET expires_at=? WHERE company_id=? AND run_id=?').bind(Date.now()-1,enabled.f.company,run).run();
 expect(await drainQualityAlerts(db,now+2*86400000)).toEqual({dueSoon:0,overdue:0});expect(await escalateInventoryReservations(db,now+2*86400000)).toEqual({expiring:0,expired:0});expect(await counts(enabled)).toEqual(before);
});
it('holds an in-flight provider result after Company withdrawal and retries the same immutable provider key only when authority returns',async()=>{
 vi.stubEnv('NODE_ENV','development');const extra=await sources('resend');
 await enrollEngineeringVerification(db,{run,sourceFingerprint:source,softwareVersion:version,fileManifestSha256:manifestSha,companyIds:[extra.f.company],disabledCompanyIds:[]});
 vi.stubEnv('NODE_ENV','production');
 const original=await db.prepare('SELECT recipient_email,subject,body_text,adapter FROM commerce_customer_notifications WHERE id=?').bind(extra.messageId).first();
 const send=vi.fn<typeof fetch>().mockImplementationOnce(async()=>{await db.prepare('UPDATE engineering_verification_contexts SET enabled=false WHERE company_id=? AND run_id=?').bind(extra.f.company,run).run();return Response.json({id:'declared-provider-id'});}).mockImplementation(async()=>Response.json({id:'declared-provider-id'}));
 const provider={RESEND_API_KEY:'declared-fixture-key',RESEND_FROM:'sender@engineering.invalid'};
 await deliverCustomerNotification(db,provider,extra.messageId,send,now+10000);expect(send).toHaveBeenCalledTimes(1);
 expect((await counts(extra)).mail).toEqual({status:'sending',attempts:1,provider_message_id:null});
 await deliverCustomerNotification(db,provider,extra.messageId,send,now+100001);expect(send).toHaveBeenCalledTimes(1);
 await db.prepare('UPDATE engineering_verification_contexts SET enabled=true WHERE company_id=? AND run_id=?').bind(extra.f.company,run).run();
 await deliverCustomerNotification(db,provider,extra.messageId,send,now+100001);expect(send).toHaveBeenCalledTimes(2);
 for(const call of send.mock.calls)expect(call[1]?.headers).toMatchObject({'Idempotency-Key':extra.messageId});
 expect(send.mock.calls[0][1]?.body).toEqual(send.mock.calls[1][1]?.body);
 expect((await counts(extra)).mail).toEqual({status:'sent',attempts:2,provider_message_id:'declared-provider-id'});
 await deliverCustomerNotification(db,provider,extra.messageId,send,now+100002);expect(send).toHaveBeenCalledTimes(2);
 expect(await db.prepare('SELECT recipient_email,subject,body_text,adapter FROM commerce_customer_notifications WHERE id=?').bind(extra.messageId).first()).toEqual(original);
});
it('cancels a production message after native customer consent revocation without sending or rewriting its retained source snapshot',async()=>{
 const original=await db.prepare('SELECT recipient_email,subject,body_text,adapter FROM commerce_customer_notifications WHERE id=?').bind(enabled.messageId).first();
 await saveMaster(enabled.f.owner,'parties',enabled.party,{kind:'customer',code:key(),name:'Declared notified customer',notificationEmail:'receiver@engineering.invalid',notificationConsent:'revoked',expectedVersion:1},key());
 const send=vi.fn<typeof fetch>();await deliverCustomerNotification(db,{},enabled.messageId,send,now+10000);expect(send).not.toHaveBeenCalled();
 expect(await db.prepare('SELECT status,attempts,last_error FROM commerce_customer_notifications WHERE id=?').bind(enabled.messageId).first()).toEqual({status:'cancelled',attempts:1,last_error:'CUSTOMER_CONSENT_OR_EMAIL_CHANGED'});
 expect(await db.prepare('SELECT recipient_email,subject,body_text,adapter FROM commerce_customer_notifications WHERE id=?').bind(enabled.messageId).first()).toEqual(original);
 await drainCustomerNotifications(db,{},now+10000);expect((await counts(disabled)).mail).toEqual({status:'pending',attempts:0,provider_message_id:null});
});
it('rechecks live Quality recipients and cached authority after revocation and recovers missing recipients without duplicating alerts',async()=>{
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id IN (?,?)').bind(Date.now(),enabled.reviewer.assignmentId,enabled.qualityReviewer.assignmentId).run();
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:1,overdue:1});
 expect((await counts(enabled)).notifications).toBe(0);
 const row=filterWork(mergeWorkItems([...(await qualityWork(enabled.f.owner)),...(await qualityAlertWork(enabled.f.owner))]),'all','quality_alert','').find(item=>item.id===enabled.holdIds[0]);
 expect(row?.blocker).toContain('No eligible independent recipient');
 const alertId=`quality-alert-due-soon-${enabled.holdIds[0]}`;
 await expect(acknowledgeQualityAlert(enabled.qualityReviewer,alertId,{expectedVersion:1,reason:'Cached reviewer grant was withdrawn before this command'},key())).rejects.toMatchObject({code:'CONFLICT'});
 const replacement={...enabled.qualityReviewer,assignmentId:await enabled.f.grant(enabled.qualityReviewer,'quality_manager')};
 expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:0,overdue:0});
 expect((await db.prepare("SELECT record_id,user_id FROM staff_notifications WHERE company_id=? AND record_type='quality_alert'").bind(enabled.f.company).all()).results).toEqual(expect.arrayContaining([{record_id:alertId,user_id:replacement.userId}]));
 const before=await counts(enabled);expect(await drainQualityAlerts(db,now)).toEqual({dueSoon:0,overdue:0});expect(await counts(enabled)).toEqual(before);
});
it('prepares worker sources through native counted inventory and actual ClamAV-bound delivery evidence',async()=>{
 vi.stubEnv('NODE_ENV','development');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_ATTACHMENT_UPLOAD_FREEZE','false');
 vi.stubEnv('ERPX_CLAMD_SOCKET',resolve('.scratch/dev/clamav/clamd.sock'));vi.stubEnv('ERPX_CLAMAV_DATABASE_DIR',resolve('.scratch/dev/clamav/db'));
 const s=await seedBusinessWorkerSources(db,files,{now,label:'Actual scanned isolated worker source',proof:'clamav',nativeOpening:true});
 expect(s.openingCounts).toHaveLength(2);expect(s.evidence?.engine).toContain('ClamAV');
 expect(await db.prepare("SELECT a.source_state,a.checksum_sha256,l.state,l.binding_type,l.binding_id FROM attachments a JOIN attachment_links l ON l.attachment_id=a.id WHERE a.id=?").bind(s.evidence!.sourceId).first()).toEqual({source_state:'available',checksum_sha256:s.evidence!.sha256,state:'bound',binding_type:'commerce_fulfillment',binding_id:s.fulfillmentId});
 for(const id of s.openingCounts)expect(await db.prepare("SELECT c.status,j.status journal_status FROM inventory_counts c JOIN finance_journals j ON j.id=c.journal_id WHERE c.id=?").bind(id).first()).toEqual({status:'posted',journal_status:'posted'});
 expect(await stockAndDeliveryJournal(s)).toMatchObject({physicalStockMinor:5400,postedInventoryMinor:5400,journal:{status:'posted',version:2,source_type:'delivery',source_id:s.fulfillmentId,created_by:s.f.owner.userId,posted_by:s.reviewer.userId}});
 expect((await counts(s)).mail).toEqual({status:'pending',attempts:0,provider_message_id:null});
});
