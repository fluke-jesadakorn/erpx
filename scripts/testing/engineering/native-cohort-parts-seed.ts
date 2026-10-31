// Synthetic leased-database fixture. Never run against an unverified database.
import {randomUUID} from 'node:crypto';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {Pool} from 'pg';
import {readFileSync,statSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import type {WorkContext as ServicePartsFixtureActor} from '@erpx/server-identity_access/public/authorization';
import {packageGrantStore} from '@erpx/server-commercial/public/package-grants';
const key=()=>randomUUID();
export async function seedZeroReferencePartsCompany(db:PostgresDatabase) {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-service-parts-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Service parts fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Service parts team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<ServicePartsFixtureActor>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,?,?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,JSON.stringify(['field_service']),company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:ServicePartsFixtureActor['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'acme',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Service parts team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'acme',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'acme',holdingName:'Test',companyId:company};
    const context:ServicePartsFixtureActor={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'acme',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:['field_service'],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),serviceReviewer=await actor('service-reviewer','holding_owner'),technician=await actor('technician','employee');
  const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[serviceReviewer,3],[technician,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices','commerce_warehouses','commerce_products'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  if((await counts()).some(value=>Number(value)!==0))throw Error('ZERO_REFERENCE_FIXTURE_REQUIRED');
  // Normal packageStore is an explicit isolated provider fixture port, never
  // installed Commercial authority or normal Company onboarding acceptance.
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-parts-provider'}),obligations:async()=>{throw Error('Service parts fixture cannot withdraw');}});
  async function purchase(){for(const packageId of ['service','service-parts-assets'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Declared native Service-only parts startup',evidence:'test:Service-parts-fresh-Company'}});}
  await purchase();
  for(const person of [owner,reviewer,manager,serviceReviewer,technician]){const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');person.iam=await loadIamAuthority(db,company,person.userId,person.role);}
  return {company,branch,department,owner,reviewer,manager,serviceReviewer,technician,counts,purchase,store,actor};
}


if(import.meta.main){
 const configPath=process.argv[2];if(!configPath)throw Error('PRIVATE_SEED_CONFIG_REQUIRED');
 const read=(path:string)=>{const st=statSync(path);if(!st.isFile()||(st.mode&0o077)!==0||st.size>65536)throw Error('PRIVATE_SEED_FILE_REQUIRED');return JSON.parse(readFileSync(path,'utf8'));};
 const c=read(configPath),raw=readFileSync(c.rosterPath);if(createHash('sha256').update(raw).digest('hex')!==c.rosterSha256)throw Error('ROSTER_DIGEST_MISMATCH');const roster=JSON.parse(raw.toString()),seed=read(c.fixtureSeedPath);if(roster.run!==c.run||seed.nonce!==c.nonce||!Array.isArray(roster.entries)||roster.entries.length!==8)throw Error('COHORT_IDENTITY_MISMATCH');
 const entry=roster.entries.find((x:{binding:string})=>x.binding==='H001_DB'),u=new URL(seed.database.url);if(!entry||!Number.isSafeInteger(entry.oid)||!/^[0-9a-f]{32}$/.test(entry.token)||seed.database.leaseToken!==entry.token||u.hostname!=='127.0.0.1'||u.port!=='5432'||u.username!=='erpx_test_admin'||u.pathname!==`/erpx_dev_rust_${entry.token}`||u.search||u.hash)throw Error('H001_LEASE_REQUIRED');
 const pool=new Pool({connectionString:seed.database.url,max:1});const identity=await pool.query("SELECT oid::int8 oid,pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=current_database()");await pool.end();if(identity.rows.length!==1||Number(identity.rows[0].oid)!==entry.oid||identity.rows[0].owner!=='erpx_test_admin'||identity.rows[0].lease!==`erpx-rust-test:${entry.token}`)throw Error('H001_LEASE_IDENTITY_CHANGED');
 const db=new PostgresDatabase({connectionString:seed.database.url,max:4});try{const f=await seedZeroReferencePartsCompany(db);const actors=([['owner',f.owner],['reviewer',f.reviewer],['manager',f.manager],['serviceReviewer',f.serviceReviewer]] as const).map(([label,a])=>({label,userId:a.userId,assignmentId:a.assignmentId,assignmentVersion:1,role:a.role,modules:a.modules,branchId:null,departmentId:null}));writeFileSync(c.output,JSON.stringify({companyId:f.company,branchId:f.branch,actors}),{mode:0o600,flag:'wx'});console.log('OWNED_PARTS_COMPANY_SEEDED');}finally{await db.close();}
}
