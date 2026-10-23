/** Fictional companies for actual owner commands through normal browser authentication. */
import {randomBytes,randomUUID} from 'node:crypto';
import {chmod,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {hashPassword} from 'better-auth/crypto';
import {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import type {FileStore} from "@erpx/server-runtime/adapters/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {seedExpenseCompany} from "../../expenses/company";
import {businessRoles} from "@erpx/feature-identity_access/lib/business-roles";
import {product,type ProductId} from '@erpx/contracts/product-registry';
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {createAccount,createAccountSchema,createChart,createChartSchema,createJournal,createJournalSchema,createPeriod,createPeriodSchema,postJournalBatch,createParty,createPayment,importBankStatement} from "@erpx/server-finance/public/finance";
import {prepareAccountingProfile,decideAccountingProfile} from "@erpx/server-finance/public/accounting-profiles";
import {confirmManualRate} from "@erpx/server-finance/public/company-money";
import {createPayGroup,payGroupSchema} from "@erpx/server-people_payroll/public/people-payroll";
import {createTalentRequisition,createTalentCandidate,createTalentApplication,createTalentProposal,createTalentOffer,changeTalentRecord,talentDetail} from "@erpx/server-talent/public/talent";
import {createLearningCourse,createLearningAssignment,changeLearningCourse} from "@erpx/server-learning/public/learning";
import {createBenefitPlan,createBenefitWindow,changeBenefitRecord} from "@erpx/server-benefits/public/benefits";

type Input={run:string;directory:string;control:PostgresDatabase;db:PostgresDatabase;files:FileStore;workspaceProducts?:readonly ProductId[]};
export type EngineeringMoneyPeopleUser={key:string;email:string;userId:string;role:string;assignmentId:string;workspaceBase:string};
export type EngineeringMoneyPeopleCompany={companyId:string;users:Record<string,EngineeringMoneyPeopleUser>;asset:{chartId:string;periodId:string;costAccountId:string;accumulatedAccountId:string;expenseAccountId:string;cashAccountId:string;gainAccountId:string};report:{chartId:string;cashId:string;capitalId:string;retainedId:string;revenueId:string;expenseId:string};hire:{handoffId:string;candidateId:string;candidateName:string;offerId:string;applicationId:string;payGroupId:string;sourceHandoff:unknown};learning:{assignmentId:string;courseId:string;planId:string;windowId:string;personId:string};withholding:{supplierId:string;paymentId:string;bankTransactionId:string;remittanceBankTransactionId:string;recoveryBankTransactionId:string;chartId:string;periodId:string;expenseAccountId:string;payableAccountId:string;inputVatAccountId:string;bankAccountId:string;withholdingAccountId:string;invoiceNumber:string}};
export type EngineeringMoneyPeopleBrowser={run:string;seedRun?:string;password:string;notice:string;companies:Record<string,EngineeringMoneyPeopleCompany>};
const key=()=>randomUUID();
function exact(db:PostgresDatabase,run:string,binding:string){const url=new URL(String(db.pool.options.connectionString));if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding}`)throw new Error('EXACT_MONEY_PEOPLE_BROWSER_DATABASE_REQUIRED');}

export async function seedEngineeringMoneyPeopleBrowser(input:Input){
 if(input.workspaceProducts&&(new Set(input.workspaceProducts).size!==input.workspaceProducts.length||input.workspaceProducts.some(id=>!product(id))))throw new Error('EXACT_WORKSPACE_PRODUCT_FIXTURE_REQUIRED');
 if(!/^[a-f0-9]{12}$/.test(input.run))throw new Error('EXACT_MONEY_PEOPLE_BROWSER_RUN_REQUIRED');exact(input.control,input.run,'control_db');exact(input.db,input.run,'h001_db');
 const password=`Engineering-${randomBytes(18).toString('hex')}`,passwordHash=await hashPassword(password),now=Date.now();
 const state:EngineeringMoneyPeopleBrowser={run:input.run,password,notice:'Fictional companies, declared manual FX quotes and synthetic academy/provider. Engineering browser evidence; no business or provider certification.',companies:{}};
 for(const project of ['desktop','mobile']){
  const f=await seedExpenseCompany(input.db,input.files),actors:Record<string,WorkContext>={},users:Record<string,EngineeringMoneyPeopleUser>={};
  await input.db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Engineering ${project} Company · synthetic only`,f.company).run();
  const rolePermissions:Record<string,string[]>={
   custom_engineering_hr:['people.read','people.manage','module.people_payroll.read','records.read','records.write'],
   custom_engineering_academy:['records.read','records.write','records.review','benefits.review','module.learning.read','module.benefits.read'],
   custom_engineering_learner:['records.read','records.write','self.requests','module.learning.read','module.benefits.read'],
   custom_engineering_talent:['records.read','records.write','records.review','module.talent.read'],
  };
  // Explicit fixture role definitions are published once; every request loads the current revision.
  for(const [role,permissions] of Object.entries(rolePermissions))await input.db.batch([
   input.db.prepare('INSERT INTO iam_roles(company_id,id,name,built_in,published_revision) VALUES(?,?,?,false,1)').bind(f.company,role,`${project} synthetic ${role.replace('custom_engineering_','')}`),
   input.db.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,1,?,'[]','[\"holding\",\"branch\",\"department\"]','fixture',?)").bind(f.company,role,JSON.stringify(permissions),now),
  ]);
  async function register(label:string,role:string,department=false){
   const actor=await f.member(`${project}-${label}`,role);actor.assignmentId=await f.grant(actor,role,department?'department':'holding',department?f.department:null);actor.holdingSlug='simulation';actor.holdingName='Simulation';actor.assignmentScope=department?'department':'holding';actor.branchId=department?f.branch:null;actor.departmentId=department?f.department:null;
   actor.scope=department?{kind:'department',holdingId:'h001',holdingSlug:'simulation',holdingName:'Simulation',companyId:f.company,branchId:f.branch,branchSlug:f.branch,branchName:'Synthetic branch',departmentId:f.department,departmentSlug:f.department,departmentName:'Synthetic team'}:{kind:'holding',holdingId:'h001',holdingSlug:'simulation',holdingName:'Simulation',companyId:f.company};
   actor.modules=role==='holding_owner'?[...new Set((input.workspaceProducts??[]).flatMap(id=>product(id)!.modules))]:role.startsWith('custom_')?rolePermissions[role].filter(permission=>permission.startsWith('module.')).map(permission=>permission.split('.')[1]):[...businessRoles[role as keyof typeof businessRoles]];
   actor.name=`${project} ${label} · synthetic`;actor.email=`engineering-${input.run}-${project}-${label}@simulation.test`;
   await input.db.batch([
    input.db.prepare('UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=? AND id<>?').bind(now,f.company,actor.userId,actor.assignmentId),
    input.db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(actor.modules),actor.assignmentId),
    input.db.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(actor.name,actor.name,actor.email,f.company,actor.userId),
    input.db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES(?,?,?,?,'active','local')").bind(f.company,actor.userId,`principal-${actor.userId}`,`person-${actor.userId}`),
   ]);
   await input.control.batch([
    input.control.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,actor.name,actor.email,now,now),
    input.control.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,passwordHash,now,now),
    input.control.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,actor.name,now),
   ]);
   actor.iam=await loadIamAuthority(input.db,f.company,actor.userId,role);actors[label]=actor;users[label]={key:label,email:actor.email,userId:actor.userId,role,assignmentId:actor.assignmentId,workspaceBase:department?`/holdings/simulation/branches/${f.branch}/departments/${f.department}`:'/holdings/simulation'};return actor;
  }
  const finance=await register('finance-author','finance_preparer'),approver=await register('finance-reviewer','finance_approver');await register('hr','custom_engineering_hr');const academy=await register('academy-author','custom_engineering_academy'),academyReview=await register('academy-reviewer','custom_engineering_academy'),learner=await register('learner','custom_engineering_learner'),talent=await register('talent-author','custom_engineering_talent',true),talentReview=await register('talent-reviewer','custom_engineering_talent',true);
  if(input.workspaceProducts?.length)await register('workspace-owner','holding_owner');
  const accumulated=String((await createAccount(finance,createAccountSchema.parse({chartId:f.chart,code:'ACCUMULATED',name:'Accumulated depreciation',category:'asset'}),key())).id);
  const reportChart=String((await createChart(finance,createChartSchema.parse({name:'Synthetic NPAEs reporting ledger',baseCurrency:'THB',accountingBasis:'TFRS for NPAEs'}),key())).id),reportAccounts:Record<string,string>={};
  for(const [name,category] of [['cash','asset'],['capital','equity'],['retained','equity'],['revenue','revenue'],['expense','expense']] as const)reportAccounts[name]=String((await createAccount(finance,createAccountSchema.parse({chartId:reportChart,code:name.toUpperCase(),name:`Report ${name}`,category}),key())).id);
  const profile=await prepareAccountingProfile(finance,{statutoryChartId:reportChart,standard:'TFRS for NPAEs',fiscalStartMonth:1,effectiveDate:'2025-01-01',eligibilityEvidence:'Fictional Company independently reviewed as meeting the NPAEs eligibility conditions.',policyEvidence:'Synthetic approved accrual, historical-cost, moving-average and straight-line policy evidence.',taxBasis:'current_tax',inventoryPolicy:'moving_average',depreciationPolicy:'straight_line'},key());
  await decideAccountingProfile(approver,String(profile.id),'approve',{expectedVersion:1,reason:'Independent review of fictional Company accounting eligibility and policy evidence.'},key());
  await confirmManualRate(approver,{currency:'THB',effectiveDate:'2025-01-01',currencyPerUsd:'36',note:'Declared synthetic comparative-period quote; no market/provider observation.'},key());
  const comparative=await createPeriod(finance,createPeriodSchema.parse({name:'Synthetic comparative 2025',startDate:'2025-01-01',endDate:'2025-12-31'}),key());
  await input.db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'journal','draft',?,?,'Synthetic independent journal reviewer',?)").bind(f.company,approver.userId,finance.userId,now).run();
  for(const [period,date,debit,credit,amount] of [[comparative.id,'2025-01-01','cash','capital','500.00'],[comparative.id,'2025-09-15','cash','revenue','100.00'],[comparative.id,'2025-09-16','expense','cash','40.00'],[f.period,'2026-09-15','cash','revenue','150.00'],[f.period,'2026-09-16','expense','cash','50.00']] as const){
   const journal=await createJournal(finance,createJournalSchema.parse({chartId:reportChart,periodId:period,entryDate:date,currency:'THB',memo:'Fictional independently reviewed financial reporting source',lines:[{accountId:reportAccounts[debit],debit:amount,credit:'0'},{accountId:reportAccounts[credit],debit:'0',credit:amount}]}),key());
   await postJournalBatch(approver,{journals:[{id:journal.id,expectedVersion:1}],reason:'Independent reviewer confirmed balanced fictional source amounts.'},key());
  }
  const payroll=await register('payroll-author','payroll_preparer');
  await register('finance-payer','treasury');
  const taxAccounts:Record<string,string>={};
  for(const [code,category,controlKind] of [['WHT_BANK','asset','bank'],['WHT_AP','liability','payable'],['WHT_VAT','asset','none'],['WHT_LIABILITY','liability','none']] as const)taxAccounts[code]=String((await createAccount(finance,createAccountSchema.parse({chartId:f.chart,code,name:`Fictional ${code}`,category,currency:'THB',controlKind,isControl:controlKind!=='none'}),key())).id);
  const supplier=await createParty(finance,{kind:'supplier',code:'WHT-SYNTHETIC',name:`${project} Fictional Thai supplier`,currency:'THB',creditLimit:'0',paymentTermsDays:30},key()),taxPayment=await createPayment(finance,{kind:'payment',partyId:String(supplier.id),paymentNumber:`WHT-PAY-${project}`,paymentDate:'2026-09-12',currency:'THB',amount:'2675.03',bankAccount:taxAccounts.WHT_BANK},key());
  await input.db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'invoice','draft',?,?,'Independent synthetic supplier invoice review',?)").bind(f.company,approver.userId,finance.userId,now).run();
  const taxStatement=await importBankStatement(finance,{bankAccount:taxAccounts.WHT_BANK,statementNumber:`WHT-BANK-${project}`,statementDate:'2026-09-12',opening:'0',closing:'-2570.03',sourceHash:`SYNTHETIC-WHT-${input.run}-${project}`,rows:[{externalId:`SYNTHETIC-WHT-${project}`,transactionDate:'2026-09-12',description:'Declared synthetic net bank transfer; no real bank delivery',amount:'-2570.03',currency:'THB'}]},key());
  const taxBankId=String(await input.db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(taxStatement.id).first('id'));
  const monthlyBankIds:string[]=[];
  for(const [suffix,date,opening,closing,amount] of [['REMIT','2026-10-07','-2570.03','-2675.03','-105.00'],['RECOVERY','2026-10-08','-2675.03','-2570.03','105.00']]){
   const statement=await importBankStatement(finance,{bankAccount:taxAccounts.WHT_BANK,statementNumber:`WHT-${suffix}-${project}`,statementDate:date,opening,closing,sourceHash:`SYNTHETIC-WHT-${suffix}-${input.run}-${project}`,rows:[{externalId:`SYNTHETIC-WHT-${suffix}-${project}`,transactionDate:date,description:`Declared synthetic manual tax ${suffix.toLowerCase()} receipt; no real filing or bank delivery`,amount,currency:'THB'}]},key());
   monthlyBankIds.push(String(await input.db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first('id')));
  }
  const group=await createPayGroup(payroll,payGroupSchema.parse({name:'Engineering monthly THB',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key());
  const req=await createTalentRequisition(talent,{jobTitle:'Synthetic service specialist',openings:1,targetStartDate:'2026-11-01',description:'Fictional role for normal Talent-to-HR browser consumption.'},key());
  await changeTalentRecord(talentReview,'requisition',String(req.id),{action:'approve',expectedVersion:1,reason:'Independent fictional headcount review'},key());await changeTalentRecord(talentReview,'requisition',String(req.id),{action:'open',expectedVersion:2,reason:'Reviewed fictional role opened to candidates'},key());
  const candidateName=`${project} Synthetic Candidate`,candidate=await createTalentCandidate(talent,{legalName:candidateName,email:`candidate-${input.run}-${project}@simulation.test`,consentEvidence:'Fictional candidate consent for browser workflow verification only.',retentionUntil:'2027-12-31'},key()),app=await createTalentApplication(talent,{requisitionId:req.id,requisitionVersion:3,candidateId:candidate.id,ownerId:talent.userId},key());
  await changeTalentRecord(talent,'application',String(app.id),{action:'interview',expectedVersion:1,reason:'Fictional interview completed with retained panel evidence'},key());
  const proposal=await createTalentProposal(talent,{applicationId:app.id,applicationVersion:2,recommendation:'hire',scoreBps:9200,evidence:'Fictional interview panel retained identity and role-fit evidence.'},key());await changeTalentRecord(talentReview,'proposal',String(proposal.id),{action:'accept-proposal',expectedVersion:1,reason:'Independent review of the fictional panel recommendation'},key());await changeTalentRecord(talent,'application',String(app.id),{action:'select',expectedVersion:2,reason:'Accepted independent proposal authorizes offer preparation'},key());
  const offer=await createTalentOffer(talent,{applicationId:app.id,applicationVersion:3,currency:'THB',baseAmount:'36000.00',startDate:'2026-11-01',expiresOn:'2026-11-15',terms:'Fictional monthly compensation and onboarding conditions.'},key());await changeTalentRecord(talentReview,'offer',String(offer.id),{action:'approve-offer',expectedVersion:1,reason:'Independent fictional compensation review'},key());await changeTalentRecord(talent,'offer',String(offer.id),{action:'send',expectedVersion:2,reason:'Reviewed offer delivered to the fictional candidate'},key());await changeTalentRecord(talent,'offer',String(offer.id),{action:'accept-offer',expectedVersion:3,reason:'Fictional candidate acceptance recorded against reviewed offer',acceptanceEvidence:'Signed synthetic offer reference ENGINEERING-HIRE-1'},key());
  const handoff=(await talentDetail(talent,String(app.id))).handoff!;
  const course=await createLearningCourse(academy,{title:`${project} Synthetic qualification`,description:'Fictional safety training prerequisite for the benefit election.',provider:'Synthetic academy fixture',durationMinutes:60,requiredFor:'equipment_operator'},key());await changeLearningCourse(academyReview,String(course.id),{action:'publish',expectedVersion:1,reason:'Independent course content and eligibility review'},key());
  const assignment=await createLearningAssignment(academy,{courseId:course.id,courseVersion:2,learnerId:learner.userId,dueDate:'2026-12-31',reason:'Fictional learner must qualify before electing the benefit'},key());
  const plan=await createBenefitPlan(academy,{name:`${project} Qualified coverage`,provider:'Synthetic provider fixture',coverageType:'employee',currency:'THB',employeeDeduction:'125.50',employerCost:'300.00',effectiveFrom:'2026-09-01',effectiveTo:null,requiredEligibilityCode:'equipment_operator'},key());await changeBenefitRecord(academyReview,'plan',String(plan.id),{action:'publish',expectedVersion:1,reason:'Independent fictional coverage terms review'},key());
  const window=await createBenefitWindow(academy,{name:`${project} Synthetic enrollment`,opensOn:'2026-09-01',closesOn:'2026-12-31',effectiveOn:'2026-09-01'},key());await changeBenefitRecord(academyReview,'window',String(window.id),{action:'open',expectedVersion:1,reason:'Independent fictional enrollment date review'},key());
  state.companies[project]={companyId:f.company,users,asset:{chartId:f.chart,periodId:f.period,costAccountId:f.accounts.cash,accumulatedAccountId:accumulated,expenseAccountId:f.accounts.expense,cashAccountId:f.accounts.tax,gainAccountId:f.accounts.revenue},report:{chartId:reportChart,cashId:reportAccounts.cash,capitalId:reportAccounts.capital,retainedId:reportAccounts.retained,revenueId:reportAccounts.revenue,expenseId:reportAccounts.expense},hire:{handoffId:String(handoff.id),candidateId:String(candidate.id),candidateName,offerId:String(offer.id),applicationId:String(app.id),payGroupId:String(group.id),sourceHandoff:handoff},learning:{assignmentId:String(assignment.id),courseId:String(course.id),planId:String(plan.id),windowId:String(window.id),personId:`person-${learner.userId}`},withholding:{supplierId:String(supplier.id),paymentId:String(taxPayment.id),bankTransactionId:taxBankId,remittanceBankTransactionId:monthlyBankIds[0],recoveryBankTransactionId:monthlyBankIds[1],chartId:f.chart,periodId:f.period,expenseAccountId:f.accounts.expense,payableAccountId:taxAccounts.WHT_AP,inputVatAccountId:taxAccounts.WHT_VAT,bankAccountId:taxAccounts.WHT_BANK,withholdingAccountId:taxAccounts.WHT_LIABILITY,invoiceNumber:`ITEMIZED-WHT-${project}`}};
 }
 const path=join(input.directory,'engineering-money-people.json');await writeFile(path,JSON.stringify(state,null,2)+'\n',{mode:0o600});await chmod(path,0o600);return {projects:Object.keys(state.companies),companyIds:Object.values(state.companies).map(company=>company.companyId),notice:state.notice,fixturePath:path};
}
