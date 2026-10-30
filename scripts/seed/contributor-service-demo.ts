/** Synthetic Service Count grants and participants in the keyed contributor case. */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { getLocalProxy } from '../local/fixtures';
import { packageGrantStore, readCompanyPackageGrants } from '@erpx/server-commercial/public/package-grants';
import { commercialCompanies } from '@erpx/server-organization/public/commercial-companies';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
const root='/workspace/erpx', config=`${root}/.runtime/local/config`, key=process.env.ERPX_DEV_WORKSPACE_KEY??'';
function privateFile(path: string) { if (!existsSync(path) || (statSync(path).mode & 0o077) !== 0) throw Error('CONTRIBUTOR_PRIVATE_FILE_REQUIRED'); return readFileSync(path, 'utf8').trim(); }
function assertFixture() {
  if (process.platform !== 'linux' || !existsSync('/.dockerenv') || resolve(process.cwd()) !== root || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(key) || privateFile('/run/erpx-secrets/workspace-key') !== key || !statSync(`${root}/.git`).isDirectory() || !statSync('/run/erpx-contributor-bootstrap.lock').isDirectory()) throw Error('CONTRIBUTOR_WORKSPACE_REQUIRED');
  if (readdirSync(root).some(name => (name === '.env' || name.startsWith('.env.')) && name !== '.env.example') || process.env.ERPX_ENVIRONMENT !== 'local' || process.env.NODE_ENV === 'production' || ![`${config}/databases.json`, `${config}/databases-jobs.json`].includes(process.env.ERPX_DATABASE_MANIFEST ?? '')) throw Error('CONTRIBUTOR_ENVIRONMENT_REQUIRED');
  const sourceId = privateFile(`${root}/.git/erpx-container-source-id`), phase = JSON.parse(privateFile(`${config}/bootstrap-phase.json`));
  if (!/^[a-f0-9]{64}$/.test(sourceId) || phase.key !== key || phase.sourceId !== sourceId || !['seed-pending','complete'].includes(phase.phase)) throw Error('CONTRIBUTOR_SOURCE_PHASE_REQUIRED');
  const roles = JSON.parse(privateFile(`${config}/provisioning.json`));
  for (const [file, role] of [['databases.json','app'], ['databases-jobs.json','jobs']] as const) {
    const manifest = JSON.parse(privateFile(`${config}/${file}`));
    const url = new URL(manifest.databases.H001_DB), username = `erpx_dev_h001_write_${role}`;
    if (!['postgres:','postgresql:'].includes(url.protocol) || url.hostname !== '127.0.0.1' || url.port !== '5432' || url.pathname !== `/${existsSync(`${config}/native-leases.json`) ? (JSON.parse(privateFile(`${config}/native-leases.json`)).entries as Array<{binding:string;newName:string}>).find(entry => entry.binding === 'H001_DB')?.newName : 'erpx_dev_h001_write'}` || url.username !== username || url.search || url.hash || !/^[a-f0-9]{64}$/.test(roles[username]) || url.password !== roles[username]) throw Error('CONTRIBUTOR_DATABASE_REQUIRED');
  }
}
assertFixture();
const fixture=await getLocalProxy();
try {
 const db=fixture.env.H001_DB as import('@erpx/server-runtime/public/runtime/postgres').PostgresDatabase;
 const store=packageGrantStore(db,{authorize:async(tx:SqlDatabase,company:string)=>{assertFixture();if(company!=='acme-co'||!(await commercialCompanies(tx)).some(c=>c.id===company&&c.slug==='acme-co'))throw Error('CONTRIBUTOR_COMPANY_REQUIRED');return {userId:'contributor-synthetic-seed'};},obligations:async()=>{throw Error('CONTRIBUTOR_WITHDRAWAL_FORBIDDEN');}});
 for(const packageId of ['service','service-parts-assets'] as const){
  const grant={companyId:'acme-co',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Synthetic contributor Service demonstration',evidence:'fixture:contributor-service-v1'};
  const id=`contributor_${packageId}_v1`, before=(await readCompanyPackageGrants(db,'acme-co')).find(g=>g.packageId===packageId);
  const matches=(value:unknown)=>!!value && typeof value==='object' && Object.keys(grant).every(k=>(value as Record<string,unknown>)[k]===grant[k as keyof typeof grant]);
  if(before && !matches(before)) throw Error('CONTRIBUTOR_SERVICE_GRANT_CONFLICT');
  const result=await store.apply({idempotencyKey:id,expectedVersion:0,grant});
  if(!matches(result))throw Error('CONTRIBUTOR_SERVICE_GRANT_CONFLICT');
 }
 const definitions=[
  {id:'contributor_files_maker',name:'Synthetic Field Files maker',user:'usr_mali',modules:['field_service'],permissions:['records.read','records.write','module.field_service.read'],scopes:['holding']},
  {id:'contributor_count_maker',name:'Synthetic Count maker',user:'usr_mali',modules:['inventory','field_service'],permissions:['records.read','inventory.read','inventory.prepare','workflow.warehouse_manager','organization.manage','module.inventory.read','module.field_service.read'],scopes:['holding','branch']},
  {id:'contributor_count_reviewer',name:'Synthetic Count reviewer',user:'usr_nisa',modules:['finance','inventory','field_service'],permissions:['finance.read','finance.write','finance.approve','inventory.read','inventory.approve','workflow.finance_approver','module.finance.read','module.inventory.read','module.field_service.read','records.read'],scopes:['holding']},
 ];
 for(const d of definitions){
  const role=await db.prepare('SELECT company_id,id,name,built_in,published_revision,status FROM iam_roles WHERE company_id=? AND id=?').bind('*',d.id).first<Record<string,unknown>>();
  if(role && (role.name!==d.name||Number(role.published_revision)!==1||role.status!=='active'||Boolean(role.built_in)))throw Error('CONTRIBUTOR_SERVICE_ROLE_CONFLICT');
  await db.prepare('INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES(?,?,?,false,1,?) ON CONFLICT DO NOTHING').bind('*',d.id,d.name,'active').run();
  const revision=await db.prepare('SELECT permissions_json,grantable_json,scopes_json FROM iam_role_revisions WHERE company_id=? AND role_id=? AND revision=1').bind('*',d.id).first<Record<string,unknown>>();
  const permissions=JSON.stringify(d.permissions), scopes=JSON.stringify(d.scopes);
  const json=(value:unknown)=>JSON.stringify(typeof value==='string'?JSON.parse(value):value);
  if(revision && (json(revision.permissions_json)!==permissions||json(revision.grantable_json)!=='[]'||json(revision.scopes_json)!==scopes))throw Error('CONTRIBUTOR_SERVICE_ROLE_CONFLICT');
  await db.prepare('INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,1,?,?,?,?,0) ON CONFLICT DO NOTHING').bind('*',d.id,permissions,'[]',scopes,'contributor-synthetic-seed').run();
  const people=await db.prepare("SELECT p.id FROM hr_people p WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.company_id=p.company_id AND e.person_id=p.id AND e.status='active')").bind('acme-co',d.user).all<{id:string}>();
  if(people.results.length!==1)throw Error('CONTRIBUTOR_SERVICE_PERSON_REQUIRED');
  const person=people.results[0]!.id;
  const member=await db.prepare('SELECT principal_id,person_id,status,login_policy FROM iam_members WHERE company_id=? AND user_id=?').bind('acme-co',d.user).first<Record<string,unknown>>();
  if(member && (member.principal_id!==`local:${d.user}`||member.person_id!==person||member.status!=='active'||member.login_policy!=='local'))throw Error('CONTRIBUTOR_SERVICE_MEMBER_CONFLICT');
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES(?,?,?,?,'active','local',1) ON CONFLICT DO NOTHING").bind('acme-co',d.user,`local:${d.user}`,person).run();
  const assignment=`${d.id}_assignment`, modules=JSON.stringify(d.modules);
  const existing=await db.prepare('SELECT user_id,role,modules_json,company_id,scope_kind,revoked_at FROM assignments WHERE id=?').bind(assignment).first<Record<string,unknown>>();
  if(existing && (existing.user_id!==d.user||existing.role!==d.id||existing.modules_json!==modules||existing.company_id!=='acme-co'||existing.scope_kind!=='holding'||existing.revoked_at!==null))throw Error('CONTRIBUTOR_SERVICE_ASSIGNMENT_CONFLICT');
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES(?,?,?,?,?,0,?,1,'holding') ON CONFLICT DO NOTHING").bind(assignment,d.user,d.id,modules,'acme-co',4102444800000).run();
 }
 const existingVisit=await db.prepare('SELECT company_id,branch_id,ticket_id,agreement_id,technician_id FROM field_service_orders WHERE id=?').bind('contributor-field-visit').first<Record<string,unknown>>();
 if(existingVisit && (existingVisit.company_id!=='acme-co'||existingVisit.branch_id!=='bangkok'||existingVisit.ticket_id!=='contributor-field-ticket'||existingVisit.agreement_id!=='contributor-field-agreement'||existingVisit.technician_id!=='usr_mali'))throw Error('CONTRIBUTOR_FIELD_VISIT_CONFLICT');
 if(!existingVisit){
  const now=Date.now();
  await db.transaction(async tx=>{
   await tx.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('contributor-field-customer','acme-co','customer','FIELD-DEMO','Synthetic Field customer','THB',?,?)").bind(now,now).run();
   await tx.prepare("INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at) VALUES('contributor-field-agreement','acme-co','bangkok','contributor-field-customer','Synthetic Field agreement','2026-01-01','2099-12-31','THB',10000,'active','usr_mali','usr_nisa',?,2,?,?)").bind(now,now,now).run();
   await tx.prepare("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES('contributor-field-agreement-review','contributor-field-agreement',2,'activate','usr_nisa','Synthetic independent approval','active',?)").bind(now).run();
   await tx.prepare("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,source_kind,service_agreement_id,service_order_id,service_order_version) VALUES('contributor-field-ticket','acme-co','bangkok',NULL,'contributor-field-customer','Synthetic Field visit','Synthetic Field file intake','usr_mali',to_char(to_timestamp(?::bigint/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'usr_mali',?,?,'field_service','contributor-field-agreement','contributor-field-visit',3)").bind(now,now,now).run();
   await tx.prepare("INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,scheduled_at,status,created_by,version,created_at,updated_at) VALUES('contributor-field-visit','acme-co','bangkok','contributor-field-ticket',1,'contributor-field-agreement',2,'Synthetic Field file intake','Synthetic site','usr_mali',?,'in_visit','usr_mali',3,?,?)").bind(now,now,now).run();
   await tx.prepare("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('contributor-field-visit-start','contributor-field-visit',3,'start','usr_mali','Synthetic contributor visit','in_visit','usr_mali',?,'{}',?)").bind(now,now).run();
  });
 }
 console.log('CONTRIBUTOR_SERVICE_DEMO_READY');
} finally { await fixture.dispose(); }
