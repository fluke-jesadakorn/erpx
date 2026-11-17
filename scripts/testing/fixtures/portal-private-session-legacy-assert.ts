/** Temporary differential oracle. Only a verified, owned Rust test lease may be read. */
import {readFileSync} from 'node:fs';
import {PostgresDatabase,transactionDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {assertAccountPreferenceSession} from '@erpx/server-identity_access/adapters/account-preference-session';

let stage="input";
async function main() {
 const input=JSON.parse(readFileSync(0,'utf8')) as {database:string;token:string;actor:{userId:string;sessionId:string;email:string;audience:string|null};now:number};
 if(process.version!=='v26.11.1'||!/^[0-9a-f]{32}$/.test(input.token)||input.database!==`erpx_dev_rust_${input.token}`)throw Error('EXACT_LEASE_REQUIRED');
 const admin=new URL(process.env.ERPX_TEST_ADMIN_URL??'');
 if(admin.hostname!=='127.0.0.1'||admin.port!=='5432'||admin.username!=='erpx_test_admin'||admin.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 admin.pathname=`/${input.database}`;
 const db=new PostgresDatabase({connectionString:admin.href,max:1});
 try {
  stage="lease-check";
  const identity=await db.prepare("SELECT current_database()::text database,current_user::text role,pg_get_userbyid(d.datdba)::text owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreatedb,r.rolcreaterole FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()").first<{database:string;role:string;owner:string;lease:string;rolsuper:boolean;rolcreatedb:boolean;rolcreaterole:boolean}>();
  if(!identity||identity.database!==input.database||identity.role!=='erpx_test_admin'||identity.owner!=='erpx_test_admin'||identity.lease!==`erpx-rust-test:${input.token}`||identity.rolsuper||!identity.rolcreatedb||identity.rolcreaterole)throw Error('EXACT_LEASE_REQUIRED');
  stage="transaction";
  const client=await db.pool.connect();
  try {
   stage='begin'; await client.query('BEGIN ISOLATION LEVEL READ COMMITTED');await client.query("SET LOCAL lock_timeout='5s'");
   stage='session-lock'; await client.query('LOCK TABLE session IN EXCLUSIVE MODE');stage='sso-lock';await client.query('LOCK TABLE sso_session_bindings IN SHARE MODE');
   const tx=transactionDatabase(client);
   try {
    stage="assertion";
    await assertAccountPreferenceSession(tx,input.actor,()=>input.now);
    const user=await tx.prepare('SELECT email,"emailVerified" verified FROM "user" WHERE id=?').bind(input.actor.userId).first<{email:string;verified:number}>();
    if(!user||!Number(user.verified)||user.email.toLowerCase()!==input.actor.email.toLowerCase()) {process.stdout.write(JSON.stringify({error:{code:'SESSION_REVOKED',status:401}}));}
    else process.stdout.write(JSON.stringify({ok:true}));
   } catch(error) {const e=error as {code?:string;status?:number};process.stdout.write(JSON.stringify({error:{code:e.code??null,status:e.status??null}}));}
   stage="rollback";
   await client.query('ROLLBACK');
  } finally {client.release();}
 } finally {await db.close();}
}
main().catch(()=>{console.error(`PORTAL_PRIVATE_SESSION_ORACLE_FAILED:${stage}`);process.exitCode=1;});
