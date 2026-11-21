/** Temporary same-source oracle; remove under migrate.customer_360 after native-only parity. */
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {readCompanyPurchaseFacts} from '@erpx/server-commercial/adapters/package-grants';
import {purchasedCapability} from '@erpx/server-commercial/domain/packages';
import {purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/adapters/purchase-command-boundary';
import {capabilityRevisionSource} from '@erpx/server-commercial/domain/capability-decision';
async function main() {
 const input=JSON.parse(readFileSync(0,'utf8')) as {database:string;token:string};
 if(process.version!=='v26.11.1'||!/^[0-9a-f]{32}$/.test(input.token)||input.database!==`erpx_dev_rust_${input.token}`)throw Error('EXACT_LEASE_REQUIRED');
 const admin=new URL(process.env.ERPX_TEST_ADMIN_URL??'');
 if(admin.hostname!=='127.0.0.1'||admin.port!=='5432'||admin.username!=='erpx_test_admin'||admin.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 admin.pathname=`/${input.database}`;const db=new PostgresDatabase({connectionString:admin.href,max:1});
 try {
  const id=await db.prepare("SELECT current_database()::text database,current_user::text role,pg_get_userbyid(d.datdba)::text owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreaterole FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()").first<{database:string;role:string;owner:string;lease:string;rolsuper:boolean;rolcreaterole:boolean}>();
  if(!id||id.database!==input.database||id.role!=='erpx_test_admin'||id.owner!=='erpx_test_admin'||id.lease!==`erpx-rust-test:${input.token}`||id.rolsuper||id.rolcreaterole)throw Error('EXACT_LEASE_REQUIRED');
  if ((input as {kind?:string}).kind==='enforced' || (input as {kind?:string}).kind==='before') {
   const x=input as unknown as {company:string;purpose:'history'|'settle-existing'};
   const boundary=purchaseCommandBoundary(db,'enforced','oracle-enforced',{companyId:x.company,capability:'sales.portal',purpose:x.purpose})!;
   try {await db.batch([...( (input as {kind?:string}).kind==='before' ? boundary.before : boundary.after),db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind('oracle-enforced')]);process.stdout.write(JSON.stringify({ok:true}));}
   catch(error) {const mapped=purchaseCommandFailure(error);process.stdout.write(JSON.stringify({ok:false,code:mapped?.code??null,status:mapped?.status??null}));}
   return;
  }
  if ((input as {kind?:string}).kind==='after') {
   const scope={companyId:'company',capability:'sales.portal',purpose:'history' as const};
   const boundary=purchaseCommandBoundary(db,'shadow','oracle-after',scope)!;
   const results=await db.batch([...boundary.after,db.prepare('DELETE FROM command_assertions WHERE command_id=?').bind('oracle-after')]);
   let line='';const original=console.info;console.info=(value:unknown)=>{line=String(value)};try{boundary.observe?.(results);}finally{console.info=original;}
   process.stdout.write(line);return;
  }
  const facts=await readCompanyPurchaseFacts(db,'company');
  const cases=[['sales.portal','settle-existing',1000],['sales.portal','history',1000],['unknown.capability','settle-existing',1000]] as const;
  process.stdout.write(JSON.stringify(cases.map(([capability,purpose,now])=>({event:'commercial.purchase.shadow',phase:'read',companyId:'company',capability,purpose,code:purchasedCapability(facts.grants,'company',capability,purpose,now,facts.history).code,purchaseRevision:createHash('sha256').update(capabilityRevisionSource('company',facts.grants,now,facts.history)).digest('hex')}))));
 } finally {await db.close();}
}
main().catch(()=>{console.error('PORTAL_PURCHASE_ORACLE_FAILED');process.exitCode=1;});
