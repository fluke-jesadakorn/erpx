/** Disposable browser acceptance workspace. Never reuses development/production databases or sessions.
 * bun --conditions=react-server scripts/local/browser-simulation.ts prepare
 * bun --conditions=react-server scripts/local/browser-simulation.ts start <run-id>
 * bun --conditions=react-server scripts/local/browser-simulation.ts destroy <run-id>
 */
import {mkdir,readFile,writeFile,cp,chmod,open,rm} from 'node:fs/promises';
import {existsSync,constants} from 'node:fs';
import {resolve,join} from 'node:path';
import {randomBytes} from 'node:crypto';
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {Pool} from 'pg';
import {PostgresDatabase} from '../../src/server/runtime/postgres';
import {migrateDatabase} from '../../src/server/runtime/migrations';
const root=resolve(import.meta.dir,'../..'),parent=join(root,'.runtime/browser'),port=3102,origin=`http://127.0.0.1:${port}`;
const node=join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin/node');
if(Bun.version!=='1.2.17'||Bun.spawnSync([node,'--version']).stdout.toString().trim()!=='v24.20.0')throw new Error('PINNED_TOOLCHAIN_REQUIRED');
let adminUrl=process.env.ERPX_TEST_ADMIN_URL;
if(!adminUrl)for(const file of ['.env.test.local','.env.test','.env.local']){const path=join(root,file);if(!existsSync(path))continue;const value=(await readFile(path,'utf8')).match(/^\s*(?:export\s+)?ERPX_TEST_ADMIN_URL\s*=\s*(.*?)\s*$/m)?.[1];if(value){adminUrl=value.replace(/^(["'])(.*)\1$/,'$2');break;}}
if(!adminUrl)throw new Error('ISOLATED_TEST_ADMIN_REQUIRED');
const connection=new URL(adminUrl);if(connection.hostname!=='127.0.0.1'||connection.port!=='5432'||connection.username!=='erpx_test_admin'||connection.pathname!=='/postgres')throw new Error('UNSAFE_TEST_DATABASE_ADMIN');
const tracks={CONTROL_DB:'control',CONTROL_READ_DB:'control-read',H001_DB:'holding-write',H001_READ_DB:'holding-read',H002_DB:'holding-write',H002_READ_DB:'holding-read'} as const;
const mode=process.argv[2],run=mode==='prepare'?randomBytes(6).toString('hex'):process.argv[3];
if(!run||!/^([a-f0-9]{12})$/.test(run))throw new Error('EXACT_BROWSER_RUN_ID_REQUIRED');
const directory=join(parent,run),app=join(directory,'app'),manifestPath=join(directory,'databases.json'),statePath=join(directory,'state.json');
type State={run:string;prefix:string;createdAt:string;origin:string;ready?:boolean;pid?:number;fixture?:Record<string,unknown>};
const safeEnv=(secret:string)=>({PATH:`${join(root,'.runtime/node/node-v24.20.0-darwin-arm64/bin')}:${process.env.PATH??''}`,HOME:process.env.HOME??'',TMPDIR:process.env.TMPDIR??'/tmp',NODE_ENV:'development' as const,ERPX_ENVIRONMENT:'local',ERPX_PUBLIC_ORIGIN:origin,BETTER_AUTH_URL:origin,BETTER_AUTH_SECRET:secret,ERPX_DATA_DIR:directory,ERPX_DATABASE_MANIFEST:manifestPath,ERPX_PUBLIC_BETA:'false',NEXT_DEPLOYMENT_ID:`browser-simulation-${run}`,NEXT_TELEMETRY_DISABLED:'1',ERPX_DEV_PG_POOL_MAX:'2'});
async function assertAdmin(admin:Pool){const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(role.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw new Error('UNSAFE_TEST_DATABASE_ROLE');const forbidden=await admin.query("SELECT datname FROM pg_database WHERE (datname LIKE 'epsx\\_%' ESCAPE '\\' OR datname IN ('n8n','plf_webapp_dev')) AND has_database_privilege(current_user,datname,'CONNECT')");if(forbidden.rowCount)throw new Error('TEST_ADMIN_CAN_CONNECT_TO_OTHER_PROJECT');}
async function writePrivate(path:string,value:unknown){await writeFile(path,JSON.stringify(value,null,2)+'\n',{mode:0o600});await chmod(path,0o600);}
async function freePort(){const server=createServer();await new Promise<void>((ok,no)=>{server.once('error',no);server.listen(port,'127.0.0.1',()=>server.close(error=>error?no(error):ok()));});}
if(mode==='prepare'){
 await freePort();await mkdir(directory,{recursive:true,mode:0o700});const prefix=`erpx_browser_${run}`,state:State={run,prefix,createdAt:new Date().toISOString(),origin};await writePrivate(statePath,state);
 const admin=new Pool({connectionString:adminUrl,max:1}),urls:Record<string,string>={};
 try{await assertAdmin(admin);for(const [binding,track] of Object.entries(tracks)){const name=`${prefix}_${binding.toLowerCase()}`;await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);const url=new URL(adminUrl);url.pathname=name;urls[binding]=url.href;const db=new PostgresDatabase({connectionString:url.href,max:1});try{await migrateDatabase(db,track);}finally{await db.close();}}}finally{await admin.end();}
 await writePrivate(manifestPath,{version:1,databases:urls,slots:[{id:'h001',write:'H001_DB',read:'H001_READ_DB',files:'H001_FILES'},{id:'h002',write:'H002_DB',read:'H002_READ_DB',files:'H002_FILES'}]});
 const secret=randomBytes(32).toString('hex');await writePrivate(join(directory,'runtime.json'),{secret});for(const name of Object.keys(process.env))if(/^(AI_|RESEND_|STRIPE_|SITEMINDER_|ERPX_IAM_SECRET_PROFILES)/.test(name))delete process.env[name];Object.assign(process.env,safeEnv(secret));
 const {getRuntime,closeRuntime}=await import('../../src/server/runtime/env'),env=getRuntime();
 try{
  await env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','simulation','Browser simulation','H001_DB','H001_READ_DB','H001_FILES','active')").run();
  const {seedExpenseCompany}=await import('../../fixtures/expenses/company'),{mapSpendingOrganization}=await import('../../tests/helpers/spending-organization');
  const f=await seedExpenseCompany(env.H001_DB,env.H001_FILES);await mapSpendingOrganization(f);
  await env.H001_DB.prepare("UPDATE organization_nodes SET display_name='Isolated browser simulation' WHERE id=?").bind(f.company).run();
  const {hashPassword}=await import('better-auth/crypto'),{businessRoles}=await import('../../src/lib/business-roles'),{moduleRegistry}=await import('../../src/data/module-registry');
  const password=`Simulation-${randomBytes(15).toString('hex')}`,hash=await hashPassword(password),now=Date.now(),users:Array<Record<string,string>>=[];
  for(const [actor,role,label,email] of [[f.owner,'holding_owner','Source operator','source@simulation.test'],[f.head,'warehouse_manager','Receiving operator','receiver@simulation.test'],[f.manager,'finance_approver','Finance reviewer','finance@simulation.test'],[f.reviewer,'quality_inspector','Quality inspector','inspector@simulation.test'],[f.payer,'quality_manager','Quality release manager','quality-manager@simulation.test'],[f.it,'purchasing_manager','Purchasing manager','buyer@simulation.test'],[f.claimant,'support_agent','Support agent','support-agent@simulation.test'],[f.tie,'support_manager','Support manager','support-manager@simulation.test']] as const){
   actor.role=role;actor.branchId=null;actor.departmentId=null;actor.holdingSlug='simulation';actor.assignmentId=await f.grant(actor,role);
   await env.H001_DB.prepare('UPDATE assignments SET revoked_at=? WHERE user_id=? AND company_id=? AND id<>?').bind(now,actor.userId,f.company,actor.assignmentId).run();
   const modules=role==='holding_owner'?moduleRegistry.map(m=>m.id):businessRoles[role as keyof typeof businessRoles];await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),actor.assignmentId).run();
   await env.H001_DB.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(label,label,email,f.company,actor.userId).run();
   await env.CONTROL_DB.batch([env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,label,email,now,now),env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,label,now)]);
   users.push({label,email,role,userId:actor.userId});
  }
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'transfer','submitted',?,?,'Simulation receiving responsibility',?)").bind(f.company,f.head.userId,f.owner.userId,now).run();
  const inventoryAccountId=`${f.company}-inventory`,openingEquityAccountId=`${f.company}-opening-equity`;
  await env.H001_DB.batch([env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'INV','Simulation inventory','asset','THB',?,?)").bind(inventoryAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'OPEN','Simulation opening equity','equity','THB',?,?)").bind(openingEquityAccountId,f.chart,f.company,now,now),env.H001_DB.prepare('UPDATE commerce_settings SET inventory_account_id=? WHERE company_id=?').bind(inventoryAccountId,f.company)]);
  const {createJournal,createJournalSchema,postJournal}=await import('../../src/server/finance'),{resolveFlow,assignFlowPrimary}=await import('../../src/server/flow-routing');
  const opening=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-01-01',currency:'THB',memo:'Simulation opening stock only',sourceType:'manual',isOpening:true,lines:[{accountId:inventoryAccountId,debit:'10',credit:'0'},{accountId:openingEquityAccountId,debit:'0',credit:'10'}]}),crypto.randomUUID());
  const openingFlow=await resolveFlow(env.H001_DB,f.owner,'journal',String(opening.id));await assignFlowPrimary(env.H001_DB,f.owner,'journal',String(opening.id),{stepKey:'draft',userId:f.manager.userId,expectedVersion:openingFlow.version,recordVersion:1,reason:'Simulation independent opening balance review',saveDefault:false},crypto.randomUUID());
  await postJournal(f.manager,String(opening.id),crypto.randomUUID(),{expectedVersion:1});
  const {saveMaster,saveOrder,orderSchema}=await import('../../src/server/commerce');
  const source=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-SOURCE',name:'Simulation source warehouse'},crypto.randomUUID())).id),target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-TARGET',name:'Simulation receiving warehouse'},crypto.randomUUID())).id),product=String((await saveMaster(f.owner,'products',null,{sku:'SIM-ITEM',name:'Simulation stock only',price:'10'},crypto.randomUUID())).id);
  await env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,1000,1)').bind(source,product).run();
  const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:'SIM-CUSTOMER',name:'Simulation customer only'},crypto.randomUUID());
  const quotation=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:source,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),crypto.randomUUID());
  state.fixture={customerId:customer.id,quotationId:quotation.id,companyId:f.company,inventoryAccountId,openingEquityAccountId,openingJournalId:opening.id,sourceWarehouseId:source,targetWarehouseId:target,productId:product,periodId:f.period,expenseAccountId:f.accounts.expense,users};await writePrivate(statePath,state);await writePrivate(join(directory,'credentials.json'),{password,users});
 }finally{await closeRuntime();}
 await mkdir(app,{recursive:true});for(const name of ['src','public','package.json','bun.lock','next.config.ts','tsconfig.json','next-env.d.ts','postcss.config.mjs','node_modules'])await cp(join(root,name),join(app,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
 state.ready=true;await writePrivate(statePath,state);
 console.log(JSON.stringify({run,origin,statePath,credentialsPath:join(directory,'credentials.json'),ready:true}));
}else if(mode==='start'){
 const state=JSON.parse(await readFile(statePath,'utf8')) as State;if(state.run!==run||state.prefix!==`erpx_browser_${run}`)throw new Error('BROWSER_STATE_MISMATCH');if(!state.ready||!existsSync(join(app,'package.json')))throw new Error('BROWSER_PREPARATION_INCOMPLETE');await freePort();const {secret}=JSON.parse(await readFile(join(directory,'runtime.json'),'utf8'));const log=await open(join(directory,'server.log'),'a',0o600);
 const child=spawn(node,[join(app,'node_modules/next/dist/bin/next'),'dev','--hostname','127.0.0.1','-p',String(port)],{cwd:app,env:safeEnv(secret),detached:true,stdio:['ignore',log.fd,log.fd]});if(!child.pid)throw new Error('BROWSER_SERVER_FAILED');state.pid=child.pid;await writePrivate(statePath,state);child.unref();await log.close();console.log(JSON.stringify({run,pid:state.pid,origin}));
}else if(mode==='destroy'){
 const state=JSON.parse(await readFile(statePath,'utf8')) as State;if(state.run!==run||state.prefix!==`erpx_browser_${run}`)throw new Error('BROWSER_STATE_MISMATCH');
 if(state.pid){const command=Bun.spawnSync(['ps','-p',String(state.pid),'-o','command=']).stdout.toString();if(command.trim()){if(!command.includes(join(app,'node_modules/next/dist/bin/next')))throw new Error('BROWSER_PROCESS_IDENTITY_CHANGED');process.kill(-state.pid,'SIGTERM');console.log('Stopped owned browser simulation process group. Run destroy again after it exits.');delete state.pid;await writePrivate(statePath,state);process.exit(0);}}
 await freePort();const admin=new Pool({connectionString:adminUrl,max:1});try{await assertAdmin(admin);for(const binding of Object.keys(tracks)){const name=`${state.prefix}_${binding.toLowerCase()}`,owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[name])).rows[0];if(!owner)continue;if(owner.owner!=='erpx_test_admin')throw new Error('BROWSER_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${name}"`);}}finally{await admin.end();}
 // Keep evidence/credentials private until the owned data has been dropped, then remove this exact run only.
 await rm(directory,{recursive:true,force:false});console.log(JSON.stringify({run,destroyed:true}));
}else throw new Error('Use prepare, start <run-id>, or destroy <run-id>.');
