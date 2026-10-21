import {lstat,readFile,realpath} from 'node:fs/promises';
import {resolve} from 'node:path';
import {Pool} from 'pg';
import {readProductHostLease} from '../../../scripts/testing/product-host-cohort';
import {decryptSensitiveSandboxEmailBody,type EncryptedSandboxEmail} from '../sandbox-email';

const accountOrigin='https://account-dev-erpx.jesadakorn.com',legalPortalOrigin='https://legal-portal-dev-erpx.jesadakorn.com';
type InvitationFixture={companyId:string;holdingSlug:'simulation';recordId:string;documentId:string;ownerUserId:string;ownerEmail:string;ownerAssignmentId:string;ownerAssignmentVersion:number};
type SimulationState={run:string;prefix:string;sourceFingerprint:string;release:{id:string;fileManifestSha256:string};fixture:{legal?:{invitationEnrollment?:InvitationFixture}}};
type Credentials={run:string;password:string;users:{key:string;name:string;email:string;userId:string;role:string;companyId:string;holdingSlug:string;assignmentId:string;scope:string}[]};
type DatabaseManifest={databases:{CONTROL_DB?:string;H001_DB?:string}};
export type LegalInvitationBrowserFixture={run:string;directory:string;state:SimulationState;fixture:InvitationFixture;password:string;controlUrl:string;h001Url:string};

async function privateBytes(path:string){
 const stat=await lstat(path);if(!stat.isFile()||stat.isSymbolicLink()||(stat.mode&0o077)!==0||await realpath(path)!==path)throw Error('LEGAL_INVITATION_PRIVATE_FIXTURE_REQUIRED');
 return readFile(path);
}
function exactDatabase(value:unknown,run:string,name:'CONTROL_DB'|'H001_DB'){
 if(typeof value!=='string')throw Error('LEGAL_INVITATION_DATABASE_BINDING_REQUIRED');
 const url=new URL(value);if(url.protocol!=='postgres:'||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${name.toLowerCase()}`)throw Error('LEGAL_INVITATION_DATABASE_SCOPE_INVALID');
 return url.href;
}
export async function legalInvitationBrowserFixture():Promise<LegalInvitationBrowserFixture>{
 const run=process.env.ERPX_DEMO_RUN,leasePath=process.env.ERPX_PRODUCT_HOST_LEASE;if(!run||!/^[a-f0-9]{12}$/.test(run)||!leasePath)throw Error('EXACT_LEGAL_INVITATION_PRODUCT_HOST_REQUIRED');
 const lease=await readProductHostLease(process.cwd(),leasePath);if(lease.run!==run||lease.scopedBusinessWorkflow!=='legal.invitation-enrollment')throw Error('LEGAL_INVITATION_PRODUCT_HOST_SCOPE_REQUIRED');
 const directory=resolve(process.cwd(),'.runtime/browser',run),[stateText,credentialText,fixtureText,manifestText]=await Promise.all(['state.json','legal-credentials.json','legal-invitation-fixture.json','databases.json'].map(name=>privateBytes(resolve(directory,name))));
 const state=JSON.parse(stateText.toString('utf8')) as SimulationState,credentials=JSON.parse(credentialText.toString('utf8')) as Credentials,fixtureFile=JSON.parse(fixtureText.toString('utf8')) as {run:string;invitationEnrollment:InvitationFixture},manifest=JSON.parse(manifestText.toString('utf8')) as DatabaseManifest,fixture=state.fixture?.legal?.invitationEnrollment;
 if(state.run!==run||state.prefix!==`erpx_browser_${run}`||state.sourceFingerprint!==lease.sourceFingerprint||JSON.stringify(state.release)!==JSON.stringify(lease.release)||credentials.run!==run||fixtureFile.run!==run||!fixture||JSON.stringify(fixture)!==JSON.stringify(fixtureFile.invitationEnrollment)||credentials.password.length<12)throw Error('LEGAL_INVITATION_FIXTURE_BINDING_REQUIRED');
 const owner=credentials.users.filter(user=>user.key==='invitation-owner');if(owner.length!==1||owner[0]!.userId!==fixture.ownerUserId||owner[0]!.companyId!==fixture.companyId||owner[0]!.assignmentId!==fixture.ownerAssignmentId||owner[0]!.email!==fixture.ownerEmail||owner[0]!.role!=='legal_manager'||fixture.ownerAssignmentVersion<1)throw Error('LEGAL_INVITATION_OWNER_FIXTURE_REQUIRED');
 return{run,directory,state,fixture,password:credentials.password,controlUrl:exactDatabase(manifest.databases.CONTROL_DB,run,'CONTROL_DB'),h001Url:exactDatabase(manifest.databases.H001_DB,run,'H001_DB')};
}
export type LegalShareRow={id:string;item_id:string;document_id:string;email:string;user_id:string|null;expires_at:number|string;revoked_at:number|string|null};
export async function legalInvitationShare(loaded:LegalInvitationBrowserFixture,shareId:string){
 const db=new Pool({connectionString:loaded.h001Url,max:1});try{
  const row=(await db.query<LegalShareRow>('SELECT id,item_id,document_id,email,user_id,expires_at,revoked_at FROM legal_shares WHERE id=$1 AND item_id=$2 AND document_id=$3',[shareId,loaded.fixture.recordId,loaded.fixture.documentId])).rows[0];
  if(!row||row.id!==shareId||row.email!==`legal-invite-${loaded.run}@example.test`)throw Error('LEGAL_INVITATION_RUN_SHARE_REQUIRED');return row;
 }finally{await db.end();}
}
export async function legalInvitationShareForRecipient(loaded:LegalInvitationBrowserFixture){
 for(let attempt=0;attempt<80;attempt++){
  const db=new Pool({connectionString:loaded.h001Url,max:1});try{
   const rows=(await db.query<LegalShareRow>('SELECT id,item_id,document_id,email,user_id,expires_at,revoked_at FROM legal_shares WHERE item_id=$1 AND document_id=$2 AND lower(email)=lower($3) ORDER BY created_at DESC',[loaded.fixture.recordId,loaded.fixture.documentId,`legal-invite-${loaded.run}@example.test`])).rows;
   if(rows.length>1)throw Error('EXACT_LEGAL_INVITATION_SHARE_REQUIRED');
   if(rows.length===1){if(rows[0]!.item_id!==loaded.fixture.recordId||rows[0]!.document_id!==loaded.fixture.documentId)throw Error('EXACT_LEGAL_INVITATION_SHARE_REQUIRED');return rows[0]!;}
  }finally{await db.end();}
  await new Promise(resolve=>setTimeout(resolve,250));
 }
 return null;
}
export async function legalInvitationStaffAssignments(loaded:LegalInvitationBrowserFixture,userId:string){
 const db=new Pool({connectionString:loaded.h001Url,max:1});try{
  const row=(await db.query<{count:number}>('SELECT count(*)::int AS count FROM assignments WHERE user_id=$1',[userId])).rows[0];if(!row||!Number.isSafeInteger(row.count))throw Error('LEGAL_INVITATION_ASSIGNMENT_STATE_REQUIRED');return row.count;
 }finally{await db.end();}
}
async function secret(loaded:LegalInvitationBrowserFixture){
 const runtime=JSON.parse((await privateBytes(resolve(loaded.directory,'runtime.json'))).toString('utf8')) as {secret?:unknown};if(typeof runtime.secret!=='string'||runtime.secret.length<32)throw Error('LEGAL_INVITATION_EMAIL_SECRET_REQUIRED');return runtime.secret;
}
async function controlEmail(loaded:LegalInvitationBrowserFixture,query:{sql:string;values:unknown[]}){
 const db=new Pool({connectionString:loaded.controlUrl,max:1});try{return(await db.query<EncryptedSandboxEmail&{created_at:number}>(query.sql,query.values)).rows;}finally{await db.end();}
}
async function waitEmail(loaded:LegalInvitationBrowserFixture,query:{sql:string;values:unknown[]},code:string){
 for(let attempt=0;attempt<80;attempt++){const rows=await controlEmail(loaded,query);if(rows[0])return rows[0];await new Promise(resolve=>setTimeout(resolve,250));}throw Error(code);
}
function decrypted(secretValue:string,email:EncryptedSandboxEmail,recipient:string,subject:string){
 if(email.recipient!==recipient||email.subject!==subject||email.body_text!=='[redacted sensitive email body]'||!email.body_ciphertext)throw Error('LEGAL_INVITATION_SENSITIVE_EMAIL_REQUIRED');
 try{return decryptSensitiveSandboxEmailBody(secretValue,email);}catch{throw Error('LEGAL_INVITATION_SENSITIVE_EMAIL_INVALID');}
}
/** Returns only exact run-bound links in memory; never attach or log their tokens. */
export async function legalInvitationEmailLinks(loaded:LegalInvitationBrowserFixture,shareId:string){
 const db=new Pool({connectionString:loaded.h001Url,max:1});let jobId:string|undefined;
 try{for(let attempt=0;attempt<80&&!jobId;attempt++){const job=(await db.query<{id:string;state:string;error:string|null}>('SELECT id,state,error FROM legal_invitation_jobs WHERE share_id=$1 ORDER BY created_at DESC LIMIT 1',[shareId])).rows[0];if(job?.state==='sent')jobId=job.id;else if(job&&['failed','cancelled'].includes(job.state))throw Error('LEGAL_INVITATION_JOB_NOT_SENT');else await new Promise(resolve=>setTimeout(resolve,250));}}finally{await db.end();}
 if(!jobId)throw Error('LEGAL_INVITATION_JOB_NOT_SENT');const key=`legal-invitation-${jobId}`,recipient=`legal-invite-${loaded.run}@example.test`,subject='You have an ERPX Legal document invitation';
 const email=await waitEmail(loaded,{sql:'SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id,created_at FROM sandbox_email_deliveries WHERE delivery_key=$1',values:[key]},'LEGAL_INVITATION_EMAIL_NOT_DELIVERED');
 if(email.delivery_key!==key)throw Error('LEGAL_INVITATION_DELIVERY_KEY_MISMATCH');const body=decrypted(await secret(loaded),email,recipient,subject),links=body.match(/https?:\/\/[^\s<>"']+/g)??[];if(links.length!==2)throw Error('LEGAL_INVITATION_EMAIL_LINKS_INVALID');
 const portal=new URL(links[0]!),enrollment=new URL(links[1]!),tokenParams=new URLSearchParams(enrollment.hash.slice(1));
 if(portal.origin!==legalPortalOrigin||portal.pathname!=='/legal-portal/simulation'||[...portal.searchParams.keys()].sort().join(',')!=='share'||portal.searchParams.get('share')!==shareId||portal.hash||enrollment.origin!==accountOrigin||enrollment.pathname!=='/legal-portal/enroll'||enrollment.search||[...tokenParams.keys()].join(',')!=='token'||(tokenParams.get('token')??'').length<20)throw Error('LEGAL_INVITATION_EMAIL_SCOPE_INVALID');
 return{portalUrl:portal.href,enrollmentUrl:enrollment.href};
}
export async function legalVerificationEmailUrl(loaded:LegalInvitationBrowserFixture,shareId:string,emailAddress:string,createdAfter:number){
 const subject='Verify your ERPX email',email=await waitEmail(loaded,{sql:'SELECT delivery_key,recipient,subject,body_text,body_ciphertext,provider_id,created_at FROM sandbox_email_deliveries WHERE recipient=$1 AND subject=$2 AND created_at >= $3 ORDER BY created_at DESC LIMIT 1',values:[emailAddress,subject,createdAfter]},'LEGAL_INVITATION_VERIFICATION_EMAIL_NOT_DELIVERED');
 const body=decrypted(await secret(loaded),email,emailAddress,subject),links=body.match(/https?:\/\/[^\s<>"']+/g)??[];if(links.length!==1)throw Error('LEGAL_INVITATION_VERIFICATION_LINK_INVALID');
 const url=new URL(links[0]!),callback=url.searchParams.get('callbackURL'),expectedCallback=accountOrigin+'/legal-portal/continue?'+new URLSearchParams({holding:'simulation',share:shareId});
 if(url.origin!==accountOrigin||url.pathname!=='/api/auth/verify-email'||[...url.searchParams.keys()].sort().join(',')!=='callbackURL,token'||!url.searchParams.get('token')||callback!==expectedCallback||url.hash)throw Error('LEGAL_INVITATION_VERIFICATION_SCOPE_INVALID');
 return url.href;
}
export async function legalInvitationAccountState(loaded:LegalInvitationBrowserFixture,emailAddress:string){
 const db=new Pool({connectionString:loaded.controlUrl,max:1});try{const user=(await db.query<{id:string;emailVerified:boolean|string|number}>('SELECT id,"emailVerified" FROM "user" WHERE lower(email)=lower($1)',[emailAddress])).rows[0];if(!user)return null;if(![true,false,0,1,'0','1'].includes(user.emailVerified))throw Error('LEGAL_INVITATION_ACCOUNT_STATE_INVALID');return{id:user.id,emailVerified:user.emailVerified===true||user.emailVerified===1||user.emailVerified==='1'};}finally{await db.end();}
}
