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
import {generalCompanyDemoGuide} from '../fixtures/general-company-demo';
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
  const password=`Simulation-${randomBytes(15).toString('hex')}`,hash=await hashPassword(password),now=Date.now(),users:Array<{label:string;email:string;role:string;userId:string;roles:string[]}>=[];
  for(const [actor,role,label,email] of [[f.owner,'holding_owner','Source operator','source@simulation.test'],[f.head,'warehouse_manager','Receiving operator','receiver@simulation.test'],[f.manager,'finance_approver','Finance reviewer','finance@simulation.test'],[f.reviewer,'finance_preparer','Finance preparer','preparer@simulation.test'],[f.payer,'quality_manager','Quality release manager','quality-manager@simulation.test'],[f.it,'purchasing_manager','Purchasing manager','buyer@simulation.test'],[f.claimant,'support_agent','Support agent','support-agent@simulation.test'],[f.tie,'support_manager','Support manager','support-manager@simulation.test']] as const){
   actor.role=role;actor.branchId=null;actor.departmentId=null;actor.holdingSlug='simulation';actor.assignmentId=await f.grant(actor,role);
   await env.H001_DB.prepare('UPDATE assignments SET revoked_at=? WHERE user_id=? AND company_id=? AND id<>?').bind(now,actor.userId,f.company,actor.assignmentId).run();
   const modules=role==='holding_owner'?moduleRegistry.map(m=>m.id):businessRoles[role as keyof typeof businessRoles];await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),actor.assignmentId).run();
   await env.H001_DB.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(label,label,email,f.company,actor.userId).run();
   await env.CONTROL_DB.batch([env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,label,email,now,now),env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,label,now)]);
   users.push({label,email,role,userId:actor.userId,roles:[role]});
  }
  const extraGrant=async(actor:typeof f.claimant,role:keyof typeof businessRoles)=>{const id=await f.grant(actor,role);await env.H001_DB.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(businessRoles[role]),id).run();return id;};
  const employeeActor={...f.claimant,role:'employee',assignmentId:await extraGrant(f.claimant,'employee')};
  await extraGrant(f.head,'people_manager');
  await extraGrant(f.payer,'hr_admin');users.find(user=>user.userId===f.payer.userId)!.roles.push('hr_admin');
  await extraGrant(f.it,'purchasing_operator');
  await extraGrant(f.it,'warehouse_manager');
  await extraGrant(f.tie,'purchasing_manager');
  const salesOperatorAssignment=await extraGrant(f.claimant,'sales_operator');
  await extraGrant(f.tie,'sales_manager');
  await extraGrant(f.reviewer,'quality_inspector');
  await extraGrant(f.payer,'treasury');users.find(user=>user.userId===f.payer.userId)!.roles.push('treasury');
  await extraGrant(f.owner,'payroll_preparer');users.find(user=>user.userId===f.owner.userId)!.roles.push('payroll_preparer');
  await extraGrant(f.manager,'payroll_approver');users.find(user=>user.userId===f.manager.userId)!.roles.push('payroll_approver');
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
    await env.H001_DB.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?, '2026-09-01','monthly',100000,0,160,'THB','[]','approved',?,?,?)").bind(demoPayrollCompensation,demoEngagement.id,f.owner.userId,now,now).run();
  }
  const {saveServiceTeam,serviceTeamSchema}=await import('../../src/server/service-teams');
  await saveServiceTeam(env.H001_DB,f.owner,null,serviceTeamSchema.parse({name:'Demo purchasing team',members:[{userId:f.it.userId,responsibility:'lead'},{userId:f.tie.userId,responsibility:'member'}],coverage:[{category:'purchasing',scopeKind:'holding',scopeNodeId:f.company}]}),crypto.randomUUID());
  const {saveSupplierInvoicePolicy}=await import('../../src/server/supplier-bill-matching');
  await saveSupplierInvoicePolicy(f.owner,{matchingMode:'required',expectedVersion:0,reason:'Demo Company requires independent receipt and supplier invoice matching before AP'},crypto.randomUUID());
  const {saveEmployeeRequest,employeeRequestSchema}=await import('../../src/server/employee-requests');
  const purchaseRequest=await saveEmployeeRequest(env.H001_DB,employeeActor,null,employeeRequestSchema.parse({kind:'purchase',title:'Laptop 10 เครื่องสำหรับทีมบริการลูกค้า',description:'เปลี่ยนเครื่องที่หมดอายุและเพิ่มเครื่องสำหรับพนักงานใหม่ ใช้ข้อมูลจำลองเท่านั้น',location:'สำนักงานใหญ่ ชั้น 4',impact:'normal',branchId:f.branch,departmentId:f.department,neededDate:'2026-10-15',currency:'THB',items:[{description:'Business laptop พร้อมประกัน 3 ปี',quantity:'10',unit:'each',estimatedCost:'25000'}]}),crypto.randomUUID());
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'transfer','submitted',?,?,'Simulation receiving responsibility',?)").bind(f.company,f.head.userId,f.owner.userId,now).run();
  for(const [stepKey,userId,reason] of [['submitted',f.tie.userId,'Simulation independent purchasing approval'],['confirmed',f.head.userId,'Simulation warehouse receiving responsibility'],['stock_gl',f.manager.userId,'Simulation independent goods GL review'],['invoicing',f.reviewer.userId,'Simulation Finance invoice preparation']] as const)
    await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'order',?,?,?, ?,?)").bind(f.company,stepKey,userId,f.owner.userId,reason,now).run();
  await env.H001_DB.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'invoice','draft',?,?,'Simulation independent Finance invoice review',?)").bind(f.company,f.manager.userId,f.owner.userId,now).run();
  const inventoryAccountId=`${f.company}-inventory`,clearingAccountId=`${f.company}-clearing`,receivableAccountId=`${f.company}-receivable`,openingEquityAccountId=`${f.company}-opening-equity`;
  await env.H001_DB.batch([env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'INV','Simulation inventory','asset','THB',?,?)").bind(inventoryAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'GRNI','Simulation goods received clearing','liability','THB',?,?)").bind(clearingAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'AR','Simulation receivables','asset','THB',?,?)").bind(receivableAccountId,f.chart,f.company,now,now),env.H001_DB.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'OPEN','Simulation opening equity','equity','THB',?,?)").bind(openingEquityAccountId,f.chart,f.company,now,now),env.H001_DB.prepare('UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=?,receivable_account_id=? WHERE company_id=?').bind(inventoryAccountId,clearingAccountId,receivableAccountId,f.company)]);
  const {createJournal,createJournalSchema,postJournal}=await import('../../src/server/finance'),{resolveFlow,assignFlowPrimary}=await import('../../src/server/flow-routing');
  const opening=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-01-01',currency:'THB',memo:'Simulation opening stock only',sourceType:'manual',isOpening:true,lines:[{accountId:inventoryAccountId,debit:'10',credit:'0'},{accountId:openingEquityAccountId,debit:'0',credit:'10'}]}),crypto.randomUUID());
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
  const closePeriods:[string,string]=[`${f.company}-close-2027-01`,`${f.company}-close-2027-02`],closeBank=`${f.company}-close-bank`,closeAr=`${f.company}-close-ar`,closeAp=`${f.company}-close-ap`,closeEquity=`${f.company}-close-equity`,closeCustomer=`${f.company}-close-customer`,closeSupplier=`${f.company}-close-supplier`,closeJournal=`${f.company}-close-opening`,paymentPeriod=`${f.company}-payment-2027-03`,paymentSupplier=`${f.company}-payment-supplier`,paymentInvoice=`${f.company}-payment-invoice`,paymentInvoiceJournal=`${f.company}-payment-invoice-journal`,paymentStatement=`${f.company}-payment-bank-statement`,paymentBankTransaction=`${f.company}-payment-bank-row`,refundStatement=`${f.company}-refund-bank-statement`,refundBankTransaction=`${f.company}-refund-bank-row`;
  await env.H001_DB.exec(`
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES
      ('${closeBank}','${f.chart}','${f.company}','CLOSE-BANK','Demo operating bank','asset','THB',1,'bank',${now},${now}),
      ('${closeAr}','${f.chart}','${f.company}','CLOSE-AR','Demo receivables','asset','THB',1,'receivable',${now},${now}),
      ('${closeAp}','${f.chart}','${f.company}','CLOSE-AP','Demo payables','liability','THB',1,'payable',${now},${now}),
      ('${closeEquity}','${f.chart}','${f.company}','CLOSE-EQUITY','Demo opening equity','equity','THB',0,'none',${now},${now});
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
      ('${paymentInvoiceJournal}','${f.company}','${f.chart}','${paymentPeriod}','DEMO-PAYABLE-JOURNAL','2027-03-10','THB',1000000,'invoice','${paymentInvoice}','Demo controlled payment source invoice','posted','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now});
    INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES
      ('${closeJournal}-bank','${closeJournal}','${closeBank}',100000,0,100000,0,0),
      ('${closeJournal}-ar','${closeJournal}','${closeAr}',30000,0,30000,0,1),
      ('${closeJournal}-ap','${closeJournal}','${closeAp}',0,20000,0,20000,2),
      ('${closeJournal}-equity','${closeJournal}','${closeEquity}',0,110000,0,110000,3),
      ('${paymentInvoiceJournal}-expense','${paymentInvoiceJournal}','${f.accounts.expense}',12500,0,12500,0,0),
      ('${paymentInvoiceJournal}-payable','${paymentInvoiceJournal}','${closeAp}',0,12500,0,12500,1);
    INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at,fx_rate_micros,base_total_minor,base_outstanding_minor) VALUES
      ('${f.company}-close-ar-invoice','${f.company}','${closeCustomer}','receivable','DEMO-OPEN-AR','2027-01-01','2027-01-31','THB',30000,30000,30000,'${closeAr}','${closeEquity}','opening_import','${closeJournal}','posted','${closeJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,30000,30000),
      ('${f.company}-close-ap-invoice','${f.company}','${closeSupplier}','payable','DEMO-OPEN-AP','2027-01-01','2027-01-31','THB',20000,20000,20000,'${closeEquity}','${closeAp}','opening_import','${closeJournal}','posted','${closeJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,20000,20000),
      ('${paymentInvoice}','${f.company}','${paymentSupplier}','payable','DEMO-CONTROLLED-PAYMENT','2027-03-10','2027-03-31','THB',12500,12500,12500,'${f.accounts.expense}','${closeAp}','standalone',NULL,'posted','${paymentInvoiceJournal}','${f.reviewer.userId}','${f.manager.userId}',${now},${now},${now},1000000,12500,12500);
    INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,status,source_hash,imported_by,created_at) VALUES
      ('${f.company}-close-bank-jan','${f.company}','${closeBank}','DEMO-JAN-2027','2027-01-31',100000,100000,'reconciled','${f.company}-close-bank-jan-hash','${f.reviewer.userId}',${now}),
      ('${f.company}-close-bank-feb','${f.company}','${closeBank}','DEMO-FEB-2027','2027-02-28',100000,100000,'reconciled','${f.company}-close-bank-feb-hash','${f.reviewer.userId}',${now}+1),
      ('${paymentStatement}','${f.company}','${closeBank}','DEMO-MARCH-PAYMENT','2027-03-31',100000,87500,'imported','${f.company}-payment-bank-hash','${f.reviewer.userId}',${now}+2),
      ('${refundStatement}','${f.company}','${closeBank}','DEMO-REFUND-CUSTOMER','2026-09-23',87500,86500,'imported','${f.company}-refund-bank-hash','${f.reviewer.userId}',${now}+3);
    INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status,version) VALUES
      ('${paymentBankTransaction}','${paymentStatement}','${f.company}','2027-03-15','DEMO-BANK-PAY-125','Controlled supplier transfer',-12500,'THB','unmatched',1),
      ('${refundBankTransaction}','${refundStatement}','${f.company}','2026-09-23','DEMO-BANK-REFUND-10','Customer return refund',-1000,'THB','unmatched',1);
  `);
  const {saveMaster,saveOrder,orderSchema}=await import('../../src/server/commerce');
  const source=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-SOURCE',name:'Simulation source warehouse'},crypto.randomUUID())).id),target=String((await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:'SIM-TARGET',name:'Simulation receiving warehouse'},crypto.randomUUID())).id),product=String((await saveMaster(f.owner,'products',null,{sku:'SIM-ITEM',name:'Simulation stock only',price:'10'},crypto.randomUUID())).id),laptopEach=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LAP-EACH',name:'Demo laptop single unit',unit:'each',price:'25000'},crypto.randomUUID())).id),laptopCase=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LAP-CASE',name:'Demo laptop case of two',unit:'case',price:'50000'},crypto.randomUUID())).id),demoSuppliers=[String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:'SIM-SUP-A',name:'Demo supplier A'},crypto.randomUUID())).id),String((await saveMaster(f.owner,'parties',null,{kind:'supplier',code:'SIM-SUP-B',name:'Demo supplier B'},crypto.randomUUID())).id)];
  const lotProduct=String((await saveMaster(f.owner,'products',null,{sku:'SIM-LOT-MED',name:'Demo lot-tracked medical supplies',unit:'box',trackingMode:'lot',price:'1200'},crypto.randomUUID())).id),serialProduct=String((await saveMaster(f.owner,'products',null,{sku:'SIM-SERIAL-DEVICE',name:'Demo serial-tracked service device',unit:'each',trackingMode:'serial',price:'35000'},crypto.randomUUID())).id);
  await env.H001_DB.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,1000,1)').bind(source,product).run();
  const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:'SIM-CUSTOMER',name:'Simulation customer only'},crypto.randomUUID());
  const salesOperatorActor={...f.claimant,role:'sales_operator' as const,assignmentId:salesOperatorAssignment,holdingSlug:'simulation',branchId:null,departmentId:null};
  const quotation=await saveOrder(salesOperatorActor,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:source,orderDate:'2026-09-20',dueDate:'2026-09-23',lines:[{productId:product,quantity:'1',price:'10',taxBps:0}]}),crypto.randomUUID());
  const quotationFlow=await resolveFlow(env.H001_DB,f.owner,'order',String(quotation.id));
  await assignFlowPrimary(env.H001_DB,f.owner,'order',String(quotation.id),{stepKey:'confirmed',userId:f.claimant.userId,expectedVersion:quotationFlow.version,recordVersion:quotationFlow.recordVersion,reason:'Demo sales operator owns quotation conversion',saveDefault:false},crypto.randomUUID());
  const actor=(userId:string,role:string)=>{const user=users.find(item=>item.userId===userId)!;return {label:user.label,email:user.email,userId,role};};
  const guide=generalCompanyDemoGuide({run,origin,holdingSlug:'simulation',organizationReviewer:actor(f.payer.userId,'hr_admin'),requestId:String(purchaseRequest.id),quotationId:String(quotation.id),sourceWarehouseId:source,targetWarehouseId:target,movementProductId:product,lotProductId:lotProduct,serialProductId:serialProduct,closePeriodIds:closePeriods,controlledPaymentInvoiceId:paymentInvoice,actors:{employee:actor(f.claimant.userId,'employee'),manager:actor(f.head.userId,'people_manager'),buyer:actor(f.it.userId,'purchasing_operator'),purchasingManager:actor(f.tie.userId,'purchasing_manager'),salesOperator:actor(f.claimant.userId,'sales_operator'),salesManager:actor(f.tie.userId,'sales_manager'),warehouse:actor(f.head.userId,'warehouse_manager'),sourceWarehouse:actor(f.it.userId,'warehouse_manager'),financePreparer:actor(f.reviewer.userId,'finance_preparer'),financeApprover:actor(f.manager.userId,'finance_approver'),treasury:actor(f.payer.userId,'treasury'),administrator:actor(f.owner.userId,'holding_owner')}});
  state.fixture={customerId:customer.id,quotationId:quotation.id,purchaseRequestId:purchaseRequest.id,companyId:f.company,inventoryAccountId,clearingAccountId,openingEquityAccountId,openingJournalId:opening.id,browserOpeningChartId,browserOpeningCashId,browserOpeningEquityId,sourceWarehouseId:source,targetWarehouseId:target,productId:product,laptopEachProductId:laptopEach,laptopCaseProductId:laptopCase,lotProductId:lotProduct,serialProductId:serialProduct,demoSupplierIds:demoSuppliers,periodId:f.period,closePeriodIds:closePeriods,closeBankAccountId:closeBank,controlledPaymentPeriodId:paymentPeriod,controlledPaymentInvoiceId:paymentInvoice,controlledPaymentBankTransactionId:paymentBankTransaction,customerRefundBankTransactionId:refundBankTransaction,expenseAccountId:f.accounts.expense,supplierInvoiceMatchingPolicy:'required',users,scenarioIds:guide.scenarios.map(s=>s.id)};await writePrivate(statePath,state);await writePrivate(join(directory,'credentials.json'),{password,users});await writePrivate(join(directory,'demo-guide.json'),guide);
 }finally{await closeRuntime();}
 await mkdir(app,{recursive:true});for(const name of ['src','public','package.json','bun.lock','next.config.ts','tsconfig.json','next-env.d.ts','postcss.config.mjs','node_modules'])await cp(join(root,name),join(app,name),{recursive:true,mode:constants.COPYFILE_FICLONE});
 state.ready=true;await writePrivate(statePath,state);
 console.log(JSON.stringify({run,origin,statePath,credentialsPath:join(directory,'credentials.json'),guidePath:join(directory,'demo-guide.json'),ready:true}));
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
