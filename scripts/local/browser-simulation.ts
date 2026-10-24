import{workspaceBrowserFixtureProfile,type WorkspaceFixtureProfile}from '../testing/development-host-profile';
import {productIds,type ProductId} from "@erpx/contracts/product-registry";
/** Disposable browser acceptance workspace. Never reuses development/production databases or sessions.
 * ERPX_BROWSER_SIMULATION_PORT=3104 bun run demo:prepare
 * ERPX_BROWSER_SIMULATION_PORT=3104 bun scripts/local/run-server-script.ts scripts/local/browser-simulation.ts prepare-diagnostic
 * ERPX_BROWSER_SIMULATION_PORT=3104 bun run demo:start <run-id>
 * ERPX_BROWSER_SIMULATION_PORT=3104 ERPX_BROWSER_SIMULATION_RELEASE_DIR=<release> bun run demo:start <run-id>
 * ERPX_BROWSER_SIMULATION_PORT=3104 bun run demo:destroy <run-id>
 */
import {mkdir,readFile,writeFile,chmod,open,rm,realpath,readdir} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {resolve,join,dirname} from 'node:path';
import {createHash,randomBytes} from 'node:crypto';
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {Pool} from 'pg';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import {migrateDatabase} from "@erpx/server-runtime/adapters/runtime/migrations";
import {generalCompanyDemoGuide} from "../../fixtures/shared/builders/general-company-demo";
import {placeScopedBrowserEmployee} from "../../fixtures/people_payroll/builders/scoped-browser-staff";
import {runtimeSourceFingerprint} from './source-fingerprint';
import {copyReleaseArtifact} from './copy-release-artifact';
import {workspaceCopyInput,copyWorkspaceSnapshot} from './workspace-copy-input';
import {workspaceBrowserApps,workspaceBrowserBundler} from '../testing/development-host-profile';
import {linkBrowserWorkspaceModules} from '../testing/browser-workspace-modules';
import {clonedFixtureMetadata,isolatedAiCredentialEnvironment,isClonedFixtureDocument,isCaseExecutionEvidence,moduleBrowserReleaseVerificationCompanyIds} from '../testing/browser-fixture-metadata';
import {browserChildEnvironment} from '../testing/runner-environment';
import {cloneSchemaTemplate,ensureSchemaTemplate,postgresFixtureFingerprint,type DatabaseSlot,type Template} from '../../tests/support/postgres-templates';
const root=resolve(import.meta.dir,'../..'),parent=join(root,'.runtime/browser'),port=Number(process.env.ERPX_BROWSER_SIMULATION_PORT??'3102');
if(![3102,3104].includes(port))throw new Error('UNSUPPORTED_BROWSER_SIMULATION_PORT');
const useWebpack=process.env.ERPX_BROWSER_SIMULATION_WEBPACK==='1';
if(useWebpack&&port!==3104)throw new Error('WEBPACK_IS_FOR_UAT_SIMULATION_ONLY');
const origin=`http://127.0.0.1:${port}`;
const node=resolve(process.env.ERPX_NODE_BIN??join(root,'.runtime/node/node-v26.11.1-darwin-arm64/bin/node'));
const profile=process.env.ERPX_BROWSER_SIMULATION_PROFILE??'general-company';
if(!['general-company','hospitality','engineering','legal','uat','local-seed','staff-work','expense','work-cards','messaging','services','executive-work','booking'].includes(profile))throw new Error('UNSUPPORTED_BROWSER_SIMULATION_PROFILE');
if(Bun.version!=='1.4.3'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v26.11.1')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
let adminUrl=process.env.ERPX_TEST_ADMIN_URL;
if(!adminUrl)for(const file of ['.env.test.local','.env.test','.env.local']){const path=join(root,file);if(!existsSync(path))continue;const value=(await readFile(path,'utf8')).match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1];if(value){adminUrl=value.replace(/^(["'])(.*)\1$/,'$2');break;}}
if(!adminUrl)throw new Error('ISOLATED_TEST_ADMIN_REQUIRED');
const connection=new URL(adminUrl);if(connection.hostname!=='127.0.0.1'||connection.port!=='5432'||connection.username!=='erpx_test_admin'||connection.pathname!=='/postgres')throw new Error('UNSAFE_TEST_DATABASE_ADMIN');
const tracks={CONTROL_DB:'control',CONTROL_READ_DB:'control-read',H001_DB:'holding-write',H001_READ_DB:'holding-read',H002_DB:'holding-write',H002_READ_DB:'holding-read',H003_DB:'holding-write',H003_READ_DB:'holding-read'} as const;
const mode=process.argv[2],diagnostic=mode==='prepare-diagnostic',run=mode==='prepare'||diagnostic?(process.env.ERPX_BROWSER_SIMULATION_RUN_ID??randomBytes(6).toString('hex')):process.argv[3];
if(!run||!/^([a-f0-9]{12})$/.test(run))throw new Error('EXACT_BROWSER_RUN_ID_REQUIRED');
const directory=join(parent,run),app=join(directory,'app'),manifestPath=join(directory,'databases.json'),statePath=join(directory,'state.json');
type State={workspaceFixtureProfile?:WorkspaceFixtureProfile;executionMode?:"development-smoke";workspaceApps?:string[];workspaceBundler?:"webpack"|"turbopack";workspaceConsoleRetired?:boolean;run:string;prefix:string;createdAt:string;origin:string;port?:number;profile?:string;providerFixture?:boolean;templateRun?:string;ready?:boolean;diagnostic?:boolean;pid?:number;applicationPoolMax?:number;controlPoolMax?:number;jobsPid?:number|null;jobsStartedAt?:string;jobsPaused?:boolean;sourceFingerprint?:string;softwareVersion?:string;webEntrypoint?:string;jobsEntrypoint?:string;productHosts?:boolean;executiveWorkHosts?:boolean;formsHosts?:boolean;release?:{id:string;fileManifestSha256:string};fixture?:Record<string,unknown>};
const productHosts=process.env.ERPX_BROWSER_SIMULATION_PRODUCT_HOSTS==='1';
if(productHosts&&(port!==3104||!process.env.ERPX_PRODUCT_HOST_PLAN||!process.env.ERPX_PRODUCT_HOST_PLAN_SHA256))throw Error('EXPLICIT_PRODUCT_HOST_ARM_REQUIRED');
const executiveWorkHosts=process.env.ERPX_BROWSER_SIMULATION_EXECUTIVE_WORK_HOSTS==='1';
if(executiveWorkHosts&&(port!==3104||profile!=='executive-work'||productHosts))throw Error('EXACT_EXECUTIVE_WORK_HOST_MODE_REQUIRED');
const formsHosts=process.env.ERPX_BROWSER_SIMULATION_FORMS_HOSTS==='1';
if(formsHosts&&(port!==3104||profile!=='engineering'||!process.env.ERPX_FORMS_HOST_PLAN||!process.env.ERPX_FORMS_HOST_PLAN_SHA256||productHosts||executiveWorkHosts))throw Error('EXACT_FORMS_HOST_ENGINEERING_MODE_REQUIRED');
if(!formsHosts&&(process.env.ERPX_FORMS_HOST_PLAN!==undefined||process.env.ERPX_FORMS_HOST_PLAN_SHA256!==undefined))throw Error('FORMS_HOST_PLAN_REQUIRES_FORMS_MODE');
if([productHosts,executiveWorkHosts,formsHosts].filter(Boolean).length>1)throw Error('BROWSER_HOST_MODES_ARE_EXCLUSIVE');
const workspaceApps=workspaceBrowserApps(process.env.ERPX_BROWSER_WORKSPACE_APPS);
const workspaceBundler=workspaceBrowserBundler(process.env.ERPX_BROWSER_WORKSPACE_BUNDLER);
const workspaceConsoleRetired=process.env.ERPX_BROWSER_WORKSPACE_CONSOLE_RETIRED==='1';
const workspaceFixtureProfile=workspaceBrowserFixtureProfile(process.env.ERPX_BROWSER_WORKSPACE_FIXTURE_PROFILE,workspaceApps,workspaceConsoleRetired);
if(process.env.ERPX_BROWSER_WORKSPACE_CONSOLE_RETIRED!==undefined&&(!workspaceApps||!workspaceConsoleRetired||workspaceApps.includes('compat')))throw new Error('EXPLICIT_DEVELOPMENT_CONSOLE_PROFILE_REQUIRED');
if(!workspaceApps&&process.env.ERPX_BROWSER_WORKSPACE_BUNDLER!==undefined)throw new Error('WORKSPACE_BUNDLER_REQUIRES_WORKSPACE_PROFILE');
if(workspaceApps&&(process.env.ERPX_BROWSER_EXECUTION_MODE!=='development-smoke'||port!==3104||productHosts||executiveWorkHosts||formsHosts||process.env.ERPX_BROWSER_SIMULATION_RELEASE_DIR||process.env.ERPX_PMS_PROVIDER_FIXTURE==='1'))throw new Error('WORKSPACE_BROWSER_IS_DEVELOPMENT_ONLY');
const localAttachmentMode=process.env.ERPX_BROWSER_SIMULATION_ATTACHMENTS==='local';
const localJobsEnabled=process.env.ERPX_BROWSER_SIMULATION_JOBS==='1';
const legalDemoEnabled=process.env.ERPX_BROWSER_SIMULATION_LEGAL==='1';
const menuImagesEnabled=process.env.ERPX_BROWSER_SIMULATION_MENU_IMAGES==='1';
const pmsScaleEnabled=process.env.ERPX_BROWSER_SIMULATION_PMS_SCALE==='1';
const pmsProviderFixtureEnabled=process.env.ERPX_PMS_PROVIDER_FIXTURE==='1';
if(pmsProviderFixtureEnabled&&(profile!=='booking'||port!==3104||pmsScaleEnabled||!localJobsEnabled||!localAttachmentMode))throw new Error('PMS_PROVIDER_FIXTURE_REQUIRES_ISOLATED_BOOKING_SCANNER_JOBS');
if(pmsScaleEnabled&&(profile!=='booking'||port!==3104))throw new Error('PMS_SCALE_REQUIRES_EXACT_BOOKING_RELEASE_PROFILE');
const pmsScalePool=process.env.ERPX_BROWSER_SIMULATION_PMS_POOL_MAX??'4';
if(!['4','8','16'].includes(pmsScalePool)||(!pmsScaleEnabled&&process.env.ERPX_BROWSER_SIMULATION_PMS_POOL_MAX!==undefined))throw new Error('EXACT_PMS_SCALE_POOL_REQUIRED');
const pmsControlPool=process.env.ERPX_BROWSER_SIMULATION_PMS_CONTROL_POOL_MAX??'5';
if(!['5','8'].includes(pmsControlPool)||(!pmsScaleEnabled&&process.env.ERPX_BROWSER_SIMULATION_PMS_CONTROL_POOL_MAX!==undefined))throw new Error('EXACT_PMS_CONTROL_POOL_REQUIRED');
const safeEnv=(secret:string)=>({
 ...Object.fromEntries(Object.entries(process.env).filter(([name])=>name.startsWith('NEXT_PUBLIC_'))),
 PATH:`${dirname(node)}:${process.env.PATH??''}`,
 HOME:process.env.HOME??'',
 TMPDIR:process.env.TMPDIR??'/tmp',
 NODE_ENV:'development' as const,
 ...(workspaceApps?{ERPX_BROWSER_EXECUTION_MODE:'development-smoke',...(workspaceFixtureProfile?{ERPX_BROWSER_WORKSPACE_FIXTURE_PROFILE:workspaceFixtureProfile}:{}),ERPX_BROWSER_WORKSPACE_APPS:JSON.stringify(workspaceApps),ERPX_BROWSER_WORKSPACE_BUNDLER:workspaceBundler,...(workspaceConsoleRetired?{ERPX_BROWSER_WORKSPACE_CONSOLE_RETIRED:'1'}:{})}:{}),
 // Match workspace dev headroom while compiling the complete route inventory.
 NODE_OPTIONS:'--max-old-space-size=8192',
 ERPX_ENVIRONMENT:'local',
 ERPX_BROWSER_SIMULATION_PORT:String(port),
 ERPX_BROWSER_SIMULATION_RUN_ID:run,
 ERPX_BROWSER_SIMULATION:'thai-hotel-v1',
 ERPX_AI_ADAPTER:'deterministic-v1',
 ERPX_PAYROLL_ADAPTER:'synthetic-fixtures-v1',
 ERPX_EMAIL_ADAPTER:'sandbox',
 ...(pmsScaleEnabled?{ERPX_BROWSER_SIMULATION_PMS_SCALE:'1',ERPX_BROWSER_SIMULATION_PMS_POOL_MAX:pmsScalePool,ERPX_BROWSER_SIMULATION_PMS_CONTROL_POOL_MAX:pmsControlPool,ERPX_PMS_RELEASE_DIAGNOSTICS:'1',ERPX_PERF_LOG:'1'}:{}),
 ...(profile==='booking'||workspaceFixtureProfile==='public-hosts'?{RESEND_API_KEY:'deterministic-browser-booking',RESEND_FROM:'Fictional Hotel <hotel@example.test>'}:{}),
 ...(process.env.ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS==='1'&&profile==='engineering'&&[3102,3104].includes(port)?{ERPX_BROWSER_SIMULATION_CUSTOMER_PORTAL_INVITATIONS:'1',RESEND_API_KEY:'isolated-sandbox-no-provider-key',RESEND_FROM:'Fictional <no-reply@example.test>'}:{}),
 ...(pmsProviderFixtureEnabled?{ERPX_PMS_PROVIDER_FIXTURE:'1',PMS_STRIPE_BOOKING_ENABLED:'true',POS_STRIPE_TEST_SECRET_KEY:'sk_test_erpx_fictional_provider',POS_STRIPE_TEST_WEBHOOK_SECRET:'whsec_erpx_fictional_provider'} : {}),
 // Explicit fixture activation for the existing restaurant/hotel pilot.
 // The compiled runtime still checks the enrolled Company and both owners.
 ERPX_POS_ROOM_CHARGE_PILOT_ENABLED:'true',
 ERPX_PUBLIC_ORIGIN:origin,
 ERPX_MARKETING_ORIGIN:`http://dev-erpx.localhost:${port}`,
 ERPX_FRONT_PMS_ORIGIN:`http://dev-pms.localhost:${port}`,
 ERPX_FRONT_POS_ORIGIN:`http://dev-pos.localhost:${port}`,
 BETTER_AUTH_URL:origin,
 BETTER_AUTH_SECRET:secret,
 ...isolatedAiCredentialEnvironment(secret),
 ERPX_DATA_DIR:directory,
 ERPX_DATABASE_MANIFEST:manifestPath,
 ERPX_PUBLIC_BETA:'false',
 NEXT_DEPLOYMENT_ID:`browser-simulation-${run}`,
 NEXT_TELEMETRY_DISABLED:'1',
 ERPX_DEV_PG_POOL_MAX:'2',
 ERPX_APP_PG_POOL_MAX:pmsScaleEnabled?pmsScalePool:'1',
 ERPX_CONTROL_PG_POOL_MAX:pmsScaleEnabled?pmsControlPool:'5',
 ...(localAttachmentMode?{
  ERPX_ATTACHMENTS_V2:'true',
  ERPX_ATTACHMENT_UPLOAD_FREEZE:'false',
  ERPX_CLAMD_SOCKET:process.env.ERPX_CLAMD_SOCKET??join(root,'.scratch/dev/clamav/clamd.sock'),
  ERPX_CLAMAV_DATABASE_DIR:process.env.ERPX_CLAMAV_DATABASE_DIR??join(root,'.scratch/dev/clamav/db'),
  ERPX_CLAMAV_VERSION:process.env.ERPX_CLAMAV_VERSION??'1.5.4',
  ERPX_HEIF_CONVERT:process.env.ERPX_HEIF_CONVERT??'/opt/homebrew/opt/libheif/bin/heif-convert',
  ERPX_HEIF_ENC:process.env.ERPX_HEIF_ENC??'/opt/homebrew/opt/libheif/bin/heif-enc',
  ERPX_PDFTOPPM_BIN:process.env.ERPX_PDFTOPPM_BIN??'/opt/homebrew/bin/pdftoppm',
 }:{}),
});
async function assertAdmin(admin:Pool){const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(role.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw new Error('UNSAFE_TEST_DATABASE_ROLE');const forbidden=await admin.query("SELECT datname FROM pg_database WHERE (datname LIKE 'epsx\\_%' ESCAPE '\\' OR datname IN ('n8n','plf_webapp_dev')) AND has_database_privilege(current_user,datname,'CONNECT')");if(forbidden.rowCount)throw new Error('TEST_ADMIN_CAN_CONNECT_TO_OTHER_PROJECT');}
async function writePrivate(path:string,value:unknown){await writeFile(path,JSON.stringify(value,null,2)+'\n',{mode:0o600});await chmod(path,0o600);}
function assertCohortManifest(value:{version:number;databases:Record<string,string>;slots:Array<{id:string;write:string;read:string;files:string;internal?:boolean}>}){
 const expected=Object.keys(tracks).sort();
 if(value.version!==1||JSON.stringify(Object.keys(value.databases).sort())!==JSON.stringify(expected)||value.slots.length!==3)throw new Error('EXACT_EIGHT_COHORT_BINDINGS_REQUIRED');
 for(const binding of expected){const url=new URL(value.databases[binding]);if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding.toLowerCase()}`)throw new Error('COHORT_DATABASE_IDENTITY_CHANGED');}
 for(const id of ['h001','h002','h003']){const slot=value.slots.filter(item=>item.id===id),prefix=id.toUpperCase();if(slot.length!==1||slot[0].write!==`${prefix}_DB`||slot[0].read!==`${prefix}_READ_DB`||slot[0].files!==`${prefix}_FILES`||id==='h003'&&slot[0].internal!==true)throw new Error('COHORT_SLOT_IDENTITY_CHANGED');}
}
async function freePort(){const server=createServer();await new Promise<void>((ok,no)=>{server.once('error',no);server.listen(port,'127.0.0.1',()=>server.close(error=>error?no(error):ok()));});}
if(mode==='prepare'||diagnostic){
 if(diagnostic&&port!==3104)throw new Error('SEED_DIAGNOSTIC_REQUIRES_ISOLATED_3104');
 const sourceFingerprint=await runtimeSourceFingerprint(root);
 if(process.env.ERPX_SERVER_SCRIPT_SOURCE_FINGERPRINT&&process.env.ERPX_SERVER_SCRIPT_SOURCE_FINGERPRINT!==sourceFingerprint)
  throw new Error('BROWSER_FIXTURE_COMPILED_SOURCE_MISMATCH');
 await freePort();if(existsSync(directory))throw new Error('BROWSER_RUN_ALREADY_EXISTS');await mkdir(directory,{recursive:true,mode:0o700});const prefix=`erpx_browser_${run}`,state:State={...(process.env.ERPX_BROWSER_EXECUTION_MODE==='development-smoke'?{executionMode:'development-smoke' as const}:{}),...(workspaceApps?{workspaceApps,workspaceBundler,workspaceConsoleRetired,workspaceFixtureProfile}:{}),run,prefix,createdAt:new Date().toISOString(),origin,port,profile,sourceFingerprint,...(productHosts?{productHosts:true}:{}),...(executiveWorkHosts?{executiveWorkHosts:true}:{}),...(formsHosts?{productHosts:false,executiveWorkHosts:false,formsHosts:true}:{})};await writePrivate(statePath,state);
 const templateRun=process.env.ERPX_BROWSER_SIMULATION_TEMPLATE_RUN;
 if(templateRun){
  if(!/^[a-f0-9]{12}$/.test(templateRun)||templateRun===run)throw new Error('EXACT_BROWSER_TEMPLATE_RUN_REQUIRED');
  const templateDirectory=join(parent,templateRun),template=JSON.parse(await readFile(join(templateDirectory,'state.json'),'utf8')) as State;
  if((template.workspaceFixtureProfile??null)!==(workspaceFixtureProfile??null)||Boolean(template.workspaceConsoleRetired)!==workspaceConsoleRetired||(template.workspaceBundler??"webpack")!==workspaceBundler||JSON.stringify(template.workspaceApps??null)!==JSON.stringify(workspaceApps)||template.executionMode!==state.executionMode||template.run!==templateRun||template.prefix!==`erpx_browser_${templateRun}`||template.profile!==profile||Boolean(template.productHosts)!==productHosts||Boolean(template.executiveWorkHosts)!==executiveWorkHosts||Boolean(template.formsHosts)!==formsHosts||template.port!==port||template.origin!==origin||!template.ready||template.diagnostic||template.pid||template.jobsPid||template.release||template.sourceFingerprint!==sourceFingerprint||await runtimeSourceFingerprint(join(templateDirectory,'app'))!==sourceFingerprint)throw new Error('FROZEN_BROWSER_TEMPLATE_REQUIRED');
  if(Boolean(template.providerFixture)!==pmsProviderFixtureEnabled)throw new Error('FROZEN_PROVIDER_TEMPLATE_SETTING_REQUIRED');
  const templateManifest=JSON.parse(await readFile(join(templateDirectory,'databases.json'),'utf8')) as {version:number;databases:Record<string,string>;slots:Array<{id:string;write:string;read:string;files:string;internal?:boolean}>};
  for(const [binding,value] of Object.entries(templateManifest.databases)){
   const url=new URL(value);if(!Object.keys(tracks).includes(binding)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${templateRun}_${binding.toLowerCase()}`)throw new Error('BROWSER_TEMPLATE_DATABASE_IDENTITY_CHANGED');
  }
  if(Object.keys(templateManifest.databases).length!==8)throw new Error('EXACT_EIGHT_TEMPLATE_DATABASES_REQUIRED');
  const admin=new Pool({connectionString:adminUrl,max:1}),databases:Record<string,string>={};
  try{
   await assertAdmin(admin);
   for(const binding of Object.keys(tracks)){
    const source=`erpx_browser_${templateRun}_${binding.toLowerCase()}`,name=`${prefix}_${binding.toLowerCase()}`;
    const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[source])).rows[0];
    if(owner?.owner!=='erpx_test_admin')throw new Error('QUIESCENT_OWNED_BROWSER_TEMPLATE_REQUIRED');
    // A just-exited preparation process can still have a backend finishing its
    // socket close. Never terminate it; wait for the owned template to quiesce.
    const until=Date.now()+10_000;
    while((await admin.query('SELECT 1 FROM pg_stat_activity WHERE datname=$1',[source])).rowCount){
     if(Date.now()>=until)throw new Error(`QUIESCENT_OWNED_BROWSER_TEMPLATE_REQUIRED:${binding}`);
     await new Promise(done=>setTimeout(done,100));
    }
    await admin.query(`CREATE DATABASE "${name}" TEMPLATE "${source}"`);
    const url=new URL(adminUrl);url.pathname=name;databases[binding]=url.href;
   }
  }finally{await admin.end();}
  await copyWorkspaceSnapshot(templateDirectory,directory,path=>{
   const relative=path.slice(templateDirectory.length+1);
   return !['state.json','databases.json','databases-jobs.json','databases-migrations.json','server.log','jobs.log','uat-auth','release','provider-fixture.json','provider-fixture.lock'].some(name=>relative===name||relative.startsWith(name+'/'))&&relative!=='app/node_modules'&&!isCaseExecutionEvidence(relative);
  });
  await linkBrowserWorkspaceModules(join(templateDirectory,'app'),app,workspaceBundler);
  for(const entry of await readdir(directory))if(entry.endsWith('.json')&&!entry.startsWith('databases')){
   const path=join(directory,entry),content=JSON.parse(await readFile(path,'utf8'));
   // Only metadata is rebound. Emails, IDs, snapshots and hashes still name
   // the exact seeded records copied by PostgreSQL, including their seed run.
   await writePrivate(path,isClonedFixtureDocument(entry)?clonedFixtureMetadata(content,templateRun,run,templateDirectory,directory):JSON.parse(JSON.stringify(content).replaceAll(templateDirectory,directory)));
  }
  const manifest={version:1,databases,slots:templateManifest.slots};assertCohortManifest(manifest);
  for(const name of ['databases.json','databases-jobs.json','databases-migrations.json'])await writePrivate(join(directory,name),manifest);
 const fixture=JSON.parse(JSON.stringify(template.fixture??{}).replaceAll(templateDirectory,directory));
  if(fixture.engineeringWorkplacePlatform)fixture.engineeringWorkplacePlatform=clonedFixtureMetadata(fixture.engineeringWorkplacePlatform,templateRun,run,templateDirectory,directory);
  Object.assign(state,{templateRun,softwareVersion:template.softwareVersion,fixture});
  if(pmsProviderFixtureEnabled){state.providerFixture=true;await writePrivate(join(directory,'provider-fixture.json'),{schemaVersion:1,keys:{},resources:{},requests:[]});}
  if(await runtimeSourceFingerprint(root)!==sourceFingerprint||await runtimeSourceFingerprint(app)!==sourceFingerprint)throw new Error('BROWSER_PREPARATION_SOURCE_CHANGED');
  state.ready=true;await writePrivate(statePath,state);console.log(JSON.stringify({run,templateRun,origin,statePath,ready:true}));process.exit(0);
 }
 const admin=new Pool({connectionString:adminUrl,max:1}),urls:Record<string,string>={};
 const schemaTemplates=new Map<string,Template>();
 // Compiled acceptance and diagnostic/migration paths retain real migrations.
 if(workspaceApps&&state.executionMode==='development-smoke'&&!diagnostic){
  const fixtureFingerprint=await postgresFixtureFingerprint(root);
  for(const track of new Set(Object.values(tracks)))schemaTemplates.set(track,await ensureSchemaTemplate(root,track,fixtureFingerprint));
 }
 try{await assertAdmin(admin);for(const [binding,track] of Object.entries(tracks)){
  const name=`${prefix}_${binding.toLowerCase()}`,template=schemaTemplates.get(track);
  if(template)await cloneSchemaTemplate(admin,{database:name,slot:binding as DatabaseSlot},template);
  else await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);
  const url=new URL(adminUrl);url.pathname=name;urls[binding]=url.href;
  if(!template){const db=new PostgresDatabase({connectionString:url.href,max:1});try{await migrateDatabase(db,track);}finally{await db.close();}}
 }}finally{await admin.end();}
 await writePrivate(manifestPath,{version:1,databases:urls,slots:[{id:'h001',write:'H001_DB',read:'H001_READ_DB',files:'H001_FILES'},{id:'h002',write:'H002_DB',read:'H002_READ_DB',files:'H002_FILES'},{id:'h003',write:'H003_DB',read:'H003_READ_DB',files:'H003_FILES',internal:true}]});
 // This is the same restricted disposable owner, never a development or
 // production migration identity. The normal Platform command verifies it
 // against the runtime cohort before replaying packaged migrations.
 await writePrivate(join(directory,'databases-migrations.json'),JSON.parse(await readFile(manifestPath,'utf8')));
 await writePrivate(join(directory,'databases-jobs.json'),JSON.parse(await readFile(manifestPath,'utf8')));
 const secret=randomBytes(32).toString('hex');await writePrivate(join(directory,'runtime.json'),{secret});
 // Bun loads the workspace dotenv before this launcher. Object.assign alone
 // leaves omitted deployment credentials behind; native seed commands must
 // use this cohort's private local files and explicitly declared adapters.
 const nativeEnvironment=browserChildEnvironment(process.env,safeEnv(secret));
 // Reuse the unchanged native compatibility fixture constructor, without inherited installed modes.
 // This seed is not purchased-product proof; the new compiled runtime below is enforced.
 if(productHosts)for(const name of Object.keys(nativeEnvironment))if(/^ERPX_(?:PRODUCT_(?:ROUTING|SSO|ENVIRONMENT)|CONSOLE_RETIRED|[A-Z_]+_PACKAGE_ENFORCEMENT)$/.test(name))delete nativeEnvironment[name];
 if(formsHosts)for(const name of Object.keys(nativeEnvironment))if(/^ERPX_(?:PRODUCT_(?:ROUTING|SSO|ENVIRONMENT)|CONSOLE_RETIRED|[A-Z_]+_PACKAGE_ENFORCEMENT)$/.test(name))delete nativeEnvironment[name];
 for(const name of Object.keys(process.env))if(!(name in nativeEnvironment))delete process.env[name];
 Object.assign(process.env,nativeEnvironment);
 const {getRuntime,closeRuntime}=await import("@erpx/server-runtime/adapters/runtime/env"),env=getRuntime();
 try{
  if(['local-seed','staff-work','expense','work-cards','messaging','services','executive-work','booking'].includes(profile)){
   const {prepareModuleBrowser}=await import("../../fixtures/shared/builders/module-browser");
   state.fixture=await prepareModuleBrowser(profile,env,directory);await writePrivate(statePath,state);
  }else{
  await env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','simulation','Browser simulation','H001_DB','H001_READ_DB','H001_FILES','active')").run();
  await env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h002','riverside-lantern','Riverside Lantern Hotel & Kitchen','H002_DB','H002_READ_DB','H002_FILES','active')").run();
  const {seedExpenseCompany}=await import('../../fixtures/expenses/company'),{mapSpendingOrganization}=await import("../../tests/support/helpers/spending-organization");
  const f=await seedExpenseCompany(env.H001_DB,env.H001_FILES);await mapSpendingOrganization(f);
  await env.H001_DB.prepare("UPDATE organization_nodes SET display_name='Isolated browser simulation' WHERE id=?").bind(f.company).run();
  const {hashPassword}=await import('better-auth/crypto'),{businessRoles}=await import("@erpx/feature-identity_access/lib/business-roles"),{moduleRegistry}=await import('@erpx/catalogue/module-registry');
 const password=`Simulation-${randomBytes(15).toString('hex')}`,hash=await hashPassword(password),now=Date.now(),users:Array<{label:string;email:string;role:string;userId:string;roles:string[];scope?:string;scopeNode?:string;scopeParentNode?:string}> = [];
  for(const [actor,role,label,email] of [[f.owner,'holding_owner','Source operator','source@simulation.test'],[f.head,'warehouse_manager','Receiving operator','receiver@simulation.test'],[f.manager,'finance_approver','Finance reviewer','finance@simulation.test'],[f.reviewer,'finance_preparer','Finance preparer','preparer@simulation.test'],[f.payer,'quality_manager','Quality release manager','quality-manager@simulation.test'],[f.it,'purchasing_manager','Purchasing manager','buyer@simulation.test'],[f.claimant,'support_agent','Support agent','support-agent@simulation.test'],[f.tie,'support_manager','Support manager','support-manager@simulation.test']] as const){
   actor.role=role;actor.branchId=null;actor.departmentId=null;actor.holdingSlug='simulation';actor.assignmentId=await f.grant(actor,role);
   await env.H001_DB.prepare('UPDATE assignments SET revoked_at=? WHERE user_id=? AND company_id=? AND id<>?').bind(now,actor.userId,f.company,actor.assignmentId).run();
   const modules=role==='holding_owner'?moduleRegistry.map(m=>m.id):businessRoles[role as keyof typeof businessRoles];await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),actor.assignmentId).run();
   await env.H001_DB.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(label,label,email,f.company,actor.userId).run();
   await env.CONTROL_DB.batch([env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,label,email,now,now),env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,label,now)]);
   users.push({label,email,role,userId:actor.userId,roles:[role]});
  }
  const extraGrant=async(actor:typeof f.claimant,role:keyof typeof businessRoles|'holding_owner')=>{const id=await f.grant(actor,role);const modules=role==='holding_owner'?moduleRegistry.map(m=>m.id):businessRoles[role];await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),id).run();return id;};
  const employeeActor={...f.claimant,role:'employee',assignmentId:await extraGrant(f.claimant,'employee')};
  await extraGrant(f.head,'people_manager');
  await extraGrant(f.payer,'hr_admin');users.find(user=>user.userId===f.payer.userId)!.roles.push('hr_admin');
  const {changeAppointment}=await import("@erpx/server-organization/public/department-authority");
  const hrSeat=await env.H001_DB.prepare("SELECT version FROM authority_seats WHERE company_id=? AND kind='hr' AND scope_id=?").bind(f.company,f.company).first<{version:number}>();
  await changeAppointment(env.H001_DB,f.owner,{kind:'hr',positionId:null,personId:`person-${f.payer.userId}`,expectedVersion:Number(hrSeat?.version??0),reason:'Fictional Company owner appoints the independent HR reviewer for organization changes.'},crypto.randomUUID());
  await extraGrant(f.payer,'holding_owner');users.find(user=>user.userId===f.payer.userId)!.roles.push('holding_owner');
  await extraGrant(f.it,'purchasing_operator');
  await extraGrant(f.it,'warehouse_manager');
  await extraGrant(f.tie,'purchasing_manager');
  const salesOperatorAssignment=await extraGrant(f.claimant,'sales_operator');
  // An action-card owner selects one published role that can open both the
  // collaboration workspace and its Sales source. The Employee role remains
  // limited to its normal self-service permissions.
  const collaborationOwnerRole='custom_demo_sales_collaboration_owner';
  await env.H001_DB.batch([
   env.H001_DB.prepare('INSERT INTO iam_roles(company_id,id,name,built_in,published_revision) VALUES(?,?,?,false,1)').bind(f.company,collaborationOwnerRole,'Fictional Sales collaboration owner'),
   env.H001_DB.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,1,?,'[]','[\"holding\"]','fixture',?)").bind(f.company,collaborationOwnerRole,JSON.stringify(['records.read','records.write','commerce.read','module.sales.read','module.collaboration.read','self.tasks']),now),
  ]);
  const collaborationOwnerAssignment=await f.grant(f.claimant,collaborationOwnerRole);
  await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(['sales','collaboration']),collaborationOwnerAssignment).run();
  users.find(user=>user.userId===f.claimant.userId)!.roles.push(collaborationOwnerRole);
  await extraGrant(f.tie,'sales_manager');
  await extraGrant(f.reviewer,'quality_inspector');
  await extraGrant(f.reviewer,'finance_approver');users.find(user=>user.userId===f.reviewer.userId)!.roles.push('finance_approver');
  const {setFinanceCompanyTool}=await import('@erpx/server-finance/public/finance-company-tools');
  await setFinanceCompanyTool(f.reviewer,{tool:'bank_automation',enabled:true,expectedVersion:0,reason:'Fictional isolated BankImport lost-response recovery browser checkpoint'},crypto.randomUUID());
  await extraGrant(f.payer,'treasury');users.find(user=>user.userId===f.payer.userId)!.roles.push('treasury');
  await extraGrant(f.owner,'payroll_preparer');users.find(user=>user.userId===f.owner.userId)!.roles.push('payroll_preparer');
  await extraGrant(f.manager,'payroll_approver');users.find(user=>user.userId===f.manager.userId)!.roles.push('payroll_approver');
  const addScopedEmployee=async(label:string,email:string,scope:'branch'|'department',node:string,parentNode?:string)=>{
   const actor=await f.member(`uat-${scope}`,'employee');
   const assignmentId=await f.grant(actor,'employee',scope,node),modules=businessRoles.employee;
   await env.H001_DB.batch([
    env.H001_DB.prepare('UPDATE assignments SET revoked_at=? WHERE user_id=? AND company_id=? AND id<>?').bind(now,actor.userId,f.company,assignmentId),
    env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),assignmentId),
    env.H001_DB.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(label,label,email,f.company,actor.userId),
   ]);
   await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,label,email,now,now),
    env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),
    env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,label,now),
   ]);
   await placeScopedBrowserEmployee(f,{userId:actor.userId,assignmentId,scope,nodeId:node,...(parentNode?{parentNodeId:parentNode}:{})});
   users.push({label,email,role:'employee',userId:actor.userId,roles:['employee'],scope,scopeNode:node,...(parentNode?{scopeParentNode:parentNode}:{})});
  };
  await addScopedEmployee('Branch employee','branch@simulation.test','branch',f.branch);
  await addScopedEmployee('Department employee','department@simulation.test','department',f.department,f.branch);
  // Keep every role used by the operator guide discoverable in the isolated
  // credentials file. Each role still has its own assignment and must be
  // selected explicitly by the browser session; this only makes the account
  // capable of exercising the documented joiner/manager handoffs.
  for(const [userId,role] of [[f.claimant.userId,'employee'],[f.head.userId,'people_manager'],[f.it.userId,'purchasing_operator'],[f.it.userId,'warehouse_manager'],[f.tie.userId,'purchasing_manager'],[f.claimant.userId,'sales_operator'],[f.tie.userId,'sales_manager'],[f.reviewer.userId,'quality_inspector']] as const){const user=users.find(item=>item.userId===userId);if(user&&!user.roles.includes(role))user.roles.push(role);}
  const demoPayrollGroup=`${f.company}-demo-pay-group`,demoPayrollCompensation=`${f.company}-demo-pay-compensation`;
  await env.H001_DB.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,?,'TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,?,?)").bind(demoPayrollGroup,f.company,'Demo monthly payroll',f.owner.userId,now,now).run();
  const demoEngagement=await env.H001_DB.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=? AND e.status='active' ORDER BY e.start_date LIMIT 1").bind(f.company,f.claimant.userId).first<{id:string}>();
  if(demoEngagement){
    await env.H001_DB.prepare("UPDATE hr_engagements SET pay_group_id=?,currency='THB',version=version+1,updated_at=? WHERE id=?").bind(demoPayrollGroup,now,demoEngagement.id).run();
    // Declared synthetic monthly THB 1,000 and THB 10/hour compensation for
    // the legacy native payroll/210-minute overtime journey. Real Thai payroll
    // uses the separate independently approved dated statutory fixture below.
    await env.H001_DB.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?, '2026-09-01','monthly',100000,1000,160,'THB','[]','approved',?,?,?)").bind(demoPayrollCompensation,demoEngagement.id,f.owner.userId,now,now).run();
  }
  // Payroll has more than one eligible specialist in this Demo (the payment
  // operator also carries the HR admin role). Persist an explicit owner for
  // every handoff so a reset starts actionable and never leaves the first
  // calculation or independent payment approval waiting for manual routing.
  for(const [stepKey,userId,reason] of [
    ['calculate',f.owner.userId,'Simulation payroll preparation owner'],
    ['preview',f.owner.userId,'Simulation payroll submission owner'],
    ['in_review',f.manager.userId,'Simulation independent payroll reviewer'],
    ['approved',f.manager.userId,'Simulation Finance posting owner'],
    ['posted',f.manager.userId,'Simulation payroll payment preparation owner'],
    ['payment_draft',f.payer.userId,'Simulation independent payroll payment approver'],
    ['payment_approved',f.payer.userId,'Simulation payroll export owner'],
    ['payment_exported',f.payer.userId,'Simulation payroll confirmation owner'],
    ['payment_partially_settled',f.payer.userId,'Simulation payroll retry owner'],
    ['payment_settled',f.payer.userId,'Simulation payroll settlement owner'],
  ] as const)
    await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll',?,?,?,?,?)").bind(f.company,stepKey,userId,f.owner.userId,reason,now).run();
  const {saveServiceTeam,serviceTeamSchema}=await import("@erpx/server-support/public/service-teams");
  await saveServiceTeam(env.H001_DB,f.owner,null,serviceTeamSchema.parse({name:'Demo purchasing team',members:[{userId:f.it.userId,responsibility:'lead'},{userId:f.tie.userId,responsibility:'member'}],coverage:[{category:'purchasing',scopeKind:'holding',scopeNodeId:f.company}]}),crypto.randomUUID());
  const {saveSupplierInvoicePolicy}=await import("@erpx/server-procurement/public/supplier-bill-matching");
  await saveSupplierInvoicePolicy(f.owner,{matchingMode:'required',expectedVersion:0,reason:'Demo Company requires independent receipt and supplier invoice matching before AP'},crypto.randomUUID());
  const {saveEmployeeRequest,employeeRequestSchema}=await import("@erpx/server-records/public/employee-requests");
  const purchaseRequest=await saveEmployeeRequest(env.H001_DB,employeeActor,null,employeeRequestSchema.parse({kind:'purchase',title:'Laptop 10 เครื่องสำหรับทีมบริการลูกค้า',description:'เปลี่ยนเครื่องที่หมดอายุและเพิ่มเครื่องสำหรับพนักงานใหม่ ใช้ข้อมูลจำลองเท่านั้น',location:'สำนักงานใหญ่ ชั้น 4',impact:'normal',branchId:f.branch,departmentId:f.department,neededDate:'2026-10-15',currency:'THB',items:[{description:'Business laptop พร้อมประกัน 3 ปี',quantity:'10',unit:'each',estimatedCost:'25000'}]}),crypto.randomUUID());
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'transfer','submitted',?,?,'Simulation receiving responsibility',?)").bind(f.company,f.head.userId,f.owner.userId,now).run();
  for(const [stepKey,userId,reason] of [['submitted',f.tie.userId,'Simulation independent purchasing approval'],['confirmed',f.head.userId,'Simulation warehouse receiving responsibility'],['stock_gl',f.manager.userId,'Simulation independent goods GL review'],['invoicing',f.reviewer.userId,'Simulation Finance invoice preparation']] as const)
    await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'order',?,?,?, ?,?)").bind(f.company,stepKey,userId,f.owner.userId,reason,now).run();
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'invoice','draft',?,?,'Simulation independent Finance invoice review',?)").bind(f.company,f.manager.userId,f.owner.userId,now).run();
  const inventoryAccountId=`${f.company}-inventory`,clearingAccountId=`${f.company}-clearing`,receivableAccountId=`${f.company}-receivable`,openingEquityAccountId=`${f.company}-opening-equity`;
  await env.H001_DB.batch([env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'INV','Simulation inventory','asset','THB',?,?)").bind(inventoryAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'GRNI','Simulation goods received clearing','liability','THB',?,?)").bind(clearingAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'AR','Simulation receivables','asset','THB',?,?)").bind(receivableAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'OPEN','Simulation opening equity','equity','THB',?,?)").bind(openingEquityAccountId,f.chart,f.company,now,now),env.H001_DB.prepare('UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=?,receivable_account_id=? WHERE company_id=?').bind(inventoryAccountId,clearingAccountId,receivableAccountId,f.company)]);
  const {createJournal,createJournalSchema,postJournal}=await import("@erpx/server-finance/public/finance"),{resolveFlow,assignFlowPrimary}=await import("@erpx/server-workflow/public/flow-routing");
  // Declared imported quantities below carry THB 10 + 500 + 1000 + 10.
  // The independently reviewed opening GL covers every initial stock row.
  const opening=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-01-01',currency:'THB',memo:'Fictional imported opening stock: sales item, single laptops, laptop cases and dedicated transport item',sourceType:'manual',isOpening:true,lines:[{accountId:inventoryAccountId,debit:'1520',credit:'0'},{accountId:openingEquityAccountId,debit:'0',credit:'1520'}]}),crypto.randomUUID());
  const openingFlow=await resolveFlow(env.H001_DB,f.owner,'journal',String(opening.id));await assignFlowPrimary(env.H001_DB,f.owner,'journal',String(opening.id),{stepKey:'draft',userId:f.manager.userId,expectedVersion:openingFlow.version,recordVersion:1,reason:'Simulation independent opening balance review',saveDefault:false},crypto.randomUUID());
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'journal','draft',?,?,'Simulation independent journal review',?)").bind(f.company,f.manager.userId,f.owner.userId,now).run();
  await postJournal(f.manager,String(opening.id),crypto.randomUUID(),{expectedVersion:1});
  const browserOpeningChartId=`${f.company}-browser-opening-chart`,browserOpeningCashId=`${f.company}-browser-opening-cash`,browserOpeningEquityId=`${f.company}-browser-opening-equity`;
  await env.H001_DB.exec(`
    INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at)
      VALUES('${browserOpeningChartId}','${f.company}','Demo opening import chart','THB','IFRS','active','${f.owner.userId}',${now},${now});
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
      ('${browserOpeningCashId}','${browserOpeningChartId}','${f.company}','1100','Demo opening cash','asset','THB',0,'none',${now},${now}),
      ('${browserOpeningEquityId}','${browserOpeningChartId}','${f.company}','3100','Demo opening equity','equity','THB',0,'none',${now},${now});
  `);
  const refundEvidenceDate=new Date(Date.now()+86_400_000).toISOString().slice(0,10);
  // Exercise the new account defaults through independent Finance owner review in this disposable cohort.
  await env.H001_DB.prepare("UPDATE finance_accounts SET is_control=1,control_kind='receivable' WHERE id=?").bind(receivableAccountId).run();
  const {prepareSalesDefaults,decideSalesDefaults}=await import('@erpx/server-finance/public/sales');
  const salesDefaults=await prepareSalesDefaults(f.reviewer,{chartId:f.chart,currency:'THB',receivableAccountId,revenueAccountId:f.accounts.revenue,taxAccountId:f.accounts.payable,reason:'Fictional independently reviewed sales account defaults for browser simulation'},crypto.randomUUID());
  await decideSalesDefaults(f.manager,String(salesDefaults.id),{expectedVersion:1,action:'approve',reason:'Independent fixture Finance approver verified active sales accounts'},crypto.randomUUID());
  const closePeriods:[string,string]=[`${f.company}-close-2027-01`,`${f.company}-close-2027-02`],closeBank=`${f.company}-close-bank`,closeAr=`${f.company}-close-ar`,closeAp=`${f.company}-close-ap`,closeEquity=`${f.company}-close-equity`,closeCustomer=`${f.company}-close-customer`,closeSupplier=`${f.company}-close-supplier`,closeJournal=`${f.company}-close-opening`,paymentPeriod=`${f.company}-payment-2027-03`,paymentSupplier=`${f.company}-payment-supplier`,paymentInvoice=`${f.company}-payment-invoice`,paymentInvoiceJournal=`${f.company}-payment-invoice-journal`,bankMatchPayment=`${f.company}-bank-match-payment`,bankMatchJournal=`${f.company}-bank-match-journal`,bankMatchClearing=`${f.company}-bank-match-clearing`,paymentStatement=`${f.company}-payment-bank-statement`,paymentBankTransaction=`${f.company}-payment-bank-row`,refundStatement=`${f.company}-refund-bank-statement`,refundBankTransaction=`${f.company}-refund-bank-row`;
  await env.H001_DB.exec(`
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
      ('${closeBank}','${f.chart}','${f.company}','CLOSE-BANK','Demo operating bank','asset','THB',1,'bank',${now},${now}),
      ('${closeAr}','${f.chart}','${f.company}','CLOSE-AR','Demo receivables','asset','THB',1,'receivable',${now},${now}),
      ('${closeAp}','${f.chart}','${f.company}','CLOSE-AP','Demo payables','liability','THB',1,'payable',${now},${now}),
      ('${closeEquity}','${f.chart}','${f.company}','CLOSE-EQUITY','Demo opening equity','equity','THB',0,'none',${now},${now}),
      ('${bankMatchClearing}','${f.chart}','${f.company}','CLOSE-UNAPPLIED-RECEIPTS','Demo unapplied customer receipts','liability','THB',0,'none',${now},${now});
    UPDATE commerce_settings SET receivable_account_id='${receivableAccountId}',cash_account_id='${closeBank}' WHERE company_id='${f.company}';
    INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES
      ('${closePeriods[0]}','${f.company}','January 2027','2027-01-01','2027-01-31',${now}),
      ('${closePeriods[1]}','${f.company}','February 2027','2027-02-01','2027-02-28',${now}),
      ('${paymentPeriod}','${f.company}','March 2027','2027-03-01','2027-03-31',${now});
    INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES
      ('${closeCustomer}','${f.company}','customer','CLOSE-CUSTOMER','Demo close customer','THB','active',${now},${now}),
      ('${closeSupplier}','${f.company}','supplier','CLOSE-SUPPLIER','Demo close supplier','THB','active',${now},${now}),
      ('${paymentSupplier}','${f.company}','supplier','PAYMENT-SUPPLIER','Demo controlled payment supplier','THB','active',${now},${now});
    INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES
      ('${closeJournal}','${f.company}','${f.chart}','${closePeriods[0]}','DEMO-CLOSE-OPEN','2027-01-01','THB',1000000,'opening_import',NULL,'Reconciled close demo opening','posted','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now}),
      ('${paymentInvoiceJournal}','${f.company}','${f.chart}','${paymentPeriod}','DEMO-PAYABLE-JOURNAL','2027-03-10','THB',1000000,'invoice','${paymentInvoice}','Demo controlled payment source invoice','posted','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now}),
      ('${bankMatchJournal}','${f.company}','${f.chart}','${paymentPeriod}','DEMO-RECONCILIATION-RECEIPT','2027-03-15','THB',1000000,'finance_payment','${bankMatchPayment}','Previously posted customer receipt awaiting exact bank reconciliation','posted','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now});
    INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES
      ('${closeJournal}-bank','${closeJournal}','${closeBank}',100000,0,100000,0,0),
      ('${closeJournal}-ar','${closeJournal}','${closeAr}',30000,0,30000,0,1),
      ('${closeJournal}-ap','${closeJournal}','${closeAp}',0,20000,0,20000,2),
      ('${closeJournal}-equity','${closeJournal}','${closeEquity}',0,110000,0,110000,3),
      ('${paymentInvoiceJournal}-expense','${paymentInvoiceJournal}','${f.accounts.expense}',12500,0,12500,0,0),
      ('${paymentInvoiceJournal}-payable','${paymentInvoiceJournal}','${closeAp}',0,12500,0,12500,1),
      ('${bankMatchJournal}-bank','${bankMatchJournal}','${closeBank}',12500,0,12500,0,0),
      ('${bankMatchJournal}-unapplied','${bankMatchJournal}','${bankMatchClearing}',0,12500,0,12500,1);
    INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at,fx_rate_micros,base_total_minor,base_outstanding_minor) VALUES
      ('${f.company}-close-ar-invoice','${f.company}','${closeCustomer}','receivable','DEMO-OPEN-AR','2027-01-01','2027-01-31','THB',30000,30000,30000,'${closeAr}','${closeEquity}','opening_import','${closeJournal}','posted','${closeJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,30000,30000),
      ('${f.company}-close-ap-invoice','${f.company}','${closeSupplier}','payable','DEMO-OPEN-AP','2027-01-01','2027-01-31','THB',20000,20000,20000,'${closeEquity}','${closeAp}','opening_import','${closeJournal}','posted','${closeJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,20000,20000),
      ('${paymentInvoice}','${f.company}','${paymentSupplier}','payable','DEMO-CONTROLLED-PAYMENT','2027-03-10','2027-03-31','THB',12500,12500,12500,'${f.accounts.expense}','${closeAp}','standalone',NULL,'posted','${paymentInvoiceJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,12500,12500);
    INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,version,created_by,approved_by,released_by,created_at,updated_at) VALUES
      ('${bankMatchPayment}','${f.company}','${closeCustomer}','receipt','DEMO-RECONCILIATION-RECEIPT','2027-03-15','THB',12500,0,'${closeBank}','settled','${bankMatchJournal}',4,'${f.reviewer.userId}','${f.manager.userId}','${f.payer.userId}',${now},${now});
    INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,imported_by,created_at) VALUES
      ('${f.company}-close-bank-jan','${f.company}','${closeBank}','DEMO-JAN-2027','2027-01-31',100000,100000,'reconciled','${f.company}-close-bank-jan-hash','${f.reviewer.userId}',${now}),
      ('${f.company}-close-bank-feb','${f.company}','${closeBank}','DEMO-FEB-2027','2027-02-28',100000,100000,'reconciled','${f.company}-close-bank-feb-hash','${f.reviewer.userId}',${now}+1),
      ('${paymentStatement}','${f.company}','${closeBank}','DEMO-MARCH-PAYMENT','2027-03-31',100000,87500,'imported','${f.company}-payment-bank-hash','${f.reviewer.userId}',${now}+2),
      ('${refundStatement}','${f.company}','${closeBank}','DEMO-REFUND-CUSTOMER','${refundEvidenceDate}',87500,86500,'imported','${f.company}-refund-bank-hash','${f.reviewer.userId}',${now}+3);
    INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version) VALUES
      ('${paymentBankTransaction}','${paymentStatement}','${f.company}','2027-03-15','DEMO-BANK-PAY-125','Controlled supplier transfer',-12500,'THB','unmatched',1),
      ('${refundBankTransaction}','${refundStatement}','${f.company}','${refundEvidenceDate}','DEMO-BANK-REFUND-10','Customer return refund',-1000,'THB','unmatched',1);
  `);
  const {saveMaster,saveOrder,orderSchema}=await import("@erpx/server-commerce/public/commerce");
  const source=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-SOURCE',name:'Simulation source warehouse'},crypto.randomUUID())).id),target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-TARGET',name:'Simulation receiving warehouse'},crypto.randomUUID())).id),product=String((await saveMaster(f.owner,'products',null,{sku:'SIM-ITEM',name:'Simulation stock only',price:'10'},crypto.randomUUID())).id),laptopEach=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LAP-EACH',name:'Demo laptop single unit',unit:'each',price:'25000'},crypto.randomUUID())).id),laptopCase=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LAP-CASE',name:'Demo laptop case of two',unit:'case',price:'50000'},crypto.randomUUID())).id),demoSuppliers=[String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:'SIM-SUP-A',name:'Demo supplier A'},crypto.randomUUID())).id),String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:'SIM-SUP-B',name:'Demo supplier B'},crypto.randomUUID())).id)];
  const lotProduct=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LOT-MED',name:'Demo lot-tracked medical supplies',unit:'box',trackingMode:'lot',price:'1200'},crypto.randomUUID())).id),serialProduct=String((await saveMaster(f.owner,'products',null,{sku:'SIM-SERIAL-DEVICE',name:'Demo serial-tracked service device',unit:'each',trackingMode:'serial',price:'35000'},crypto.randomUUID())).id);
  const transitProduct=String((await saveMaster(f.owner,'products',null,{sku:'SIM-TRANSIT-ITEM',name:'Fictional transport stock only',price:'10'},crypto.randomUUID())).id);
  await env.H001_DB.batch([
    env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,1000,1)').bind(source,product),
    env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,50000,1)').bind(source,laptopEach),
    env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2000,100000,1)').bind(source,laptopCase),
    env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,1000,1)').bind(source,transitProduct),
  ]);
  const initialStock=await env.H001_DB.prepare('SELECT COALESCE(SUM(s.value_minor),0)::bigint value FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=?').bind(f.company).first<{value:number}>();
  const initialInventoryGl=await env.H001_DB.prepare("SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0)::bigint value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND l.account_id=?").bind(f.company,inventoryAccountId).first<{value:number}>();
  if(Number(initialStock?.value)!==152000||Number(initialInventoryGl?.value)!==152000)throw new Error('FICTIONAL_INITIAL_STOCK_GL_NOT_RECONCILED');
  const {createQualityHold}=await import("@erpx/server-quality/public/quality");
  const {drainQualityAlerts}=await import("@erpx/server-quality/public/quality-alerts");
  const demoDate=(offset:number)=>new Date(Date.now()+offset*86_400_000).toISOString().slice(0,10);
  const qualityDueSoon=await createQualityHold(f.head,{warehouseId:source,productId:laptopEach,expectedStockVersion:1,dueDate:demoDate(0),reason:'Demo laptop intake requires inspection before service deployment'},'demo-quality-due-soon');
  const qualityOverdue=await createQualityHold(f.head,{warehouseId:source,productId:laptopCase,expectedStockVersion:1,dueDate:demoDate(-1),reason:'Demo laptop case packaging review missed its target date'},'demo-quality-overdue');
  await drainQualityAlerts(env.H001_DB,Date.now(),86_400_000);
  const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:'SIM-CUSTOMER',name:'Simulation customer only'},crypto.randomUUID());
  const duplicateCustomerLeft=await saveMaster(f.owner,'parties',null,{kind:'customer',code:'SIM-DUP-A',name:'Demo Harbor Logistics'},crypto.randomUUID()),duplicateCustomerRight=await saveMaster(f.owner,'parties',null,{kind:'customer',code:'SIM-DUP-B',name:'Demo Harbor Logistic'},crypto.randomUUID());
  await env.H001_DB.prepare('UPDATE finance_parties SET tax_registration=? WHERE id IN (?,?)').bind('SIM-TAX-204812340001',duplicateCustomerLeft.id,duplicateCustomerRight.id).run();
  const duplicateCustomerQuotation=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:duplicateCustomerRight.id,warehouseId:source,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),crypto.randomUUID());
  const salesOperatorActor={...f.claimant,role:'sales_operator' as const,assignmentId:salesOperatorAssignment,holdingSlug:'simulation',branchId:null,departmentId:null};
  const quotation=await saveOrder(salesOperatorActor,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:source,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),crypto.randomUUID());
  const quotationFlow=await resolveFlow(env.H001_DB,f.owner,'order',String(quotation.id));
  await assignFlowPrimary(env.H001_DB,f.owner,'order',String(quotation.id),{stepKey:'confirmed',userId:f.claimant.userId,expectedVersion:quotationFlow.version,recordVersion:quotationFlow.recordVersion,reason:'Demo sales operator owns quotation conversion',saveDefault:false},crypto.randomUUID());
  const {createCRMLead}=await import("@erpx/server-crm/public/crm");
  const crmLead=await createCRMLead(salesOperatorActor,{branchId:f.branch,title:'Demo installation scheduling inquiry',contactName:'Demo customer contact',contactDetail:'Email channel is approved for this fictional customer',source:'Demo Customer 360 scenario',brief:'Customer needs a confirmed installation window before quotation follow-up.',dueDate:'2026-09-26'},crypto.randomUUID());
  const actor=(userId:string,role:string)=>{const user=users.find(item=>item.userId===userId)!;return {label:user.label,email:user.email,userId,role};};
  const guide=generalCompanyDemoGuide({run,origin,holdingSlug:'simulation',organizationReviewer:actor(f.payer.userId,'hr_admin'),customerId:String(customer.id),customerDuplicateIds:[String(duplicateCustomerLeft.id),String(duplicateCustomerRight.id)],customerOperator:actor(f.claimant.userId,'sales_operator'),crmLeadId:String(crmLead.id),qualityDueSoonHoldId:String(qualityDueSoon.id),qualityOverdueHoldId:String(qualityOverdue.id),qualityInspector:actor(f.reviewer.userId,'quality_inspector'),qualityManager:actor(f.payer.userId,'quality_manager'),requestId:String(purchaseRequest.id),quotationId:String(quotation.id),sourceWarehouseId:source,targetWarehouseId:target,movementProductId:transitProduct,lotProductId:lotProduct,serialProductId:serialProduct,closePeriodIds:closePeriods,controlledPaymentInvoiceId:paymentInvoice,actors:{employee:actor(f.claimant.userId,'employee'),manager:actor(f.head.userId,'people_manager'),hrApprover:actor(f.payer.userId,'hr_admin'),payrollPreparer:actor(f.owner.userId,'payroll_preparer'),buyer:actor(f.it.userId,'purchasing_operator'),purchasingManager:actor(f.tie.userId,'purchasing_manager'),salesOperator:actor(f.claimant.userId,'sales_operator'),salesManager:actor(f.tie.userId,'sales_manager'),warehouse:actor(f.head.userId,'warehouse_manager'),sourceWarehouse:actor(f.it.userId,'warehouse_manager'),financePreparer:actor(f.reviewer.userId,'finance_preparer'),financeApprover:actor(f.manager.userId,'finance_approver'),treasury:actor(f.payer.userId,'treasury'),administrator:actor(f.owner.userId,'holding_owner')}});
  const {seedHospitalityBrowserDemo}=await import("../../fixtures/pms/builders/hospitality-browser-demo");
  const hospitality=await seedHospitalityBrowserDemo({db:env.H002_DB,control:env.CONTROL_DB,files:env.H002_FILES,passwordHash:hash,run,origin,menuImages:menuImagesEnabled&&localAttachmentMode,publishBooking:workspaceFixtureProfile==='public-hosts'});
  state.fixture={customerId:customer.id,customerDuplicateIds:[duplicateCustomerLeft.id,duplicateCustomerRight.id],customerDuplicateQuotationId:duplicateCustomerQuotation.id,quotationId:quotation.id,crmLeadId:crmLead.id,purchaseRequestId:purchaseRequest.id,companyId:f.company,inventoryAccountId,clearingAccountId,openingEquityAccountId,openingJournalId:opening.id,browserOpeningChartId,browserOpeningCashId,browserOpeningEquityId,sourceWarehouseId:source,targetWarehouseId:target,productId:product,transitProductId:transitProduct,laptopEachProductId:laptopEach,laptopCaseProductId:laptopCase,lotProductId:lotProduct,serialProductId:serialProduct,qualityDueSoonHoldId:qualityDueSoon.id,qualityOverdueHoldId:qualityOverdue.id,demoSupplierIds:demoSuppliers,periodId:f.period,closePeriodIds:closePeriods,closeBankAccountId:closeBank,bankMatchPaymentId:bankMatchPayment,controlledPaymentPeriodId:paymentPeriod,controlledPaymentInvoiceId:paymentInvoice,controlledPaymentBankTransactionId:paymentBankTransaction,customerRefundBankTransactionId:refundBankTransaction,expenseAccountId:f.accounts.expense,supplierInvoiceMatchingPolicy:'required',users,scenarioIds:guide.scenarios.map(s=>s.id),hospitality:{companyId:hospitality.companyId,propertyId:hospitality.propertyId,registerId:hospitality.registerId,warehouseId:hospitality.warehouseId,openingJournalId:hospitality.openingJournalId,openingInventoryAccountId:hospitality.openingInventoryAccountId,roomIds:hospitality.roomIds,roomTypeIds:hospitality.roomTypeIds,rateIds:hospitality.rateIds,productIds:hospitality.productIds,supplierIds:hospitality.supplierIds,purchaseRequestId:hospitality.purchaseRequestId,payrollGroupId:hospitality.payrollGroupId,payrollEngagementIds:hospitality.payrollEngagementIds,payrollPersonIds:hospitality.payrollPersonIds,leaveTypeId:hospitality.leaveTypeId,leavePolicyId:hospitality.leavePolicyId,leaveAssignmentId:hospitality.leaveAssignmentId,rosterWeekId:hospitality.rosterWeekId,rosterShiftId:hospitality.rosterShiftId}};await writePrivate(statePath,state);await writePrivate(join(directory,'credentials.json'),{password,users:[...users,...hospitality.users]});await writePrivate(join(directory,'demo-guide.json'),guide);await writePrivate(join(directory,'hospitality-guide.json'),hospitality.guide);
  if(legalDemoEnabled) {
    if(port!==3104)throw new Error('LEGAL_UAT_REQUIRES_3104');
    const legal=await (await import("../../fixtures/legal/builders/legal-browser-demo")).seedLegalBrowserDemo({run,directory,control:env.CONTROL_DB as PostgresDatabase,h001:env.H001_DB as PostgresDatabase,h002:env.H002_DB as PostgresDatabase,h001Files:env.H001_FILES,h002Files:env.H002_FILES});
    state.fixture.legal=legal;await writePrivate(statePath,state);
  }
  if(port===3104||profile==='engineering') {
    const {seedEngineeringMoneyPeopleBrowser}=await import("../../fixtures/finance/builders/engineering-money-people-browser");
    const {seedEngineeringBusinessBrowser}=await import("../../fixtures/shared/builders/engineering-business-browser");
    const {seedEngineeringWorkplacePlatformBrowser}=await import("../../fixtures/shared/builders/engineering-workplace-platform-browser");
    const {seedEngineeringThaiPayrollBrowser}=await import("../../fixtures/people_payroll/builders/engineering-thai-payroll-browser");
    const {seedEngineeringFinanceCloseBrowser}=await import("../../fixtures/finance/builders/engineering-finance-close-browser");
    const {createEngineeringShowcaseGuide}=await import("../../fixtures/shared/builders/engineering-showcase-guide");
    state.fixture.engineeringMoneyPeople=await seedEngineeringMoneyPeopleBrowser({run,directory,control:env.CONTROL_DB as PostgresDatabase,db:env.H001_DB as PostgresDatabase,files:env.H001_FILES,...(workspaceFixtureProfile==='money-people'?{workspaceProducts:['finance','people','work'] as ProductId[]}:workspaceConsoleRetired?{workspaceProducts:workspaceApps!.filter((id):id is ProductId=>productIds.includes(id as ProductId))}:{})});
    // Account/Apps/Admin need the foundation fixture; Learn additionally declares
    // its fictional academy package. Other profiles retain the full cohort.
    const foundationOnly=workspaceApps?.every(id=>['account','apps','admin','portal','legal-portal','forms'].includes(id));
    const academyOnly=workspaceApps?.includes('learn')&&workspaceApps.every(id=>['account','apps','learn'].includes(id));
    const productApps=workspaceApps?.filter(id=>['work','people','recruit'].includes(id))??[];
    const productOnly=productApps.length>0&&workspaceApps?.every(id=>['account','apps','work','people','recruit'].includes(id))||workspaceConsoleRetired&&workspaceApps?.every(id=>['account','apps',...productIds].includes(id));
    if(academyOnly){
     const {seedLearningWorkspacePackages}=await import('../../fixtures/learning/builders/workspace-browser');
     await seedLearningWorkspacePackages({run,db:env.H001_DB as PostgresDatabase,academy:JSON.parse(await readFile(join(directory,'engineering-money-people.json'),'utf8'))});
    }
    if(workspaceFixtureProfile==='money-people'){
     const {seedWorkspaceProductPackages}=await import('../../fixtures/shared/builders/workspace-product-packages');
     await seedWorkspaceProductPackages({run,db:env.H001_DB as PostgresDatabase,apps:['work','people','finance'],cohort:JSON.parse(await readFile(join(directory,'engineering-money-people.json'),'utf8'))});
    }
    if(productOnly&&!workspaceConsoleRetired&&workspaceFixtureProfile!=='money-people'){
     const {seedWorkspaceProductPackages}=await import('../../fixtures/shared/builders/workspace-product-packages');
     await seedWorkspaceProductPackages({run,db:env.H001_DB as PostgresDatabase,apps:productApps,cohort:JSON.parse(await readFile(join(directory,'engineering-money-people.json'),'utf8'))});
    }
    if(workspaceApps?.includes('forms')){const {seedWorkspaceForms}=await import('../../fixtures/marketing/builders/workspace-browser');await seedWorkspaceForms({run,directory,db:env.H001_DB as PostgresDatabase,files:env.H001_FILES});}
    if(!foundationOnly&&!academyOnly&&!productOnly&&!workspaceFixtureProfile){
    state.fixture.engineeringBusiness=await seedEngineeringBusinessBrowser({run,db:env.H001_DB,f});
    state.fixture.engineeringWorkplacePlatform=await seedEngineeringWorkplacePlatformBrowser({run,directory,control:env.CONTROL_DB as PostgresDatabase,h001:env.H001_DB as PostgresDatabase,h002:env.H002_DB as PostgresDatabase,h003:env.H003_DB as PostgresDatabase,h001Files:env.H001_FILES,h002Files:env.H002_FILES,h003Files:env.H003_FILES});
    state.fixture.engineeringThaiPayroll=await seedEngineeringThaiPayrollBrowser({run,directory,control:env.CONTROL_DB as PostgresDatabase,db:env.H001_DB as PostgresDatabase,files:env.H001_FILES});
    state.fixture.engineeringFinanceClose=await seedEngineeringFinanceCloseBrowser({run,directory,control:env.CONTROL_DB as PostgresDatabase,db:env.H001_DB as PostgresDatabase,files:env.H001_FILES});
    }
    if(port===3104&&legalDemoEnabled){
     const showcaseGuide=await createEngineeringShowcaseGuide({run,directory,origin,sourceFingerprint,fixture:state.fixture,databases:{CONTROL_DB:env.CONTROL_DB as PostgresDatabase,H001_DB:env.H001_DB as PostgresDatabase,H002_DB:env.H002_DB as PostgresDatabase,H003_DB:env.H003_DB as PostgresDatabase}});
     await writePrivate(join(directory,'engineering-showcase-guide.json'),showcaseGuide);
     state.fixture.engineeringShowcaseGuide={moduleCount:showcaseGuide.moduleCount,path:join(directory,'engineering-showcase-guide.json')};
    }
    await writePrivate(statePath,state);
  }
  }
 }finally{await closeRuntime();}
 // Fixture diagnostics never become a runnable cohort or release evidence.
 // They exercise every native seed while implementation continues, avoiding
 // an expensive app copy and a misleading ready flag on changing source.
 if(diagnostic){state.ready=false;state.diagnostic=true;await writePrivate(statePath,state);console.log(JSON.stringify({run,origin,statePath,ready:false,diagnostic:true,nativeSeedsCompleted:true}));process.exit(0);}
 if(localAttachmentMode){
  // Establish actual scanner/checksum/quota receipts once before freezing the
  // template. Each isolated case inherits only this prerequisite evidence.
  const resultLog=await open(join(directory,'attachment-baseline.json'),'w',0o600),errorLog=await open(join(directory,'attachment-baseline-error.log'),'w',0o600);
  try{
   const reconciliation=Bun.spawn([process.execPath,'--env-file=/dev/null',join(root,'scripts/local/run-server-script.ts'),join(root,'scripts/local/reconcile-attachments.ts'),'--apply','--checksum','--scan','--final'],{
    cwd:root,env:{...process.env,...safeEnv(secret),ERPX_ATTACHMENTS_V2:'false',ERPX_ATTACHMENT_UPLOAD_FREEZE:'true'},stdout:resultLog.fd,stderr:errorLog.fd,
   });
   if(await reconciliation.exited!==0)throw new Error('BROWSER_BASELINE_RECONCILIATION_FAILED');
  }finally{await resultLog.close();await errorLog.close();}
  const receipt=JSON.parse(await readFile(join(directory,'attachment-baseline.json'),'utf8'));
  if(receipt.gateReady!==true||receipt.sourceFingerprint!==sourceFingerprint)throw new Error('BROWSER_BASELINE_ATTACHMENT_GATE_NOT_READY');
 }
 await mkdir(app,{recursive:true});for(const name of ['src','apps','packages','public','scripts','fixtures','migrations/postgres','tests/support','tests/catalogue','tests/config','ops/macos/clamav','ops/macos/install-attachment-runtime.py','ops/macos/attachment-rollout.md','ops/macos/erpxctl.py','ops/macos/offsite.py','ops/macos/recovery_status.py','package.json','bun.lock','next.config.ts','tsconfig.json','tsconfig.base.json','next-env.d.ts','postcss.config.mjs','node_modules'])await copyWorkspaceSnapshot(join(root,name),join(app,name),path=>workspaceCopyInput(root,path));
 if(await runtimeSourceFingerprint(root)!==sourceFingerprint||await runtimeSourceFingerprint(app)!==sourceFingerprint)throw new Error('BROWSER_PREPARATION_SOURCE_CHANGED');
 state.softwareVersion=`release-${sourceFingerprint}`;
 await writeFile(join(app,'packages/catalogue/src/release-artifact.json'),JSON.stringify({softwareVersion:state.softwareVersion})+'\n');
 if(pmsProviderFixtureEnabled){
  state.providerFixture=true;
  await writePrivate(join(directory,'provider-fixture.json'),{schemaVersion:1,keys:{},resources:{},requests:[]});
 }
 state.ready=true;await writePrivate(statePath,state);
 console.log(JSON.stringify({run,origin,statePath,credentialsPath:join(directory,'credentials.json'),guidePath:join(directory,'demo-guide.json'),hospitalityGuidePath:join(directory,'hospitality-guide.json'),ready:true}));
}else if(mode==='start'){
 const state=JSON.parse(await readFile(statePath,'utf8')) as State;if(state.run!==run||state.prefix!==`erpx_browser_${run}`)throw new Error('BROWSER_STATE_MISMATCH');if((state.port??3102)!==port||state.origin!==origin)throw new Error('BROWSER_SIMULATION_PORT_MISMATCH');if(state.diagnostic||!state.ready||!existsSync(join(app,'package.json')))throw new Error('BROWSER_PREPARATION_INCOMPLETE');await freePort();const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));const environment={...safeEnv(secret),ERPX_REPOSITORY_ROOT:app};
 if(Boolean(state.productHosts)!==productHosts)throw Error('PRODUCT_HOST_MODE_CHANGED');
 if(Boolean(state.executiveWorkHosts)!==executiveWorkHosts)throw Error('EXECUTIVE_WORK_HOST_MODE_CHANGED');
 if(Boolean(state.formsHosts)!==formsHosts)throw Error('FORMS_HOST_MODE_CHANGED');
 if(formsHosts&&(productHosts||executiveWorkHosts||port!==3104||profile!=='engineering'||!process.env.ERPX_BROWSER_SIMULATION_RELEASE_DIR||!state.fixture?.engineeringWorkplacePlatform))throw Error('EXACT_FORMS_HOST_ENGINEERING_RELEASE_REQUIRED');
 if(productHosts){
  const armPath=resolve(process.env.ERPX_PRODUCT_HOST_PLAN!),raw=await readFile(armPath),arm=JSON.parse(raw.toString());
  if(!armPath.startsWith(join(root,'.scratch/dev/'))||await realpath(armPath)!==armPath||createHash('sha256').update(raw).digest('hex')!==process.env.ERPX_PRODUCT_HOST_PLAN_SHA256||arm.enabled!==true||arm.expiresAt<=Date.now()||arm.runtimeFingerprint!==state.sourceFingerprint||!process.env.ERPX_BROWSER_SIMULATION_RELEASE_DIR||arm.allowIsolatedNormalMfa!==true||arm.allowIsolatedNormalCommercial!==true)throw Error('CURRENT_PRODUCT_HOST_ARM_REQUIRED');
  Object.assign(environment,{ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_CONSOLE_RETIRED:'enabled'});
  for(const owner of ['WORK','SUPPORT','SERVICE','POS','PMS','FINANCE','PEOPLE','COMMERCE','PROJECTS','ASSETS','LEARNING','MANUFACTURING','TALENT','LEGAL'])Object.assign(environment,{[`ERPX_${owner}_PACKAGE_ENFORCEMENT`]:'enforced'});
 }
 if(executiveWorkHosts){
  if(productHosts||profile!=='executive-work'||port!==3104||!process.env.ERPX_BROWSER_SIMULATION_RELEASE_DIR)throw Error('EXACT_EXECUTIVE_WORK_RELEASE_COHORT_REQUIRED');
  if(Object.keys(environment).some(name=>/^(?:ERPX_(?:COMMERCIAL|MFA|CONSOLE_RETIRED)|ERPX_[A-Z_]+_PACKAGE_ENFORCEMENT)$/.test(name)))throw Error('EXECUTIVE_WORK_FLAGS_MUST_REMAIN_UNSET');
  Object.assign(environment,{ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled',ERPX_PRODUCT_ENVIRONMENT:'development'});
 }
 if(formsHosts&&Object.keys(environment).some(name=>/^(?:ERPX_(?:COMMERCIAL|MFA|CONSOLE_RETIRED|PRODUCT_(?:ROUTING|SSO|ENVIRONMENT))|ERPX_[A-Z_]+_PACKAGE_ENFORCEMENT)$/.test(name)))throw Error('FORMS_HOST_INSTALLED_MODES_MUST_REMAIN_UNSET');
 assertCohortManifest(JSON.parse(await readFile(manifestPath,'utf8')));
 if(localAttachmentMode){
  const required=[environment.ERPX_CLAMD_SOCKET,environment.ERPX_CLAMAV_DATABASE_DIR,environment.ERPX_HEIF_CONVERT,environment.ERPX_PDFTOPPM_BIN];
  if(required.some(value=>typeof value!=='string'||!existsSync(value)))throw new Error('LOCAL_ATTACHMENT_RUNTIME_FILES_MISSING');
  const {attachmentRuntimeReadiness}=await import("@erpx/server-files/public/attachments/readiness");
  const readiness=await attachmentRuntimeReadiness(environment);
  if(!readiness.ready)throw new Error(`LOCAL_ATTACHMENT_RUNTIME_NOT_READY:${readiness.scanner.error??readiness.preview.error??'UNKNOWN'}`);
 }
 let webEntrypoint=join(app,'node_modules/next/dist/bin/next'),jobsBundle=join(app,'.runtime/jobs/jobs.mjs');
 let webArgs=[webEntrypoint,'dev',join(app,'apps/compat'),...(useWebpack?['--webpack']:[]),'--hostname','127.0.0.1','-p',String(port)],workingDirectory=app;
 const suppliedRelease=process.env.ERPX_BROWSER_SIMULATION_RELEASE_DIR;
 if(Boolean(state.providerFixture)!==pmsProviderFixtureEnabled||pmsProviderFixtureEnabled&&!suppliedRelease)throw new Error('PMS_PROVIDER_FIXTURE_COMPILED_RELEASE_REQUIRED');
 if(workspaceApps){
  if(Boolean(state.workspaceConsoleRetired)!==workspaceConsoleRetired||state.executionMode!=='development-smoke'||JSON.stringify(state.workspaceApps)!==JSON.stringify(workspaceApps)||(state.workspaceBundler??'webpack')!==workspaceBundler||state.release)throw new Error('WORKSPACE_BROWSER_STATE_APPS_CHANGED');
  const output=join(app,'.runtime/workspace-gateway');await mkdir(output,{recursive:true});
  const bundle=await Bun.build({entrypoints:[join(app,'scripts/testing/workspace-browser-runtime.ts')],outdir:output,target:'node',format:'esm',naming:'gateway.mjs',env:'disable'});
  if(!bundle.success)throw new Error('WORKSPACE_BROWSER_GATEWAY_BUILD_FAILED');
  webEntrypoint=join(output,'gateway.mjs');webArgs=[webEntrypoint];
 }
 if(suppliedRelease){
  if(port!==3104||!localJobsEnabled||!localAttachmentMode)throw new Error('RELEASE_VERIFICATION_REQUIRES_3104_SCANNER_AND_JOBS');
  const releaseRoot=await realpath(resolve(suppliedRelease)),allowedRoot=join(root,'.runtime/releases')+'/';
  if(!releaseRoot.startsWith(allowedRoot))throw new Error('LOCAL_RELEASE_DIRECTORY_REQUIRED');
  const release=JSON.parse(await readFile(join(releaseRoot,'release.json'),'utf8'));
  if(release.node!=='26.11.1'||release.sourceFingerprint!==state.sourceFingerprint||release.softwareVersion!==state.softwareVersion)
   throw new Error('BROWSER_RELEASE_SOURCE_MISMATCH');
  const manifestBytes=await readFile(join(releaseRoot,'release-files.json'));
  if(createHash('sha256').update(manifestBytes).digest('hex')!==release.fileManifestSha256)throw new Error('RELEASE_FILE_MANIFEST_MISMATCH');
  for(const [name,digest] of Object.entries(JSON.parse(manifestBytes.toString()) as Record<string,string>)){
   const path=resolve(releaseRoot,name);if(!path.startsWith(releaseRoot+'/')||createHash('sha256').update(await readFile(path)).digest('hex')!==digest)
    throw new Error('RELEASE_FILE_CHECKSUM_MISMATCH');
  }
  workingDirectory=join(directory,'release');await copyReleaseArtifact(releaseRoot,workingDirectory);
  webEntrypoint=join(workingDirectory,'server.js');jobsBundle=join(workingDirectory,'tools/jobs.mjs');webArgs=[webEntrypoint];
  Object.assign(environment,{NODE_ENV:'production',PORT:String(port),HOSTNAME:'127.0.0.1',NEXT_DEPLOYMENT_ID:release.deploymentId});
  Object.assign(environment,{ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_ENGINEERING_SOURCE_FINGERPRINT:state.sourceFingerprint});
  state.release={id:release.id,fileManifestSha256:release.fileManifestSha256};
  const {enrollEngineeringVerification}=await import("@erpx/server-platform_administration/public/engineering-verification");
  const urls=JSON.parse(await readFile(manifestPath,'utf8')).databases as Record<string,string>;
  const legal=state.fixture?.legal as {companies?:Record<string,{id:string}>}|undefined;
  const company=state.fixture?.companyId as string,hospitality=state.fixture?.hospitality as {companyId:string};
  const moneyPeople=state.fixture?.engineeringMoneyPeople as {companyIds:string[]}|undefined;
  const thaiPayroll=state.fixture?.engineeringThaiPayroll as {companyIds:string[]}|undefined;
  const financeClose=state.fixture?.engineeringFinanceClose as {companyIds:string[]}|undefined;
  const workplace=state.fixture?.engineeringWorkplacePlatform as {companyId:string;realmCompanyId:string}|undefined;
  const bookingFixture=profile==='booking'&&state.profile==='booking'&&state.fixture?.profile==='booking'&&state.fixture?.holdingSlug==='booking-test';
  const moduleCompanyIds=moduleBrowserReleaseVerificationCompanyIds(state.profile??'',state.fixture);
  const completeNormalAccountCohort=typeof company==='string'&&typeof hospitality?.companyId==='string'&&Boolean(moneyPeople?.companyIds.length)&&thaiPayroll?.companyIds.length===2&&financeClose?.companyIds.length===2&&Boolean(workplace?.companyId)&&Boolean(workplace?.realmCompanyId);
  if(!bookingFixture&&!completeNormalAccountCohort&&(!moduleCompanyIds||productHosts))throw new Error('COMPLETE_NORMAL_ACCOUNT_COHORT_REQUIRED');
  const enrollment={run,sourceFingerprint:release.sourceFingerprint,softwareVersion:release.softwareVersion,fileManifestSha256:release.fileManifestSha256};
  const first=new PostgresDatabase({connectionString:urls.H001_DB,max:1}),second=new PostgresDatabase({connectionString:urls.H002_DB,max:1}),third=new PostgresDatabase({connectionString:urls.H003_DB,max:1});
  try{
   let attestation;
   if(bookingFixture){
    // A public guest journey needs only its own isolated hotel. Never enroll
    // arbitrary IDs from metadata or reuse the broad normal-account cohort.
    const hotel=await first.prepare('SELECT id FROM pms_properties WHERE id=? AND company_id=?').bind(state.fixture!.propertyId,company).first();
    if(!hotel)throw new Error('EXACT_BOOKING_PROPERTY_REQUIRED');
    attestation=await enrollEngineeringVerification(first,{...enrollment,companyIds:[company]});
   }else if(moduleCompanyIds&&!completeNormalAccountCohort){
    // General module suites exercise their exact native Company fixture. They
    // need the isolated compiled-runtime attestation, but must not inherit the
    // unrelated all-product engineering cohort or its Company grants.
    attestation=await enrollEngineeringVerification(first,{...enrollment,companyIds:moduleCompanyIds});
   }else{
    attestation=await enrollEngineeringVerification(first,{...enrollment,companyIds:[company,...moneyPeople!.companyIds,...thaiPayroll!.companyIds,...financeClose!.companyIds,workplace!.companyId,...(legal?.companies?.A?[legal.companies.A.id]:[])],disabledCompanyIds:legal?.companies?.B?[legal.companies.B.id]:[]});
    await enrollEngineeringVerification(second,{...enrollment,companyIds:[hospitality.companyId,...(legal?.companies?.C?[legal.companies.C.id]:[])]});
    await enrollEngineeringVerification(third,{...enrollment,companyIds:[workplace!.realmCompanyId]});
   }
   await writePrivate(join(directory,'engineering-attestation.json'),attestation);
  }finally{await first.close();await second.close();await third.close();}
  // Establish real scan, ownership, checksum and quota receipts before the
  // isolated gate is opened. No web/jobs process can upload during this pass.
  const reconcileLog=await open(join(directory,'attachment-final.json'),'w',0o600);
  const reconciliation=spawn(node,[join(workingDirectory,'tools/reconcile-attachments.mjs'),'--apply','--checksum','--scan','--final'],{
   cwd:workingDirectory,env:{...environment,ERPX_ATTACHMENTS_V2:'false',ERPX_ATTACHMENT_UPLOAD_FREEZE:'true'},stdio:['ignore',reconcileLog.fd,reconcileLog.fd],
  });
  const reconciliationExit=await new Promise<number|null>((ok,no)=>{reconciliation.once('error',no);reconciliation.once('exit',ok);});
  await reconcileLog.close();
  if(reconciliationExit!==0)throw new Error('BROWSER_FINAL_ATTACHMENT_RECONCILIATION_FAILED');
  const receipt=JSON.parse(await readFile(join(directory,'attachment-final.json'),'utf8'));
  if(receipt.gateReady!==true)throw new Error('BROWSER_FINAL_ATTACHMENT_GATE_NOT_READY');
 }
 if(localJobsEnabled&&!suppliedRelease){
  const outdir=join(app,'.runtime/jobs');await mkdir(outdir,{recursive:true});
  if(!state.sourceFingerprint||await runtimeSourceFingerprint(app)!==state.sourceFingerprint)throw new Error('BROWSER_WORKER_SOURCE_CHANGED');
  const bundle=await Bun.build({entrypoints:[join(app,'scripts/local/jobs.ts')],outdir,target:'node',format:'esm',conditions:['react-server'],external:['pg'],naming:'jobs.mjs',
   plugins:[{name:'next-server-navigation',setup(build){build.onResolve({filter:/^next\/navigation$/},()=>({path:join(app,'node_modules/next/dist/client/components/navigation.react-server.js')}));}}],
  });
  if(!bundle.success)throw new Error('BROWSER_SIMULATION_JOBS_BUILD_FAILED');
 }
 // The preload validates persisted release identity before either process executes.
 await writePrivate(statePath,state);
 if(pmsProviderFixtureEnabled)environment.NODE_OPTIONS+=` --import=${join(workingDirectory,'tools/pms-provider-fixture.mjs')}`;
 const log=await open(join(directory,'server.log'),'a',0o600);
 const child=spawn(node,webArgs,{cwd:workingDirectory,env:environment,detached:true,stdio:['ignore',log.fd,log.fd]});if(!child.pid)throw new Error('BROWSER_SERVER_FAILED');state.pid=child.pid;state.applicationPoolMax=Number(environment.ERPX_APP_PG_POOL_MAX??1);state.controlPoolMax=Number(environment.ERPX_CONTROL_PG_POOL_MAX??5);state.webEntrypoint=webEntrypoint;await writePrivate(statePath,state);child.unref();await log.close();
 if(localJobsEnabled){
  const jobsLog=await open(join(directory,'jobs.log'),'a',0o600);
  const worker=spawn(node,[jobsBundle],{cwd:workingDirectory,env:{...environment,ERPX_PROCESS_ROLE:'jobs',ERPX_DATABASE_MANIFEST:manifestPath},detached:true,stdio:['ignore',jobsLog.fd,jobsLog.fd]});
  if(!worker.pid){await jobsLog.close();throw new Error('BROWSER_SIMULATION_JOBS_FAILED');}
  state.jobsPid=worker.pid;state.jobsStartedAt=Bun.spawnSync(['ps','-p',String(worker.pid),'-o','lstart=']).stdout.toString().trim();state.jobsPaused=false;state.jobsEntrypoint=jobsBundle;await writePrivate(statePath,state);worker.unref();await jobsLog.close();
 }
 console.log(JSON.stringify({run,pid:state.pid,...(state.jobsPid?{jobsPid:state.jobsPid}:{}),origin,runtime:workspaceApps?'workspace-development':suppliedRelease?'standalone-release':useWebpack?'webpack':'turbopack',softwareVersion:state.softwareVersion}));
}else if(mode==='pause-jobs'||mode==='resume-jobs'){
 const state=JSON.parse(await readFile(statePath,'utf8')) as State,workingDirectory=state.release?join(directory,'release'):app,entrypoint=state.release?join(workingDirectory,'tools/jobs.mjs'):join(app,'.runtime/jobs/jobs.mjs');
 if(Boolean(state.providerFixture)!==pmsProviderFixtureEnabled)throw new Error('PMS_PROVIDER_FIXTURE_RESTART_SETTING_CHANGED');
 if(!localJobsEnabled||!localAttachmentMode||state.run!==run||state.prefix!==`erpx_browser_${run}`||state.port!==port||state.origin!==origin||state.jobsEntrypoint!==entrypoint)throw new Error('EXACT_BROWSER_COHORT_JOBS_REQUIRED');
 const release=state.release?JSON.parse(await readFile(join(workingDirectory,'release.json'),'utf8')):undefined;
 if(release){if(port!==3104||release.sourceFingerprint!==state.sourceFingerprint||release.softwareVersion!==state.softwareVersion||release.fileManifestSha256!==state.release!.fileManifestSha256||createHash('sha256').update(await readFile(join(workingDirectory,'release-files.json'))).digest('hex')!==release.fileManifestSha256)throw new Error('COHORT_JOBS_RELEASE_MISMATCH');}
 else if(await runtimeSourceFingerprint(app)!==state.sourceFingerprint)throw new Error('BROWSER_WORKER_SOURCE_CHANGED');
 const releaseManifestSha256=release?.fileManifestSha256??null,scope=release?'isolated-engineering':'isolated-browser';
 const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
 assertCohortManifest(manifest);
 const receiptPath=join(directory,'jobs-pause.json');
 if(mode==='pause-jobs'){
  if(state.jobsPaused&&!state.jobsPid){console.log(JSON.stringify({run,jobsPaused:true,replayed:true}));process.exit(0);}
  const pid=state.jobsPid;if(!pid||!state.jobsStartedAt)throw new Error('EXACT_COHORT_JOB_PID_REQUIRED');
  const command=Bun.spawnSync(['ps','-p',String(pid),'-o','command=']).stdout.toString().trim();
  const startedAt=Bun.spawnSync(['ps','-p',String(pid),'-o','lstart=']).stdout.toString().trim();
  const actualCwd=Bun.spawnSync(['lsof','-a','-p',String(pid),'-d','cwd','-Fn']).stdout.toString().split('\n').find(line=>line.startsWith('n'))?.slice(1);
  if(!command.includes(entrypoint)||startedAt!==state.jobsStartedAt||actualCwd!==workingDirectory)throw new Error('COHORT_JOB_PROCESS_IDENTITY_CHANGED');
  process.kill(-pid,'SIGTERM');
  const deadline=Date.now()+5_000;while(Date.now()<deadline&&Bun.spawnSync(['ps','-p',String(pid),'-o','pid=']).stdout.toString().trim())await Bun.sleep(100);
  if(Bun.spawnSync(['ps','-p',String(pid),'-o','pid=']).stdout.toString().trim())process.kill(-pid,'SIGKILL');
  const stoppedDeadline=Date.now()+5_000;while(Date.now()<stoppedDeadline&&Bun.spawnSync(['ps','-p',String(pid),'-o','pid=']).stdout.toString().trim())await Bun.sleep(100);
  if(Bun.spawnSync(['ps','-p',String(pid),'-o','pid=']).stdout.toString().trim())throw new Error('COHORT_JOB_DID_NOT_STOP');
  await writePrivate(receiptPath,{scope,run,sourceFingerprint:state.sourceFingerprint,softwareVersion:state.softwareVersion,releaseManifestSha256,jobsPid:pid,jobsStartedAt:startedAt,entrypoint,cwd:actualCwd,stopped:true,pausedAt:new Date().toISOString()});
  state.jobsPid=null;state.jobsPaused=true;await writePrivate(statePath,state);console.log(JSON.stringify({run,jobsPaused:true,stoppedPid:pid}));
 }else{
  if(!state.jobsPaused||state.jobsPid)throw new Error('COHORT_JOBS_ALREADY_RUNNING');
  const pause=JSON.parse(await readFile(receiptPath,'utf8'));if(!pause.stopped||pause.scope!==scope||pause.run!==run||pause.sourceFingerprint!==state.sourceFingerprint||pause.softwareVersion!==state.softwareVersion||pause.entrypoint!==entrypoint||pause.cwd!==workingDirectory||pause.releaseManifestSha256!==releaseManifestSha256)throw new Error('COHORT_JOBS_PAUSE_RECEIPT_REQUIRED');
  const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));
  const environment:NodeJS.ProcessEnv={...safeEnv(secret),ERPX_REPOSITORY_ROOT:workingDirectory,ERPX_RESOURCE_ROOT:workingDirectory,...(release?{NODE_ENV:'production',PORT:String(port),HOSTNAME:'127.0.0.1',NEXT_DEPLOYMENT_ID:release.deploymentId,ERPX_ENGINEERING_VERIFICATION_RUN:run,ERPX_ENGINEERING_SOURCE_FINGERPRINT:state.sourceFingerprint}:{}),ERPX_PROCESS_ROLE:'jobs',ERPX_DATABASE_MANIFEST:manifestPath};
  if(pmsProviderFixtureEnabled)environment.NODE_OPTIONS+=` --import=${join(workingDirectory,'tools/pms-provider-fixture.mjs')}`;
  const log=await open(join(directory,'jobs.log'),'a',0o600),worker=spawn(node,[entrypoint],{cwd:workingDirectory,env:environment,detached:true,stdio:['ignore',log.fd,log.fd]});
  if(!worker.pid){await log.close();throw new Error('COHORT_JOBS_RESTART_FAILED');}
  state.jobsPid=worker.pid;state.jobsStartedAt=Bun.spawnSync(['ps','-p',String(worker.pid),'-o','lstart=']).stdout.toString().trim();state.jobsPaused=false;await writePrivate(statePath,state);worker.unref();await log.close();
  await writePrivate(join(directory,'jobs-resume.json'),{scope,run,sourceFingerprint:state.sourceFingerprint,softwareVersion:state.softwareVersion,releaseManifestSha256,jobsPid:state.jobsPid,jobsStartedAt:state.jobsStartedAt,resumedAt:new Date().toISOString()});
  console.log(JSON.stringify({run,jobsPaused:false,jobsPid:state.jobsPid}));
 }
}else if(mode==='destroy'){
 const state=JSON.parse(await readFile(statePath,'utf8')) as State;if(state.run!==run||state.prefix!==`erpx_browser_${run}`)throw new Error('BROWSER_STATE_MISMATCH');if((state.port??3102)!==port||state.origin!==origin)throw new Error('BROWSER_SIMULATION_PORT_MISMATCH');
 if(existsSync(manifestPath))assertCohortManifest(JSON.parse(await readFile(manifestPath,'utf8')));
 let stopped=false;
 const webMarker=state.webEntrypoint??join(app,'node_modules/next/dist/bin/next'),jobsMarker=state.jobsEntrypoint??join(app,'.runtime/jobs/jobs.mjs');
 if(![join(app,'node_modules/next/dist/bin/next'),join(directory,'release/server.js'),...(state.workspaceApps&&state.executionMode==='development-smoke'&&!state.release?[join(app,'.runtime/workspace-gateway/gateway.mjs')]:[])].includes(webMarker)||
   ![join(app,'.runtime/jobs/jobs.mjs'),join(directory,'release/tools/jobs.mjs')].includes(jobsMarker))throw new Error('BROWSER_ENTRYPOINT_MISMATCH');
 for(const [key,pid,marker,group] of [['pid',state.pid,webMarker,!state.workspaceApps],['jobsPid',state.jobsPid,jobsMarker,true]] as const){
  if(!pid)continue;
  const command=Bun.spawnSync(['ps','-p',String(pid),'-o','command=']).stdout.toString();
  if(command.trim()){
   const expectedCwd=marker.startsWith(join(directory,'release')+'/')?join(directory,'release'):app;
   const actualCwd=Bun.spawnSync(['lsof','-a','-p',String(pid),'-d','cwd','-Fn']).stdout.toString().split('\n').find(line=>line.startsWith('n'))?.slice(1);
   const ownedNextTitle=key==='pid'&&/^next-server(?:\s|$)/.test(command.trim())&&actualCwd===expectedCwd;
   if(!(command.includes(marker)||ownedNextTitle)||actualCwd!==expectedCwd)throw new Error(key==='pid'?'BROWSER_PROCESS_IDENTITY_CHANGED':'BROWSER_JOBS_IDENTITY_CHANGED');
   process.kill(group?-pid:pid,'SIGTERM');stopped=true;if(state.workspaceApps)continue;
  }
  delete state[key];
 }
 if(stopped){console.log('Stopped only the owned browser simulation and worker process groups. Run destroy again after they exit.');await writePrivate(statePath,state);process.exit(0);}
 await freePort();const admin=new Pool({connectionString:adminUrl,max:1});try{await assertAdmin(admin);for(const binding of Object.keys(tracks)){const name=`${state.prefix}_${binding.toLowerCase()}`,owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[name])).rows[0];if(!owner)continue;if(owner.owner!=='erpx_test_admin')throw new Error('BROWSER_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${name}"`);}}finally{await admin.end();}
 // Keep evidence/credentials private until the owned data has been dropped, then remove this exact run only.
 await rm(directory,{recursive:true,force:false});console.log(JSON.stringify({run,destroyed:true}));
}else throw new Error('Use prepare, start <run-id>, pause-jobs <run-id>, resume-jobs <run-id>, or destroy <run-id>.');
