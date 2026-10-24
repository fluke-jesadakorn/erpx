import {expect,it,vi} from 'vitest';
import {cp,mkdir,mkdtemp,readdir,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
import {Pool} from 'pg';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {LocalFileStore} from '@erpx/server-runtime/adapters/runtime/filesystem';
import {env} from '../../support/runtime-env';
import {migrateDatabase} from '@erpx/server-runtime/public/runtime/migrations';
import {packageGrantStore} from '@erpx/server-commercial/public/package-grants';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {postInvoice,postJournal} from '@erpx/server-finance/public/finance';
import {completeCommerceReturn,decideCommerceReturn,fulfillOrder,invoiceFulfillment,orderSchema,requestCommerceReturn,returnRequestSchema,saveMaster,saveOrder,settleInvoice,transitionOrder,fulfillmentSchema} from '@erpx/server-commerce/public/commerce';
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';

let db:PostgresDatabase,files:LocalFileStore;
const key=()=>randomUUID();
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files}),runtimeEnv:async()=>env}));

function native(error:unknown):{code?:string;constraint?:string;message?:string}{
 let value=error;for(let count=0;count<6&&value&&typeof value==='object';count++){if('code' in value&&value.code==='23514')return {code:'23514',constraint:'constraint' in value&&typeof value.constraint==='string'?value.constraint:undefined,message:'message' in value&&typeof value.message==='string'?value.message:undefined};if(!('cause' in value))break;value=value.cause;}return {};
}
async function migrationReceipts(){return (await db.prepare('SELECT name,checksum,applied_at::text applied_at FROM erpx_migrations ORDER BY name').all<{name:string;checksum:string;applied_at:string}>()).results;}
async function nativeGuardObjects(){return Promise.all([
 db.prepare("SELECT proname FROM pg_proc WHERE pronamespace='public'::regnamespace AND proname IN ('finance_assert_refund_bank_consumer','guard_finance_refund_bank_consumer') ORDER BY proname").all().then(rows=>rows.results),
 db.prepare("SELECT tgname FROM pg_trigger WHERE tgrelid='finance_bank_transactions'::regclass AND tgname='finance_refund_bank_evidence_consumer'").all().then(rows=>rows.results),
]);}
async function allRows(){
 const names=(await db.prepare("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename<>'erpx_migrations' ORDER BY tablename").all<{tablename:string}>()).results.map(row=>row.tablename);
 return Promise.all(names.map(async table=>{if(!/^[a-z][a-z0-9_]*$/.test(table))throw Error('DECLARED_SCHEMA_IDENTIFIER_REQUIRED');return {table,...await db.prepare(`SELECT count(*)::int rows,md5(coalesce(jsonb_agg(to_jsonb(row) ORDER BY to_jsonb(row)::text),'[]'::jsonb)::text) digest FROM "${table}" row`).first<{rows:number;digest:string}>()};}));
}
async function schemaImage(){
 const [relations,functions,triggers,constraints]=await Promise.all([
  db.prepare("SELECT relname name,relkind::text kind,pg_get_userbyid(relowner) owner,relacl::text acl FROM pg_class WHERE relnamespace='public'::regnamespace ORDER BY relname").all(),
  db.prepare("SELECT proname name,pg_get_function_identity_arguments(oid) arguments,pg_get_functiondef(oid) definition,pg_get_userbyid(proowner) owner,proacl::text acl FROM pg_proc WHERE pronamespace='public'::regnamespace AND prokind IN ('f','p') ORDER BY proname,pg_get_function_identity_arguments(oid)").all<{name:string;arguments:string;definition:string;owner:string;acl:string|null}>(),
  db.prepare("SELECT c.relname relation,t.tgname name,pg_get_triggerdef(t.oid) definition FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace='public'::regnamespace AND NOT t.tgisinternal ORDER BY c.relname,t.tgname").all<{name:string;relation:string;definition:string}>(),
  db.prepare("SELECT c.relname relation,k.conname name,pg_get_constraintdef(k.oid) definition FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid WHERE k.connamespace='public'::regnamespace ORDER BY c.relname,k.conname").all<{name:string;relation:string;definition:string}>(),
 ]);
 return {relations:relations.results,functions:functions.results,triggers:triggers.results,constraints:constraints.results};
}

// Original normal Company/Service fixtures copied verbatim from the current
// registered Returns proof; exact function/source hashes are retained privately.
// Only fictional native organization/worker assignment facts are seeded. Every
// ledger/default/Service/refund/bank/payment below is a real owner command.
async function zeroLedgerSalesCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-sales-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Sales fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Sales team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<WorkContext>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,'[]',?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:WorkContext['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Sales team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company};
    const context:WorkContext={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:[],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),salesManager=await actor('sales-manager','sales_manager'),treasury=await actor('treasury','treasury');
  const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[salesManager,3],[treasury,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0]);
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-zero-ledger-Sales-provider'}),obligations:async()=>{throw Error('ZERO_LEDGER_FIXTURE_CANNOT_WITHDRAW');}}),grant={companyId:company,packageId:'sales',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Reviewed isolated zero-ledger services foundation',evidence:'test:zero-ledger-Sales'};
  async function purchase(){await store.apply({expectedVersion:0,idempotencyKey:key(),grant});}
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  return {company,branch,department,owner,reviewer,manager,salesManager,treasury,counts,purchase,store,grant,actor};
}

async function zeroSalesEnvelope(actor:WorkContext){
  const {salesPostingSetup}=await import('@erpx/server-finance/public/sales-posting-setup');
  const view=await salesPostingSetup(actor);
  expect(view.available).toBe(true);expect(view.contextKey).toMatch(/^[a-f0-9]{64}$/);
  if(!view.contextKey)throw Error('Actual current Sales posting context required');
  return {expectedVersion:0 as const,contextKey:view.contextKey};
}

async function zeroSalesReferences(z:Awaited<ReturnType<typeof zeroLedgerSalesCompany>>){
  const api=await import('@erpx/server-finance/public/sales-posting-setup'),chartBody={...await zeroSalesEnvelope(z.reviewer),input:{name:'Original first Sales service chart',baseCurrency:'THB',accountingBasis:'IFRS'}},chartKey=key(),chart=await api.createSalesPostingChart(z.reviewer,chartBody,chartKey);
  expect(await api.createSalesPostingChart(z.reviewer,chartBody,chartKey)).toEqual(chart);
  const accounts={receivableAccountId:'',revenueAccountId:'',taxAccountId:'',cashAccountId:''};
  for(const [field,category,controlKind] of [['receivableAccountId','asset','receivable'],['revenueAccountId','revenue','none'],['taxAccountId','liability','none'],['cashAccountId','asset','bank']] as const){
    const body={...await zeroSalesEnvelope(z.reviewer),input:{chartId:chart.id,code:key(),name:`Original initial ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},command=key(),row=await api.createSalesPostingAccount(z.reviewer,body,command);
    accounts[field]=row.id;expect(await api.createSalesPostingAccount(z.reviewer,body,command)).toEqual(row);
  }
  const periodBody={...await zeroSalesEnvelope(z.reviewer),input:{name:'Original first service year',startDate:'2026-01-01',endDate:'2026-12-31'}},periodKey=key(),period=await api.createSalesPostingPeriod(z.reviewer,periodBody,periodKey);
  expect(period).toMatchObject({state:'open',version:0});expect(await api.createSalesPostingPeriod(z.reviewer,periodBody,periodKey)).toEqual(period);
  const rateInput={currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Original actual Company approver confirms fictional dated service quote'};
  await expect(api.confirmSalesPostingRate(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:rateInput},key())).rejects.toMatchObject({status:403});
  const rateBody={...await zeroSalesEnvelope(z.manager),input:rateInput},rateKey=key(),rate=await api.confirmSalesPostingRate(z.manager,rateBody,rateKey);
  expect(await api.confirmSalesPostingRate(z.manager,rateBody,rateKey)).toEqual(rate);
  expect(await db.prepare('SELECT confirmed_by FROM company_fx_rates WHERE id=?').bind(rate.id).first('confirmed_by')).toBe(z.manager.userId);
  return {api,chart,accounts,period,rate,defaults:{chartId:chart.id,currency:'THB',receivableAccountId:accounts.receivableAccountId,revenueAccountId:accounts.revenueAccountId,taxAccountId:accounts.taxAccountId,reason:'Normal original Company preparer selected the actual service receivable revenue and output VAT'}};
}

async function assignZeroSales(z:Awaited<ReturnType<typeof zeroLedgerSalesCompany>>,kind:'order'|'invoice'|'journal',id:string,stepKey:string,actor:WorkContext){
  const flow=await resolveFlow(db,z.owner,kind,id);
  await assignFlowPrimary(db,z.owner,kind,id,{stepKey,userId:actor.userId,reason:'Original current Sales-only independent responsibility',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
}

async function actualSalesServiceBeforeGoods(noVat=false){
  const z=await zeroLedgerSalesCompany();await z.purchase();const refs=await zeroSalesReferences(z),api=refs.api;
  const draft=await api.prepareSalesPostingDefaults(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:{...refs.defaults,taxAccountId:noVat?undefined:refs.defaults.taxAccountId}},key());
  const approved=await api.decideSalesPostingDefaults(z.manager,String(draft.id),{contextKey:(await zeroSalesEnvelope(z.manager)).contextKey,input:{expectedVersion:1,action:'approve',reason:'Original independent reviewer approves actual service defaults before the goods extension'}},key());
  const customer=await saveMaster(z.owner,'parties',null,{kind:'customer',code:key(),name:'Actual pre-existing service customer'},key()),service=await saveMaster(z.owner,'products',null,{sku:key(),name:'Actual pre-existing installed service',itemType:'service',price:'100',taxBps:noVat?0:700},key()),order=await saveOrder(z.owner,null,orderSchema.parse({kind:'sales',partyId:customer.id,orderDate:'2026-09-02',dueDate:'2026-09-10',lines:[{productId:service.id,quantity:'1',price:'100',taxBps:noVat?0:700}]}),key()),orderId=String(order.id);
  await transitionOrder(z.owner,orderId,{expectedVersion:1,action:'submit'},key());await transitionOrder(z.salesManager,orderId,{expectedVersion:2,action:'approve'},key());await assignZeroSales(z,'order',orderId,'confirmed',z.owner);
  const lineId=String((await getOrder(z.owner,orderId)).lines[0].id),completion=await fulfillOrder(z.owner,orderId,fulfillmentSchema.parse({expectedVersion:3,date:'2026-09-03',completionMode:'service',evidenceNote:'Original customer accepted the actual pre-existing service',lines:[{lineId,quantity:'1'}]}),key());
  await assignZeroSales(z,'order',orderId,'invoicing',z.owner);
  const invoiced=await invoiceFulfillment(z.owner,String(completion.fulfillmentId),{expectedVersion:1,date:'2026-09-04',dueDate:'2026-09-10'},key()),invoiceId=String(invoiced.invoiceId);
  await assignZeroSales(z,'invoice',invoiceId,'draft',z.manager);await postInvoice(z.manager,invoiceId,{expectedVersion:1,periodId:refs.period.id,entryDate:'2026-09-04'},key());await assignZeroSales(z,'invoice',invoiceId,'posted',z.treasury);
  const paid=await settleInvoice(z.treasury,invoiceId,{expectedVersion:2,date:'2026-09-05',amount:'50',bankAccountId:refs.accounts.cashAccountId,reference:'Actual service customer payment predates the Goods mapping'},key());
  const mapping={receivableAccountId:refs.accounts.receivableAccountId,revenueAccountId:refs.accounts.revenueAccountId,outputTaxAccountId:refs.accounts.taxAccountId,cashAccountId:refs.accounts.cashAccountId,payableAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:''};
  for(const [field,category,controlKind] of [['payableAccountId','liability','payable'],['inventoryAccountId','asset','none'],['clearingAccountId','liability','none'],['cogsAccountId','expense','none'],['inputTaxAccountId','asset','none']] as const){
    const account=await api.createSalesPostingAccount(z.reviewer,{...await zeroSalesEnvelope(z.reviewer),input:{chartId:refs.chart.id,code:key(),name:`Actual initial Goods ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());mapping[field]=account.id;
  }
  const serviceHistory=()=>Promise.all([...['commerce_orders','finance_charts','finance_accounts','finance_sales_defaults','finance_invoices','finance_journals','finance_customer_receipts','finance_financial_events','money_transaction_snapshots'].map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE company_id=? ORDER BY image`).bind(z.company).all()).results),db.prepare('SELECT to_jsonb(r)::text image FROM commerce_fulfillments r JOIN commerce_orders o ON o.id=r.order_id WHERE o.company_id=? ORDER BY image').bind(z.company).all().then(rows=>rows.results),db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? ORDER BY image').bind(z.company).all().then(rows=>rows.results)]);
  expect(await db.prepare('SELECT count(*) n FROM commerce_settings WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM commerce_warehouses WHERE company_id=?').bind(z.company).first('n')).toBe(0);
  return {z,refs,mapping,approved,orderId,invoiceId,fulfillmentId:String(completion.fulfillmentId),paymentId:String(paid.paymentId),serviceHistory};
}

async function companyServiceCashFixture(twoBanks=false){
  const s=await actualSalesServiceBeforeGoods(),finance=await import('@erpx/server-finance/public/finance');
  const banks=[s.refs.accounts.cashAccountId],payments=[s.paymentId];
  let reversedBank:string|null=null,reversedPayment:string|null=null;
  async function createBank(name:string){
    return finance.createAccount(s.z.reviewer,{chartId:s.refs.chart.id,code:key(),name,category:'asset',currency:'THB',isControl:true,controlKind:'bank'},key());
  }
  if(twoBanks){
    for(const [name,amount] of [['Second actual original partial receipt bank','25'],['Actually reversed original receipt bank','10']] as const){
      const bank=await createBank(name),version=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(s.invoiceId).first('version'));
      const payment=await settleInvoice(s.z.treasury,s.invoiceId,{expectedVersion:version,date:'2026-09-05',amount,bankAccountId:bank.id,reference:name},key());
      if(amount==='10'){
        const reverseKey=key(),body={expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-06',reason:'Independent actual reviewer corrects the third original receipt before crediting the service'};
        const result=await finance.reversePayment(s.z.manager,String(payment.paymentId),body,reverseKey);
        expect(await finance.reversePayment(s.z.manager,String(payment.paymentId),body,reverseKey)).toEqual(result);
        expect(await db.prepare('SELECT status FROM finance_payments WHERE id=?').bind(payment.paymentId).first('status')).toBe('voided');
        reversedBank=bank.id;reversedPayment=String(payment.paymentId);
      }else{banks.push(bank.id);payments.push(String(payment.paymentId));}
    }
  }
  const detail=await getOrder(s.z.owner,s.orderId),returned=await requestCommerceReturn(s.z.owner,s.fulfillmentId,returnRequestSchema.parse({expectedVersion:2,date:'2026-09-06',reason:'Actual Company service customer independently accepts the whole credit',lines:[{lineId:String(detail.fulfillmentLines[0].id),quantity:'1'}]}),key());
  const reviewed=await decideCommerceReturn(s.z.salesManager,String(returned.id),'approve',{expectedVersion:1,reason:'Different actual Sales manager reviews the original completed service'},key());
  expect(reviewed.stockJournalId).toBeUndefined();await assignZeroSales(s.z,'journal',String(reviewed.creditJournalId),'draft',s.z.manager);await postJournal(s.z.manager,String(reviewed.creditJournalId),key(),{expectedVersion:1});
  const completed=await completeCommerceReturn(s.z.manager,String(returned.id),{expectedVersion:2},key()),refundId=String(completed.refundId),refunds=await import('@erpx/server-finance/public/refunds');
  expect(completed.refundMinor).toBe(twoBanks?7500:5000);
  const reviewer=await s.z.actor('independent-cash-source-reviewer','finance_approver');
  await refunds.approveRefund(reviewer,refundId,{expectedVersion:1,beneficiaryName:'Original fictional service customer',beneficiaryAccount:'FICTIONAL-SERVICE-REFUND',reason:'Independent original credit and settled receipt review'},key());
  const input=async(cashAccountId=banks[0],amount='1')=>({expectedVersion:Number((await refunds.refundWorkspace(s.z.treasury,refundId)).owner.version),amount,plannedDate:'2026-09-07',bankAccount:cashAccountId,cashAccountId});
  return {...s,refundId,refunds,banks,payments,reversedBank,reversedPayment,createBank,input};
}

async function serviceRefundBankBody(s:Awaited<ReturnType<typeof companyServiceCashFixture>>,instructionId:string,amount:string){
  const version=Number(await db.prepare('SELECT version FROM finance_refund_instructions WHERE id=?').bind(instructionId).first('version'));
  return {expectedVersion:version,bankAccount:s.banks[0],statementNumber:`FICTIONAL-NAMED-${key()}`,statementDate:'2026-09-07',opening:'100',closing:(100-Number(amount)).toFixed(2),sourceHash:key(),rows:[{externalId:`FICTIONAL-NAMED-ROW-${key()}`,transactionDate:'2026-09-07',description:'Actual bank observation for the exact original exported instruction',amount:`-${amount}`,currency:'THB'}]};
}

it('rejects invalid populated pre1057 bank consumption atomically and upgrades normal corrected history without rewriting rows',async()=>{
 const oldRoot=await mkdtemp(resolve('.runtime/tests/refund-bank-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/refund-bank-current-')),directory=await mkdtemp(resolve('.runtime/tests/refund-bank-data-'));
 const adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');
 if(!['postgresql:','postgres:'].includes(adminUrl.protocol)||adminUrl.search||adminUrl.hash||adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 const name=`erpx_test_refund_bank_${key().replaceAll('-','')}`,admin=new Pool({connectionString:adminUrl.href,max:1});let created=false;
 try{
  const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];
  if(role?.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw Error('UNSAFE_TEST_DATABASE_ROLE');
  await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);created=true;const url=new URL(adminUrl);url.pathname=name;db=new PostgresDatabase({connectionString:url.href,max:4});files=new LocalFileStore(join(directory,'files'));
  for(const root of [oldRoot,currentRoot])await mkdir(join(root,'holding-write'),{recursive:true});
  const source=resolve('migrations/postgres/holding-write');
  for(const file of (await readdir(source)).filter(file=>file.endsWith('.sql')&&Number(file.slice(0,4))<=1057)){
   await cp(join(source,file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1056)await cp(join(source,file),join(oldRoot,'holding-write',file));
  }
  await migrateDatabase(db,'holding-write',oldRoot);
  expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1057_%'").first('n')).toBe(0);expect(await nativeGuardObjects()).toEqual([[],[]]);
  // Original fictional Holding identity bootstrap only. Ledger, Company quote,
  // defaults, Service sources and bank evidence below use normal owner commands.
  await db.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,'h001','fixture',1) ON CONFLICT(singleton) DO NOTHING").run();
  const s=await companyServiceCashFixture(),refunds=await import('@erpx/server-finance/public/refunds'),finance=await import('@erpx/server-finance/public/finance');
  await s.z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...s.z.grant,packageId:'finance',version:1,reason:'Separate actual generic Bank populated migration control',evidence:'test:refund-bank-pre1057-upgrade'}});
  const prepared=await refunds.prepareRefundInstruction(s.z.treasury,s.refundId,await s.input(s.banks[0],'50'),key()),instructionId=String(prepared.id);
  await refunds.exportRefundInstructions(s.z.treasury,{instructions:[{id:instructionId,expectedVersion:1}]},key());
  const importKey=key(),importBody=await serviceRefundBankBody(s,instructionId,'50'),evidence=await refunds.importRefundBankEvidence(s.z.treasury,instructionId,importBody,importKey);
  const supplier=await finance.createParty(s.z.reviewer,{kind:'supplier',code:key(),name:'Actual populated migration supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
  const invoice=await finance.createInvoice(s.z.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:`FICTIONAL-MIGRATION-BILL-${key()}`,invoiceDate:'2026-09-04',dueDate:'2026-09-10',currency:'THB',subtotal:'50',tax:'0',debitAccountId:s.mapping.cogsAccountId,creditAccountId:s.mapping.payableAccountId},key()),invoiceId=String(invoice.id);
  await assignZeroSales(s.z,'invoice',invoiceId,'draft',s.z.manager);await postInvoice(s.z.manager,invoiceId,{expectedVersion:1,periodId:s.refs.period.id,entryDate:'2026-09-04'},key());
  async function releasedPayment(label:string){
   const payment=await finance.createPayment(s.z.reviewer,{partyId:String(supplier.id),kind:'payment',paymentNumber:`FICTIONAL-MIGRATION-${label}-${key()}`,paymentDate:'2026-09-06',currency:'THB',amount:'50',bankAccount:s.banks[0]},key()),id=String(payment.id);
   await finance.approvePayment(s.z.manager,id,{expectedVersion:1,proof:'Different actual Finance approver checked the supplier obligation'},key());await finance.releasePayment(s.z.treasury,id,{expectedVersion:2,proof:'Original Treasury released the actual supplier payment'},key());return id;
  }
  function paymentConfirmation(bankVersion:number){return {expectedVersion:3,periodId:s.refs.period.id,bankTransactionId:evidence.bankTransactionId,bankVersion,bankReference:'FICTIONAL-MIGRATION-CROSS-CONSUMER',proof:'Genuine supplier payment must not consume the other exported Refund instruction evidence',allocations:[{invoiceId,amount:'50'}]};}
  const paymentB=await releasedPayment('B'),commandB=key(),bodyB=paymentConfirmation(1),wronglySettled=await finance.confirmPayment(s.z.treasury,paymentB,bodyB,commandB);
  expect(wronglySettled).toMatchObject({id:paymentB,status:'settled',version:4});expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(evidence.bankTransactionId).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:paymentB});
  expect(await db.prepare('SELECT status,version FROM finance_refund_instructions WHERE id=?').bind(instructionId).first()).toEqual({status:'exported',version:2});
  // The invalid populated row is created by the real old normal workflow. A
  // failed additive upgrade may neither grandfather nor repair its history.
  const invalidRows=await allRows(),invalidSchema=await schemaImage(),oldReceipts=await migrationReceipts();let failedUpgrade:unknown;
  try{await migrateDatabase(db,'holding-write',currentRoot);}catch(error){failedUpgrade=error;}
  expect(native(failedUpgrade)).toMatchObject({code:'23514',constraint:'finance_refund_bank_evidence_consumer',message:'REFUND_BANK_PROVENANCE_REQUIRED'});
  expect(await allRows()).toEqual(invalidRows);expect(await schemaImage()).toEqual(invalidSchema);expect(await migrationReceipts()).toEqual(oldReceipts);expect(await nativeGuardObjects()).toEqual([[],[]]);
  // SupplierB really has an invoice allocation, so its original independent
  // correction is a legitimate reset. Do not SQL-unmatch or reverse RefundA.
  const reverseKey=key(),reverseBody={expectedVersion:4,periodId:s.refs.period.id,entryDate:'2026-09-08',reason:'Independent reviewer corrects the old misconsumed Supplier payment while retaining its original history'},reversed=await finance.reversePayment(s.z.manager,paymentB,reverseBody,reverseKey);
  expect(await finance.reversePayment(s.z.manager,paymentB,reverseBody,reverseKey)).toEqual(reversed);expect(await db.prepare('SELECT status,version FROM finance_payments WHERE id=?').bind(paymentB).first()).toEqual({status:'voided',version:5});
  const restoredBank=await db.prepare('SELECT match_status,matched_type,matched_id,version FROM finance_bank_transactions WHERE id=?').bind(evidence.bankTransactionId).first<{match_status:string;matched_type:string|null;matched_id:string|null;version:number}>();
  expect(restoredBank).toEqual({match_status:'unmatched',matched_type:null,matched_id:null,version:3});expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'posted',outstanding_minor:5000});
  const correctedRows=await allRows(),correctedSchema=await schemaImage();await migrateDatabase(db,'holding-write',currentRoot);
  expect(await allRows()).toEqual(correctedRows);expect((await migrationReceipts()).filter(row=>!row.name.startsWith('1057_'))).toEqual(oldReceipts);expect((await migrationReceipts()).filter(row=>row.name.startsWith('1057_'))).toHaveLength(1);
  const currentSchema=await schemaImage();expect({...currentSchema,functions:currentSchema.functions.filter(row=>!['finance_assert_refund_bank_consumer','guard_finance_refund_bank_consumer'].includes(row.name)),triggers:currentSchema.triggers.filter(row=>row.name!=='finance_refund_bank_evidence_consumer'),constraints:currentSchema.constraints.filter(row=>row.name!=='finance_refund_bank_evidence_consumer')}).toEqual(correctedSchema);
  const installed=await nativeGuardObjects();expect(installed[0]).toHaveLength(2);expect(installed[1]).toHaveLength(1);
  // A new released payment and still-unmatched row avoid both historical replay
  // and already-matched guards masking the new shared native provenance check.
  const paymentC=await releasedPayment('C'),commandC=key(),beforeC=await allRows();
  await expect(finance.confirmPayment(s.z.treasury,paymentC,paymentConfirmation(3),commandC)).rejects.toMatchObject({code:'REFUND_BANK_PROVENANCE_REQUIRED'});expect(await allRows()).toEqual(beforeC);
  const confirmer=await s.z.actor('populated-upgrade-refund-confirmer','treasury'),confirmation={expectedVersion:2,bankTransactionId:evidence.bankTransactionId,bankVersion:3,periodId:s.refs.period.id,bankReference:'FICTIONAL-MIGRATION-ORIGINAL-REFUND',evidence:'Actual independent Treasury checks the original exported Refund after correcting the unrelated Supplier payment',attachmentSetIds:[]},confirmationKey=key();
  const confirmed=await refunds.confirmRefundInstruction(confirmer,instructionId,confirmation,confirmationKey);expect(await refunds.confirmRefundInstruction(confirmer,instructionId,confirmation,confirmationKey)).toEqual(confirmed);
  expect(await db.prepare('SELECT status,outstanding_minor FROM finance_refund_obligations WHERE id=?').bind(s.refundId).first()).toEqual({status:'settled',outstanding_minor:0});expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(evidence.bankTransactionId).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:confirmed.paymentId});
  expect(await refunds.importRefundBankEvidence(s.z.treasury,instructionId,importBody,importKey)).toEqual(evidence);
  const history=await allRows();expect(await finance.confirmPayment(s.z.treasury,paymentB,bodyB,commandB)).toEqual(wronglySettled);expect(await allRows()).toEqual(history);
 }finally{
  if(db)await db.close();
  if(created){const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[name])).rows[0]?.owner;if(owner!=='erpx_test_admin')throw Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${name}"`);}
  await admin.end();await Promise.all([oldRoot,currentRoot,directory].map(path=>rm(path,{recursive:true,force:true})));vi.unstubAllEnvs();
 }
},180000);

// Ignored ONE1058 migration/restore candidate. Append only after the exact
// existing05a07355 migration-file prefix; all original helpers/cases stay byte exact.
it('preserves populated pre1058 manual Count history and restored source while admitting only reciprocal new Initial Physical Stock provenance',async()=>{
 const {execFile}=await import('node:child_process'),{promisify,isDeepStrictEqual}=await import('node:util'),{chmod,writeFile}=await import('node:fs/promises'),run=promisify(execFile),pgDump='/opt/homebrew/opt/postgresql@18/bin/pg_dump',pgRestore='/opt/homebrew/opt/postgresql@18/bin/pg_restore';
 const oldRoot=await mkdtemp(resolve('.runtime/tests/initial-stock-before-')),currentRoot=await mkdtemp(resolve('.runtime/tests/initial-stock-current-')),directory=await mkdtemp(resolve('.runtime/tests/initial-stock-data-')),adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL??'postgresql://erpx_test_admin@127.0.0.1:5432/postgres');
 if(!['postgresql:','postgres:'].includes(adminUrl.protocol)||adminUrl.search||adminUrl.hash||adminUrl.hostname!=='127.0.0.1'||adminUrl.port!=='5432'||adminUrl.username!=='erpx_test_admin'||adminUrl.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');
 const prefix=`erpx_test_initial_stock_${key().replaceAll('-','')}`,beforeName=`${prefix}_before`,restoreName=`${prefix}_restore`,admin=new Pool({connectionString:adminUrl.href,max:1}),databases:PostgresDatabase[]=[],created:string[]=[];
 async function createOwned(name:string){if(!name.startsWith(prefix+'_'))throw Error('ISOLATED_DATABASE_REQUIRED');await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);created.push(name);const url=new URL(adminUrl);url.pathname=name;const database=new PostgresDatabase({connectionString:url.href,max:4});databases.push(database);return database;}
 // Compare actual effective grants, including grantors/options, rather than
 // PostgreSQL's nullable/default ACL-array representation or array ordering.
 async function canonicalSchemaImage(){
  const schema=await schemaImage(),[relations,functions]=await Promise.all([
   db.prepare("SELECT c.relname name,COALESCE((SELECT jsonb_agg(jsonb_build_object('grantor',pg_get_userbyid(a.grantor),'grantee',CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,'privilege',a.privilege_type,'grantable',a.is_grantable) ORDER BY pg_get_userbyid(a.grantor),CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,a.privilege_type,a.is_grantable)::text FROM aclexplode(NULLIF(COALESCE(c.relacl,CASE WHEN c.relkind IN ('r','p','v','m','f','S') THEN acldefault(CASE WHEN c.relkind='S' THEN 's'::\"char\" ELSE 'r'::\"char\" END,c.relowner) ELSE '{}'::aclitem[] END),'{}'::aclitem[])) a),'[]') acl FROM pg_class c WHERE c.relnamespace='public'::regnamespace ORDER BY c.relname").all<{name:string;acl:string}>(),
   db.prepare("SELECT p.proname name,pg_get_function_identity_arguments(p.oid) arguments,COALESCE((SELECT jsonb_agg(jsonb_build_object('grantor',pg_get_userbyid(a.grantor),'grantee',CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,'privilege',a.privilege_type,'grantable',a.is_grantable) ORDER BY pg_get_userbyid(a.grantor),CASE WHEN a.grantee=0 THEN 'PUBLIC' ELSE pg_get_userbyid(a.grantee) END,a.privilege_type,a.is_grantable)::text FROM aclexplode(NULLIF(COALESCE(p.proacl,acldefault('f'::\"char\",p.proowner)),'{}'::aclitem[])) a),'[]') acl FROM pg_proc p WHERE p.pronamespace='public'::regnamespace AND p.prokind IN ('f','p') ORDER BY p.proname,pg_get_function_identity_arguments(p.oid)").all<{name:string;arguments:string;acl:string}>(),
  ]),relationAcls=new Map(relations.results.map(row=>[row.name,row.acl])),functionAcls=new Map(functions.results.map(row=>[`${row.name}:${row.arguments}`,row.acl]));
  return {...schema,relations:schema.relations.map((row):Record<string,unknown>&{acl:string|undefined}=>({...row,acl:relationAcls.get(String(row.name))})),functions:schema.functions.map(row=>({...row,acl:functionAcls.get(`${row.name}:${row.arguments}`)}))};
 }
 async function cacheBaselines(){return (await db.prepare('SELECT table_name,revision::text revision FROM work_cache_revision_baselines ORDER BY table_name').all<{table_name:string;revision:string}>()).results;}
 async function definitions(){return {schema:await canonicalSchemaImage(),columns:(await db.prepare("SELECT table_name relation,column_name name,ordinal_position position,udt_name type,is_nullable nullable,column_default default_value,is_identity identity,is_generated generated FROM information_schema.columns WHERE table_schema='public' ORDER BY table_name,ordinal_position").all<{relation:string;name:string;position:number;type:string;nullable:string;default_value:string|null;identity:string;generated:string}>()).results,sequences:(await db.prepare("SELECT sequencename name,last_value::text value FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename").all<{name:string;value:string|null}>()).results};}
 function preserveOldDefinitions(old:Awaited<ReturnType<typeof definitions>>,after:Awaited<ReturnType<typeof definitions>>){
  const relations=new Set(old.schema.relations.map(row=>String(row.name))),functions=new Set(old.schema.functions.map(row=>`${row.name}:${row.arguments}`)),triggers=new Set(old.schema.triggers.map(row=>`${row.relation}:${row.name}`)),constraints=new Set(old.schema.constraints.map(row=>`${row.relation}:${row.name}`)),columns=new Set(old.columns.map(row=>`${row.relation}:${row.name}`));
  expect(after.schema.relations.filter(row=>relations.has(String(row.name)))).toEqual(old.schema.relations);expect(after.schema.functions.filter(row=>functions.has(`${row.name}:${row.arguments}`))).toEqual(old.schema.functions);expect(after.schema.triggers.filter(row=>triggers.has(`${row.relation}:${row.name}`))).toEqual(old.schema.triggers);expect(after.schema.constraints.filter(row=>constraints.has(`${row.relation}:${row.name}`))).toEqual(old.schema.constraints);expect(after.columns.filter(row=>columns.has(`${row.relation}:${row.name}`))).toEqual(old.columns);expect(after.sequences).toEqual(old.sequences);
 }
 function firstDefinitionDifference(left:unknown,right:unknown,location='$'):unknown{
  if(isDeepStrictEqual(left,right))return null;
  const description=(value:unknown)=>({type:typeof value,tag:Object.prototype.toString.call(value),prototype:value!==null&&typeof value==='object'?Object.getPrototypeOf(value)?.constructor?.name??null:null});
  const detail={path:location,leftDescription:description(left),rightDescription:description(right)};
  if(left===null||right===null||typeof left!=='object'||typeof right!=='object')return {...detail,left,right};
  if(Object.getPrototypeOf(left)!==Object.getPrototypeOf(right))return {...detail,samePrototype:false};
  const leftKeys=Reflect.ownKeys(left),rightKeys=Reflect.ownKeys(right);
  if(!isDeepStrictEqual(leftKeys.map(String).sort(),rightKeys.map(String).sort()))return {...detail,leftKeys:leftKeys.map(String),rightKeys:rightKeys.map(String)};
  for(const name of leftKeys){const difference=firstDefinitionDifference(Reflect.get(left,name),Reflect.get(right,name),`${location}.${String(name)}`);if(difference!==null)return difference;}
  return {...detail,left,right};
 }
 async function assertRestoredDefinitions(old:Awaited<ReturnType<typeof definitions>>,restored:Awaited<ReturnType<typeof definitions>>,originalDb:PostgresDatabase){
  const oldConstraints=old.schema.constraints,restoredConstraints=restored.schema.constraints;
  expect({...restored,schema:{...restored.schema,constraints:[]}}).toEqual({...old,schema:{...old.schema,constraints:[]}});
  expect(restoredConstraints.map(row=>({...row,definition:null}))).toEqual(oldConstraints.map(row=>({...row,definition:null})));
  const mismatches=oldConstraints.flatMap((row,index)=>row.definition===restoredConstraints[index].definition?[]:[index]),originalCanonical=oldConstraints.map(row=>({...row})),restoredCanonical=restoredConstraints.map(row=>({...row})),rowsBefore=await allRows(),receiptsBefore=await migrationReceipts(),temporaryNames:string[]=[],rollback=Error('ROLLBACK_TEMPORARY_CHECK_OBSERVER');
  const quote=(name:string)=>'"'+name.replaceAll('"','""')+'"',isCheck="SELECT k.contype='c' AS is_check FROM pg_constraint k JOIN pg_class r ON r.oid=k.conrelid WHERE k.connamespace='public'::regnamespace AND r.relname=? AND k.conname=?";
  try{await db.transaction(async tx=>{
   for(const index of mismatches){
    const row=oldConstraints[index];expect(await originalDb.prepare(isCheck).bind(row.relation,row.name).first('is_check')).toBe(true);expect(await tx.prepare(isCheck).bind(row.relation,row.name).first('is_check')).toBe(true);
    for(const [side,definition] of [['original',row.definition],['restored',restoredConstraints[index].definition]] as const){
     const temporary=`initial_check_${key().replaceAll('-','')}`;temporaryNames.push(temporary);
     await tx.exec(`CREATE TEMP TABLE ${quote(temporary)} (LIKE public.${quote(row.relation)}) ON COMMIT DROP;ALTER TABLE pg_temp.${quote(temporary)} ADD CONSTRAINT native_check ${definition}`);
     const canonical=await tx.prepare("SELECT pg_get_constraintdef(oid) definition FROM pg_constraint WHERE conrelid=to_regclass(?) AND conname='native_check'").bind(`pg_temp.${quote(temporary)}`).first<string>('definition');if(!canonical)throw Error('ACTUAL_NATIVE_CHECK_DEFINITION_REQUIRED');
     (side==='original'?originalCanonical:restoredCanonical)[index].definition=canonical;
    }
   }
   throw rollback;
  });}catch(error){if(error!==rollback)throw error;}finally{
   expect(await allRows()).toEqual(rowsBefore);expect(await migrationReceipts()).toEqual(receiptsBefore);expect(await definitions()).toEqual(restored);
   for(const temporary of temporaryNames)expect(await db.prepare('SELECT count(*)::int n FROM pg_class WHERE relname=?').bind(temporary).first('n')).toBe(0);
  }
  // Native PG18 parse/deparse comparison for differing CHECK text only.
  // Every unchanged definition, object field, ACL, column and sequence stays exact.
  expect({...restored,schema:{...restored.schema,constraints:restoredCanonical}}).toEqual({...old,schema:{...old.schema,constraints:originalCanonical}});
 }
 try{
  const role=(await admin.query('SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user')).rows[0];if(role?.name!=='erpx_test_admin'||role.rolsuper||!role.rolcreatedb||role.rolcreaterole)throw Error('UNSAFE_TEST_DATABASE_ROLE');
  const sourceDb=await createOwned(beforeName);db=sourceDb;files=new LocalFileStore(join(directory,'files'));
  for(const root of [oldRoot,currentRoot])await mkdir(join(root,'holding-write'),{recursive:true});
  for(const file of (await readdir(resolve('migrations/postgres/holding-write'))).filter(file=>file.endsWith('.sql')&&Number(file.slice(0,4))<=1058)){await cp(resolve('migrations/postgres/holding-write',file),join(currentRoot,'holding-write',file));if(Number(file.slice(0,4))<=1057)await cp(resolve('migrations/postgres/holding-write',file),join(oldRoot,'holding-write',file));}
  await migrateDatabase(db,'holding-write',oldRoot);expect(await db.prepare("SELECT count(*)::int n FROM erpx_migrations WHERE name LIKE '1058_%'").first('n')).toBe(0);expect(await db.prepare("SELECT to_regclass('inventory_initial_physical_stock_sources') name").first('name')).toBeNull();expect((await db.prepare("SELECT proname FROM pg_proc WHERE pronamespace='public'::regnamespace AND proname LIKE 'inventory_%initial%physical%' ORDER BY proname").all()).results).toEqual([]);
  await db.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,'h001','fixture',1) ON CONFLICT(singleton) DO NOTHING").run();
  const s=await actualSalesServiceBeforeGoods(),initial=await import('@erpx/server-inventory/public/initial-physical-stock-count'),inventory=await import('@erpx/server-inventory/public/inventory-counts'),goods=await import('@erpx/server-commerce/public/sales-goods-posting-setup');
  const variance=await s.refs.api.createSalesPostingAccount(s.z.reviewer,{...await zeroSalesEnvelope(s.z.reviewer),input:{chartId:s.refs.chart.id,code:'5400',name:'Real pre-upgrade current-period Stock variance',category:'expense',currency:'THB',isControl:false,controlKind:'none'}},key()),mapping=await goods.salesGoodsPostingSetup(s.z.reviewer);if(!mapping.contextKey)throw Error('Actual current Goods mapping context required');
  expect(await db.prepare('SELECT category,currency,status,control_kind FROM finance_accounts WHERE id=? AND company_id=?').bind(variance.id,s.z.company).first()).toEqual({category:'expense',currency:'THB',status:'active',control_kind:'none'});
  await goods.initializeSalesGoodsPostingSetup(s.z.reviewer,{expectedVersion:0,contextKey:mapping.contextKey,expectedSalesDefaults:mapping.expectedSalesDefaults,chartId:s.refs.chart.id,currency:'THB',reason:'Normal reviewed Goods mapping for populated manual Count migration control',...s.mapping},key());
  // Explicit separate lifecycle control: this Company really buys Supply to
  // create an original manual Count. It is NOT Sales-only purchase acceptance.
  await s.z.store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...s.z.grant,packageId:'supply',version:1,reason:'Declared real original manual Supply Count history control',evidence:'test:pre1058-manual-count-migration'}});
  const warehouse=await saveMaster(s.z.owner,'warehouses',null,{branchId:s.z.branch,code:key(),name:'Actual populated pre1058 warehouse'},key()),manualProduct=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Actual old manually counted item',itemType:'stock',price:'10',taxBps:700},key()),freshProduct=await saveMaster(s.z.owner,'products',null,{sku:key(),name:'Actual untouched initial Stock item',itemType:'stock',price:'10',taxBps:700},key()),manualKey=key(),manualInput=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate:'2026-09-06',reason:'Original normal Supply count independently verifies real current-period units',lines:[{productId:String(manualProduct.id),countedQuantity:'2',unitCost:'6'}]}),manual=await inventory.saveCount(s.z.owner,null,manualInput,manualKey),manualId=String(manual.id);
  await inventory.transitionCount(s.z.owner,manualId,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
  async function originalAssign(stepKey:string,recordVersion:number){const flow=await resolveFlow(db,s.z.owner,'stock_count',manualId);expect(flow.recordVersion).toBe(recordVersion);await assignFlowPrimary(db,s.z.owner,'stock_count',manualId,{stepKey,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Original independent manual Stock reviewer before1058',saveDefault:false},key());}
  await originalAssign('submitted',2);await inventory.transitionCount(s.z.manager,manualId,'approve',{expectedVersion:2,reason:'Independent original manual Stock quantity and cost review',attachmentSetIds:[]},key());await originalAssign('approved',3);
  const manualPostKey=key(),manualPostBody={expectedVersion:3,reason:'Original current-period manual Count posting remains original history',attachmentSetIds:[]},manualPosted=await inventory.transitionCount(s.z.manager,manualId,'post',manualPostBody,manualPostKey);expect(manualPosted).toMatchObject({id:manualId,status:'posted',version:4});expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,manualProduct.id).first()).toEqual({quantity_milli:2000,value_minor:1200});
  const selection={warehouseId:String(warehouse.id),productId:String(freshProduct.id),countDate:'2026-09-06'},workspace=await initial.initialPhysicalStockCountWorkspace(s.z.owner,selection);if(!workspace.contextKey)throw Error('Actual native selected context required before schema-gated command');
  const initialBody={expectedVersion:0 as const,contextKey:workspace.contextKey,input:{warehouseId:selection.warehouseId,countDate:selection.countDate,reason:'Verified independent first physical count of three actual units at seven THB in the current period',lines:[{productId:selection.productId,countedQuantity:'3',unitCost:'7',allocations:[]}]}};
  const missingKey=key(),missingBefore=await allRows();await expect(initial.prepareInitialPhysicalStockCount(s.z.owner,initialBody,missingKey)).rejects.toMatchObject({status:503,code:'INITIAL_PHYSICAL_STOCK_SCHEMA_REQUIRED'});expect(await allRows()).toEqual(missingBefore);
  const retained=await allRows(),oldNames=new Set(retained.map(row=>row.table)),oldReceipts=await migrationReceipts(),oldBaselines=await cacheBaselines(),oldDefinitions=await definitions(),archive=join(directory,'populated-pre1058.dump'),cliEnv={...process.env,PGHOST:adminUrl.hostname,PGPORT:adminUrl.port,PGUSER:adminUrl.username,PGPASSWORD:decodeURIComponent(adminUrl.password),PGDATABASE:beforeName};
  expect((await run(pgDump,['--version'],{env:cliEnv})).stdout).toContain('18.6');expect((await run(pgRestore,['--version'],{env:cliEnv})).stdout).toContain('18.6');await run(pgDump,['--format=custom','--no-owner','--file',archive],{env:cliEnv,timeout:120000});await chmod(archive,0o600);
  // Preserve ACLs under the same restricted owner in the isolated restore.
  // Restore into an independently created EMPTY owned DB. --clean on an
  // upgraded DB would not know how to remove newly added1058 objects.
  const restored=await createOwned(restoreName);await run(pgRestore,['--exit-on-error','--transaction-size=64','--no-owner','--dbname',restoreName,archive],{env:{...cliEnv,PGDATABASE:restoreName},timeout:120000});
  await migrateDatabase(sourceDb,'holding-write',currentRoot);
  // The normal runner adds coverage for exactly this new tracked relation:
  // default revision0 + DML trigger1 + TRUNCATE trigger1 (original1038).
  const newTable='inventory_initial_physical_stock_sources',newBaselines=await cacheBaselines();
  expect(oldBaselines.some(row=>row.table_name===newTable)).toBe(false);
  expect(newBaselines.filter(row=>row.table_name!==newTable)).toEqual(oldBaselines);
  expect(newBaselines.filter(row=>row.table_name===newTable)).toEqual([{table_name:newTable,revision:'2'}]);
  expect((await allRows()).filter(row=>oldNames.has(row.table)&&row.table!=='work_cache_revision_baselines')).toEqual(retained.filter(row=>row.table!=='work_cache_revision_baselines'));
  expect((await migrationReceipts()).filter(row=>!row.name.startsWith('1058_'))).toEqual(oldReceipts);expect((await migrationReceipts()).filter(row=>row.name.startsWith('1058_'))).toHaveLength(1);preserveOldDefinitions(oldDefinitions,await definitions());expect(await db.prepare('SELECT count(*)::int n FROM inventory_initial_physical_stock_sources').first('n')).toBe(0);
  const newWorkspace=await initial.initialPhysicalStockCountWorkspace(s.z.owner,selection);if(!newWorkspace.contextKey)throw Error('Actual post-upgrade selected context required');const body={...initialBody,contextKey:newWorkspace.contextKey},createKey=key(),prepared=await initial.prepareInitialPhysicalStockCount(s.z.owner,body,createKey),id=String(prepared.id);expect(prepared).toMatchObject({status:'draft',version:1});expect(await initial.prepareInitialPhysicalStockCount(s.z.owner,body,createKey)).toEqual(prepared);
  await initial.transitionInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[]},key());
  async function namedAssign(stepKey:string,recordVersion:number){const flow=await initial.initialPhysicalStockCountFlow(s.z.owner,id);await initial.assignInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion,reason:'Real original Company authority assigns the independent initial Stock reviewer',saveDefault:false},key());}
  await namedAssign('submitted',2);await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,reason:'Independent real post-upgrade Stock quantity/cost review',attachmentSetIds:[]},key());await namedAssign('approved',3);
  const postKey=key(),postBody={expectedVersion:3,reason:'Actual current-period initial Stock variance has complete original owner evidence',attachmentSetIds:[]},posted=await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});expect(await initial.transitionInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,freshProduct.id).first()).toEqual({quantity_milli:3000,value_minor:2100});
  const nativeSource=await db.prepare("SELECT source.proof_json::text source_proof,(receipt.result_json::jsonb->'_initialPhysicalStockCount')::text receipt_proof FROM inventory_initial_physical_stock_sources source JOIN command_receipts receipt ON receipt.command_id=source.count_command_id WHERE source.count_id=? AND receipt.command_id=?").bind(id,createKey).first<{source_proof:string;receipt_proof:string|null}>();if(!nativeSource?.receipt_proof)throw Error('Actual original private create receipt required');const parsedProof:unknown=JSON.parse(nativeSource.receipt_proof);if(!parsedProof||typeof parsedProof!=='object'||Array.isArray(parsedProof))throw Error('Actual native private proof object required');expect(JSON.parse(nativeSource.source_proof)).toEqual(parsedProof);expect(parsedProof).toMatchObject({countId:id,companyId:s.z.company,branchId:s.z.branch,productId:String(freshProduct.id),countCommandId:createKey});const adoptedProof={...parsedProof,countId:manualId,productId:String(manualProduct.id),countCommandId:manualKey},override=JSON.stringify({count_id:manualId,product_id:String(manualProduct.id),count_command_id:manualKey,proof_json:adoptedProof}),intact=await allRows();
  const insertAdoption='INSERT INTO inventory_initial_physical_stock_sources SELECT (jsonb_populate_record(NULL::inventory_initial_physical_stock_sources,to_jsonb(original)||?::jsonb)).* FROM inventory_initial_physical_stock_sources original WHERE count_id=?';
  let sourceOnlyFailure:unknown;try{await db.prepare(insertAdoption).bind(override,id).run();}catch(error){sourceOnlyFailure=error;}expect(native(sourceOnlyFailure)).toMatchObject({code:'23514',message:'INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await allRows()).toEqual(intact);
  // Adverse metadata facts only: even mutually matching forged provenance may
  // not retroactively turn an ordinary creation receipt into a private source.
  let simultaneousFailure:unknown,sourceInserted=false,birthAttempted=false;try{await db.transaction(async tx=>{await tx.prepare(insertAdoption).bind(override,id).run();sourceInserted=!!await tx.prepare('SELECT 1 FROM inventory_initial_physical_stock_sources WHERE count_id=?').bind(manualId).first();birthAttempted=true;await tx.prepare("UPDATE command_receipts SET result_json=jsonb_set(result_json::jsonb,'{_initialPhysicalStockCount}',?::jsonb)::text WHERE command_id=?").bind(JSON.stringify(adoptedProof),manualKey).run();});}catch(error){simultaneousFailure=error;}
  expect(sourceInserted).toBe(true);expect(birthAttempted).toBe(true);expect(native(simultaneousFailure)).toMatchObject({code:'23514',message:'INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await allRows()).toEqual(intact);
  // Only the old populated snapshot is restored. New initial transactions
  // created after the dump are deliberately NOT claimed to survive rollback.
  db=restored;expect(await allRows()).toEqual(retained);expect(await migrationReceipts()).toEqual(oldReceipts);const restoredDefinitions=await definitions(),diagnosticPath=resolve('.scratch/dev/sales-stock-ingress-next/restore-definitions-diagnostic.json');await mkdir(resolve('.scratch/dev/sales-stock-ingress-next'),{recursive:true});await writeFile(diagnosticPath,JSON.stringify({at:new Date().toISOString(),oldDefinitions,restoredDefinitions,jsonEqual:JSON.stringify(oldDefinitions)===JSON.stringify(restoredDefinitions),nodeDeepStrictEqual:isDeepStrictEqual(oldDefinitions,restoredDefinitions),firstDifference:firstDefinitionDifference(oldDefinitions,restoredDefinitions)},null,2)+'\n',{mode:0o600});await chmod(diagnosticPath,0o600);await assertRestoredDefinitions(oldDefinitions,restoredDefinitions,sourceDb);expect(await db.prepare("SELECT to_regclass('inventory_initial_physical_stock_sources') name").first('name')).toBeNull();expect(await inventory.getCount(s.z.manager,manualId)).toMatchObject({status:'posted',version:4});expect(await inventory.transitionCount(s.z.manager,manualId,'post',manualPostBody,manualPostKey)).toEqual(manualPosted);expect(await allRows()).toEqual(retained);
  // Explicitly adversarial pre1058 marker on the disposable restored branch.
  // Preserve it until ordinary DB disposal; no SQL repair or valid-source claim.
  await db.prepare("UPDATE command_receipts SET result_json=jsonb_set(result_json::jsonb,'{_initialPhysicalStockCount}',?::jsonb)::text WHERE command_id=?").bind(JSON.stringify(adoptedProof),manualKey).run();const badRows=await allRows(),badDefinitions=await definitions(),badReceipts=await migrationReceipts();let badUpgrade:unknown;try{await migrateDatabase(db,'holding-write',currentRoot);}catch(error){badUpgrade=error;}
  expect(native(badUpgrade)).toMatchObject({code:'23514',message:'INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await allRows()).toEqual(badRows);expect(await definitions()).toEqual(badDefinitions);expect(await migrationReceipts()).toEqual(badReceipts);expect(await db.prepare("SELECT to_regclass('inventory_initial_physical_stock_sources') name").first('name')).toBeNull();
 }finally{
  await Promise.all(databases.map(database=>database.close()));for(const name of [...created].reverse()){if(!name.startsWith(prefix+'_'))throw Error('ISOLATED_DATABASE_REQUIRED');const owner=(await admin.query('SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1',[name])).rows[0]?.owner;if(owner!=='erpx_test_admin')throw Error('ISOLATED_DATABASE_OWNER_CHANGED');await admin.query(`DROP DATABASE "${name}"`);}await admin.end();await Promise.all([oldRoot,currentRoot,directory].map(path=>rm(path,{recursive:true,force:true})));vi.unstubAllEnvs();
 }
},180000);
