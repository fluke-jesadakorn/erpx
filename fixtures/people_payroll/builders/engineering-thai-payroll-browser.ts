/** Native, source-dated Thai payroll examples. All people and bank data are fictional. */
import {randomBytes,randomUUID} from 'node:crypto';
import {chmod,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {hashPassword} from 'better-auth/crypto';
import type {PostgresDatabase} from "@erpx/server-runtime/adapters/runtime/postgres";
import type {FileStore} from "@erpx/server-runtime/adapters/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {seedExpenseCompany} from "../../expenses/company";
import {businessRoles} from "@erpx/feature-identity_access/lib/business-roles";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {createAccount,createAccountSchema,createJournal,createJournalSchema,postJournalBatch} from '@erpx/server-finance/public/finance';
import {thaiPayroll2026Pack} from "@erpx/feature-people_payroll/lib/thai-payroll-pack";
import {assignPayGroup,approvePayrollAccountMapping,approveTimeEntry,compensationSchema,createCompensation,createCountryPack,createPayGroup,createTimeEntry,countryPackTransitionSchema,payGroupSchema,payrollAccountMappingSchema,savePayrollAccountMapping,timeEntrySchema,transitionCompensation,transitionCountryPack} from "@erpx/server-people_payroll/public/people-payroll";
import {createThaiPayrollProfile,decideThaiPayrollProfile,thaiPayrollProfileSchema} from "@erpx/server-people_payroll/public/payroll-statutory";
import {thai2026Reference} from "../../../tests/support/helpers/thai-payroll-reference";

export type ThaiPayrollBrowserUser={email:string;userId:string;assignmentId:string;role:string};
export type ThaiPayrollBrowserCompany={companyId:string;users:Record<string,ThaiPayrollBrowserUser>;preparerSelfReviewAssignmentId:string;payGroupId:string;packId:string;packVersion:string;profileId:string;engagementId:string;chartId:string;periodId:string;bankAccountId:string;netPayableAccountId:string;reference:ReturnType<typeof thai2026Reference>};
export type ThaiPayrollBrowserFixture={run:string;password:string;notice:string;companies:Record<string,ThaiPayrollBrowserCompany>};
type Input={run:string;directory:string;control:PostgresDatabase;db:PostgresDatabase;files:FileStore};
const commandKey=()=>randomUUID();
function exact(db:PostgresDatabase,run:string,binding:string){const url=new URL(String(db.pool.options.connectionString));if(url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${run}_${binding}`)throw new Error('EXACT_THAI_PAYROLL_BROWSER_DATABASE_REQUIRED');}

export async function seedEngineeringThaiPayrollBrowser(input:Input){
 if(!/^[a-f0-9]{12}$/.test(input.run))throw new Error('EXACT_THAI_PAYROLL_BROWSER_RUN_REQUIRED');
 exact(input.control,input.run,'control_db');exact(input.db,input.run,'h001_db');
 const password=`Payroll-${randomBytes(18).toString('hex')}`,passwordHash=await hashPassword(password),now=Date.now();
 const state:ThaiPayrollBrowserFixture={run:input.run,password,notice:'Fictional employees and bank statements. Official source-dated Thai rule revision, independently reviewed declarations and independent reference worksheet. Engineering verification; no live bank transfer or external certification.',companies:{}};
 for(const project of ['desktop','mobile']){
  const f=await seedExpenseCompany(input.db,input.files),users:Record<string,ThaiPayrollBrowserUser>={};
  await input.db.prepare('UPDATE organization_nodes SET display_name=? WHERE id=?').bind(`Thai Payroll ${project} · fictional Company`,f.company).run();
  const hrRole='custom_thai_payroll_hr',hrPermissions=['people.read','people.manage','module.people_payroll.read','records.read','records.write'];
  await input.db.batch([
   input.db.prepare('INSERT INTO iam_roles(company_id,id,name,built_in,published_revision) VALUES(?,?,?,false,1)').bind(f.company,hrRole,'Fictional payroll HR preparer'),
   input.db.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,1,?,'[]','[\"holding\"]','fixture',?)").bind(f.company,hrRole,JSON.stringify(hrPermissions),now),
  ]);
  async function register(label:string,actor:WorkContext,role:string){
   actor.role=role;actor.assignmentId=await f.grant(actor,role);actor.assignmentScope='holding';actor.branchId=null;actor.departmentId=null;actor.holdingSlug='simulation';actor.holdingName='Simulation';
   actor.scope={kind:'holding',holdingId:'h001',holdingSlug:'simulation',holdingName:'Simulation',companyId:f.company};
   actor.modules=role===hrRole?['people_payroll']:[...businessRoles[role as keyof typeof businessRoles]];actor.name=`${project} ${label} · fictional`;actor.email=`thai-payroll-${input.run}-${project}-${label}@simulation.test`;
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
   actor.iam=await loadIamAuthority(input.db,f.company,actor.userId,role);users[label]={email:actor.email,userId:actor.userId,assignmentId:actor.assignmentId,role};return actor;
  }
  const preparer=await register('preparer',f.owner,'payroll_preparer'),reviewer=await register('reviewer',f.manager,'payroll_approver'),finance=await register('finance',f.reviewer,'finance_approver'),mappingAuthor=await register('mapping-author',f.tie,'finance_approver'),hr=await register('hr',f.it,hrRole),employee=await register('employee',f.claimant,'employee'),timeReviewer=await register('time-reviewer',f.head,'people_manager'),treasury=await register('treasury',f.payer,'treasury');
  await register('other-employee',await f.member(`${project}-other-employee`),'employee');
  // An explicit second published role proves same-person rejection even when
  // the preparer holds approval authority. The actual workflow still uses the
  // separately authenticated independent reviewer.
  const preparerSelfReviewAssignmentId=await f.grant(preparer,'payroll_approver');
  let pack=await input.db.prepare("SELECT id,status,rules_json,source_digest FROM hr_country_packs WHERE country_code='TH' AND version=?").bind(thaiPayroll2026Pack.rules.version).first<{id:string;status:string;rules_json:string;source_digest:string}>();
  if(!pack){const created=await createCountryPack(preparer,{countryCode:'TH',version:thaiPayroll2026Pack.rules.version,rules:thaiPayroll2026Pack,exportFormats:['generic-csv']},commandKey());await transitionCountryPack(reviewer,String(created.id),countryPackTransitionSchema.parse({status:'validated',expectedVersion:1,proof:'Independent internal review of retained official source digests and independent synthetic reference worksheet.'}),commandKey());await transitionCountryPack(reviewer,String(created.id),countryPackTransitionSchema.parse({status:'active',expectedVersion:2,proof:'Enable this reviewed dated rule revision for isolated engineering calculations; external certification remains false.'}),commandKey());pack=await input.db.prepare('SELECT id,status,rules_json,source_digest FROM hr_country_packs WHERE id=?').bind(created.id).first();}
  if(!pack||pack.status!=='active'||JSON.stringify(JSON.parse(pack.rules_json))!==JSON.stringify(thaiPayroll2026Pack)||!/^[a-f0-9]{64}$/.test(pack.source_digest))throw new Error('REVIEWED_THAI_RULE_PACK_REQUIRED');
  const group=await createPayGroup(preparer,payGroupSchema.parse({name:`Thai statutory monthly ${project}`,countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),commandKey()),engagementId=`eng-${employee.userId}`;
  const engagementVersion=Number(await input.db.prepare('SELECT version FROM hr_engagements WHERE id=?').bind(engagementId).first('version'));
  await assignPayGroup(hr,engagementId,{payGroupId:String(group.id),expectedVersion:engagementVersion},commandKey());
  const compensation=await createCompensation(hr,compensationSchema.parse({engagementId,effectiveFrom:'2026-09-01',effectiveTo:'2026-12-31',payFrequency:'monthly',baseAmount:'30000.00',hourlyRate:'125.00',expectedHours:160,currency:'THB',components:[{code:'BONUS',label:'Fictional one-off reviewed bonus',kind:'earning',amount:'90000.00',taxable:true,incomeTiming:'irregular',socialSecurityWage:false},{code:'SERVICE_CHARGE',label:'Fictional monthly service charge',kind:'earning',amount:'3000.00',taxable:true,incomeTiming:'regular',socialSecurityWage:true}]}),commandKey());
  await transitionCompensation(reviewer,String(compensation.id),{status:'approved',expectedVersion:1},commandKey());
  const time=await createTimeEntry(employee,timeEntrySchema.parse({engagementId,workDate:'2026-09-15',regularHours:8,overtimeHours:4,note:'Fictional work and OT independently reviewed for the monthly source-dated payroll.'}),commandKey());await approveTimeEntry(timeReviewer,String(time.id),1,commandKey());
  const profile=await createThaiPayrollProfile(preparer,thaiPayrollProfileSchema.parse({engagementId,expectedEngagementVersion:engagementVersion+1,taxYear:2026,effectiveFrom:'2026-09-01',effectiveTo:'2026-12-31',verifiedAnnualDeductionsMinor:7050000,scheduledPaysInYear:12,socialSecurityEligible:true,openingThroughDate:'2026-08-31',openingTaxableMinor:24000000,openingIrregularMinor:0,openingWithheldMinor:150000,priorIncomeComplete:true,evidenceRef:'Declared fictional employee allowances and complete employer payroll opening YTD through August; retained for isolated engineering verification.'}),commandKey());await decideThaiPayrollProfile(reviewer,String(profile.id),{action:'approve',expectedVersion:1,reason:'Different reviewer reconciled the fictional employee declaration and complete opening income and withholding.'},commandKey());
  const accounts:Record<string,string>={};
  for(const [name,category] of [['tax','liability'],['sso','liability'],['employer','expense'],['bonus','expense'],['service','expense']] as const)accounts[name]=String((await createAccount(finance,createAccountSchema.parse({chartId:f.chart,code:`PAYROLL-${name.toUpperCase()}`,name:`Thai payroll ${name}`,category}),commandKey())).id);
  const mapping=await savePayrollAccountMapping(mappingAuthor,String(group.id),payrollAccountMappingSchema.parse({chartId:f.chart,expenseAccountId:f.accounts.expense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:accounts.tax,socialSecurityPayableAccountId:accounts.sso,employerExpenseAccountId:accounts.employer,bonusExpenseAccountId:accounts.bonus,serviceChargeExpenseAccountId:accounts.service}),commandKey());await approvePayrollAccountMapping(finance,String(mapping.id),1,commandKey());
  const capital=await createAccount(finance,createAccountSchema.parse({chartId:f.chart,code:'PAYROLL-CAPITAL',name:'Fictional reviewed opening capital',category:'equity'}),commandKey());
  await input.db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'journal','draft',?,?,'Independent reviewer of the declared fictional payroll bank opening',?)").bind(f.company,mappingAuthor.userId,finance.userId,now).run();
  const opening=await createJournal(finance,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-01',currency:'THB',memo:'Declared fictional THB 200000 opening bank capital for exact payroll statement and GL reconciliation',lines:[{accountId:f.accounts.cash,debit:'200000.00',credit:'0.00'},{accountId:String(capital.id),debit:'0.00',credit:'200000.00'}]}),commandKey());await postJournalBatch(mappingAuthor,{journals:[{id:opening.id,expectedVersion:1}],reason:'Different Finance reviewer confirmed balanced fictional opening capital and exact bank control account.'},commandKey());
  for(const [step,user] of [['calculate',preparer.userId],['preview',preparer.userId],['in_review',reviewer.userId],['approved',finance.userId]])await input.db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll',?,?,?,'Declared fictional independent payroll responsibilities',?)").bind(f.company,step,user,preparer.userId,now).run();
  state.companies[project]={companyId:f.company,users,preparerSelfReviewAssignmentId,payGroupId:String(group.id),packId:pack.id,packVersion:thaiPayroll2026Pack.rules.version,profileId:String(profile.id),engagementId,chartId:f.chart,periodId:f.period,bankAccountId:f.accounts.cash,netPayableAccountId:f.accounts.payable,reference:thai2026Reference({ordinaryBaht:33000,occasionalBaht:90750,annualDeductionsBaht:70500,wageBaseBaht:33000,pays:12})};
  // Merely retaining this account does not imply that the fixture has transferred funds.
  if(treasury.userId===finance.userId)throw new Error('INDEPENDENT_TREASURY_REQUIRED');
 }
 const path=join(input.directory,'engineering-thai-payroll.json');await writeFile(path,JSON.stringify(state,null,2)+'\n',{mode:0o600});await chmod(path,0o600);
 return {projects:Object.keys(state.companies),companyIds:Object.values(state.companies).map(company=>company.companyId),notice:state.notice,fixturePath:path};
}
