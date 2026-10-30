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
 console.log('CONTRIBUTOR_SERVICE_DEMO_READY');
} finally { await fixture.dispose(); }
