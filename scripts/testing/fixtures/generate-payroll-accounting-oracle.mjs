const expectedInputs={
  "packages/server-people_payroll/src/domain/payroll-accounting.ts": "0c3c3814af6a577ab7d4f9ea2c4eb829399a1cffbb88c8c16db25faf4d07b451",
  "packages/server-people_payroll/src/adapters/payroll-accounting.ts": "6577499f9a3602fda9c7320f303b76cb74bd723983973dcc2ff2c5a2203846b3",
  "packages/server-people_payroll/src/public/payroll-accounting.ts": "f7e75500f7c9250ef7674f3f5460532128084dc52159ad5cc8c7c015010abc5c",
  "packages/feature-finance/src/lib/finance-money.ts": "c36415b741869d868e321634749ddd97854ccf81dfb32740a0c41789488651b8",
  "packages/server-shared/src/domain/error.ts": "ff9c315f159f7adcff264d6637617b80760f758c7cb0b19e1896f85bb51b798c",
  "tests/unit/people_payroll/payroll-accounting.test.ts": "2f9a62b5604996c0c34ced741a3924156f9a1c72d12fffdd6bbb8dbaa9aa52fd",
  "packages/server-people_payroll/src/application/people-payroll.ts": "acaf0b3618032c256d78ee9c00864df3c6dc26150377722d8c5e002ad5ec83f2",
  "package.json": "e158b60b978f47f7a718283bbaa23b8296b1d075f710ac9ef2e64d241320555a",
  "bun.lock": "714d959fc1f827a8ebdfef2edf73a9f29c52c7c2bd9c0a3e5a613d844d03376a",
  "node_modules/typescript/package.json": "d9b8fb53c67c947fece83bcb73fa8512227ea6a12b110e096fdab8ecdc02b655",
  "node_modules/typescript/lib/typescript.js": "d3f3cd2b04b7f466f4484df921b744223f7bd1f3e353ec9110bdf52695b983d5",
  "node_modules/@typescript/old/package.json": "9332e97c30d3e53ed54910b89207ed657fb444066484df6e5b6965bf130865e9",
  "node_modules/@typescript/old/lib/typescript.js": "569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39"
};
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,readdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {dirname} from 'node:path';
import {resolve} from 'node:path';
if(process.version!=='v26.11.1')throw Error('NODE_VERSION_DRIFT');
const require=createRequire(resolve('package.json')),ts=require('typescript'),wrapper=require('typescript/package.json'),engine=require('@typescript/old/package.json');
if(wrapper.name!=='@typescript/typescript6'||wrapper.version!=='6.0.2'||engine.name!=='typescript'||engine.version!=='6.0.3'||ts.version!=='6.0.3')throw Error('TYPESCRIPT_VERSION_DRIFT');
const hash=x=>createHash('sha256').update(x).digest('hex');
const treeDigest=dir=>{const h=createHash('sha256');function walk(path,rel=''){for(const ent of readdirSync(path,{withFileTypes:true}).sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0)){const name=rel?rel+'/'+ent.name:ent.name,p=path+'/'+ent.name;if(ent.isDirectory())walk(p,name);else if(ent.isFile()){h.update(name+'\0');h.update(readFileSync(p));h.update('\0')}else throw Error('UNEXPECTED_PACKAGE_ENTRY:'+p)}}walk(dir);return h.digest('hex')};
const fixtureRoot=resolve(dirname(fileURLToPath(import.meta.url)),'../../../fixtures/people_payroll/rust-accounting');
const engineTree=treeDigest('node_modules/@typescript/old');
const sources={};for(const [p,h] of Object.entries(expectedInputs)){const b=readFileSync(p);if(hash(b)!==h)throw Error('SOURCE_DRIFT:'+p);sources[p]=b.toString('utf8')}
const b0='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const b0paths=['packages/server-people_payroll/src/domain/payroll-accounting.ts','packages/server-people_payroll/src/adapters/payroll-accounting.ts','packages/server-people_payroll/src/public/payroll-accounting.ts','packages/feature-finance/src/lib/finance-money.ts','packages/server-shared/src/domain/error.ts','tests/unit/people_payroll/payroll-accounting.test.ts','packages/server-people_payroll/src/application/people-payroll.ts'];
for(const p of b0paths)if(execFileSync('git',['show',`${b0}:${p}`],{encoding:'utf8'})!==sources[p])throw Error('B0_SOURCE_DRIFT:'+p);
const mod=(p,bindings)=>{const m={exports:{}};const code=ts.transpileModule(sources[p],{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new Function('module','exports','require',code)(m,m.exports,n=>{if(Object.hasOwn(bindings,n))return bindings[n];throw Error('UNREVIEWED_IMPORT:'+p+':'+n)});return m.exports};
const shared=mod('packages/server-shared/src/domain/error.ts',{}),money=mod('packages/feature-finance/src/lib/finance-money.ts',{}),domain=mod('packages/server-people_payroll/src/domain/payroll-accounting.ts',{'@erpx/server-shared/index':shared,'@erpx/feature-finance/lib/finance-money':money});
const accounts={expenseAccountId:'wages',bonusExpenseAccountId:'bonus',employerExpenseAccountId:'sso-expense',netPayableAccountId:'net',taxPayableAccountId:'tax',socialSecurityPayableAccountId:'sso',advanceReceivableAccountId:'advances',benefitPayableAccountId:'benefits',otherDeductionPayableAccountId:'other',employerContributionAccountId:'employer',serviceChargeExpenseAccountId:'service',overtimeExpenseAccountId:'overtime'};
const baseSnapshot={engagement:{employment_type:'salaried',start_date:'2026-09-01',end_date:null},period:{start:'2026-09-01',end:'2026-09-30'},hours:{entries:[],handoffs:[]},costAssignments:[{id:'assignment-1',effective_from:'2026-09-01',effective_to:null,branch_id:'branch-1',department_id:'department-1',evidence_status:'verified'}]};
const baseComponents=[{code:'BONUS',kind:'earning',amountMinor:20000},{code:'thai:withholding',kind:'deduction',amountMinor:7000},{code:'thai:sso-employee',kind:'deduction',amountMinor:5000},{code:'payroll-advance:repayment-1',kind:'deduction',amountMinor:2000},{code:'thai:sso-employer',kind:'employer',amountMinor:5000}];
const baseItem={id:'item-1',gross_minor:120000,deductions_minor:14000,employer_cost_minor:5000,net_minor:106000,input_snapshot_json:JSON.stringify(baseSnapshot),result_json:JSON.stringify({explanation:{hourlyRateMinor:0,regularHours:0,overtimeHours:0,components:baseComponents}})};
const simple=(changes={})=>({...baseItem,gross_minor:100,deductions_minor:0,employer_cost_minor:0,net_minor:100,result_json:JSON.stringify({explanation:{components:[]}}),...changes});
const snap=(item,value)=>({...item,input_snapshot_json:JSON.stringify(value)}),result=(item,components,extra={})=>({...item,result_json:JSON.stringify({explanation:{...extra,components}})}),clone=x=>structuredClone(x);
const assignment=(id,from,to,branch,department=null,evidence='verified')=>({id,effective_from:from,effective_to:to,branch_id:branch,department_id:department,evidence_status:evidence});
const split=clone(baseSnapshot);split.costAssignments=[assignment('assignment-1','2026-09-01','2026-09-15','branch-1','department-1'),assignment('assignment-2','2026-09-16',null,'branch-2','department-2')];
const hourly=clone(split);hourly.engagement.employment_type='hourly';hourly.hours.entries=[{work_date:'2026-09-05',regular_hours:5,overtime_hours:0,cost_assignment_id:'assignment-1',cost_branch_id:'branch-1',cost_department_id:'department-1'},{work_date:'2026-09-20',regular_hours:5,overtime_hours:0,cost_assignment_id:'assignment-2',cost_branch_id:'branch-2',cost_department_id:'department-2'}];
const cases=[];
function add(id,items,acct=accounts){cases.push({id,itemsJson:JSON.stringify(items),accountsJson:JSON.stringify(acct)})}
function one(id,item,acct=accounts){add(id,[item],acct)}
one('existing-group-components',baseItem,{expenseAccountId:'wages',bonusExpenseAccountId:'bonus',employerExpenseAccountId:'sso-expense',netPayableAccountId:'net',taxPayableAccountId:'tax',socialSecurityPayableAccountId:'sso',advanceReceivableAccountId:'advances'});
one('existing-missing-employer-expense',baseItem,(({employerExpenseAccountId,...rest})=>rest)(accounts));
one('existing-unknown-deduction-unmapped',result(baseItem,[{code:'UNMAPPED',kind:'deduction',amountMinor:14000},{code:'thai:sso-employer',kind:'employer',amountMinor:5000},{code:'BONUS',kind:'earning',amountMinor:20000}]),(({otherDeductionPayableAccountId,...rest})=>rest)(accounts));
one('existing-dated-salary-split',snap(baseItem,split));
const hourItem=snap(simple({gross_minor:100000,net_minor:100000,result_json:JSON.stringify({explanation:{hourlyRateMinor:10000,regularHours:10,overtimeHours:0,components:[]}})}),hourly);
one('existing-hourly-split',hourItem);const mismatch=clone(hourly);mismatch.hours.entries[0].cost_assignment_id=null;one('existing-hourly-id-mismatch',snap(hourItem,mismatch));
// Small bounded date/assignment and time evidence cases.
for(const [id,change] of [
 ['clipped-engagement',s=>{s.engagement.start_date='2026-09-05';s.engagement.end_date='2026-09-20'}],
 ['leap-day-inclusive',s=>{s.period={start:'2024-02-28',end:'2024-03-01'};s.engagement.start_date='2024-02-28';s.costAssignments=[assignment('leap','2024-02-28',null,'leap')]}],
 ['reversed-period',s=>{s.period={start:'2026-09-10',end:'2026-09-01'}}],['empty-date',s=>{s.period.start=''}],['invalid-date',s=>{s.period.start='invalid'}],
 ['date-offset-parse',s=>{s.period.start='2026-09-01T23:00:00-02:00';s.period.end='2026-09-03T00:00:00Z'}],
 ['missing-assignments',s=>{delete s.costAssignments}],['overlapping-assignments',s=>{s.costAssignments.push(assignment('overlap','2026-09-01',null,'other'))}],
 ['unverified-assignment',s=>{s.costAssignments[0].evidence_status='pending'}],
 ['missing-engagement-start',s=>{delete s.engagement.start_date}],
 ['out-of-period-entry-handoff',s=>{s.engagement.employment_type='hourly';s.hours.entries=[{work_date:'2026-08-31',regular_hours:1,overtime_hours:0,cost_assignment_id:'outside',cost_branch_id:'outside',cost_department_id:null}];s.hours.handoffs=[{work_date:'2026-08-31',minutes:60}];s.costAssignments.push(assignment('outside','2026-08-31','2026-08-31','outside'))}],
]){const s=clone(baseSnapshot);change(s);one(id,snap(simple(),s));}
for(const [id,field,value] of [['time-id-mismatch','cost_assignment_id','other'],['time-branch-mismatch','cost_branch_id','other'],['time-department-mismatch','cost_department_id','other']]){const s=clone(hourly);s.hours.entries[0][field]=value;one(id,snap(hourItem,s));}
// All dimensions are sorted within one allocation, not just in item insertion order.
const localeSnapshot=clone(baseSnapshot);localeSnapshot.period={start:'2026-09-01',end:'2026-09-08'};const localeValues=[[null,null],['',''],['a',null],['A',null],['2',null],['ไทย',null],['é','dept'],['e\u0301','dept']];localeSnapshot.costAssignments=localeValues.map(([b,d],i)=>assignment('locale-'+i,`2026-09-0${i+1}`,`2026-09-0${i+1}`,b,d));one('single-item-locale-cost-unit-order',snap(simple({gross_minor:103,net_minor:103}),localeSnapshot));
// Allocation, half rounding, ties and independent multiplier fallbacks.
const unequal=clone(split);unequal.costAssignments[0].effective_to='2026-09-10';unequal.costAssignments[1].effective_from='2026-09-11';
one('unequal-remainders',snap(simple({gross_minor:101,net_minor:101}),unequal));
one('equal-tie-reordered-assignment-list',snap(simple({gross_minor:101,net_minor:101}),{...split,costAssignments:[...split.costAssignments].reverse()}));
const zero=clone(hourly);zero.hours.entries=[];one('zero-amount-zero-weights',snap(simple({gross_minor:0,net_minor:0}),zero));one('positive-amount-zero-weights',snap(simple(),zero));
for(const [id,regular,ot,minutes] of [['half-micro-hour-positive',0.0000005,0.0000005,0],['below-half-micro-hour',0.00000049,0.00000049,0],['negative-half-hour',-0.0000005,0,0],['handoff-half-round',1,0,0.00003],['handoff-negative',1,0,-1]]){const s=clone(hourly);s.hours.entries=s.hours.entries.slice(0,1);s.hours.entries[0].regular_hours=regular;s.hours.entries[0].overtime_hours=ot;s.hours.handoffs=[{work_date:'2026-09-05',minutes}];one(id,snap(simple(),s));}
for(const [id,exp,policy] of [
 ['overtime-explicit-zero',{hourlyRateMinor:10,overtimeHours:1,overtimeMinor:0,overtimeMultiplierBps:12000},{overtimeMultiplierBps:18000}],
 ['overtime-derived-explanation-multiplier',{hourlyRateMinor:10,overtimeHours:1,overtimeMultiplierBps:12000},{overtimeMultiplierBps:18000}],
 ['overtime-derived-snapshot-multiplier',{hourlyRateMinor:10,overtimeHours:1},{overtimeMultiplierBps:18000}],
 ['overtime-derived-default-multiplier',{hourlyRateMinor:10,overtimeHours:1},{}],
 ['fractional-multiplier',{hourlyRateMinor:10,overtimeHours:1,overtimeMultiplierBps:1.5},{}],
 ['invalid-multiplier',{hourlyRateMinor:10,overtimeHours:1,overtimeMultiplierBps:'bad'},{}],
 ['negative-derived-overtime',{hourlyRateMinor:10,overtimeHours:-1},{}],
 ['null-multiplier-falls-back',{hourlyRateMinor:10,overtimeHours:1,overtimeMultiplierBps:null},{overtimeMultiplierBps:12000}],
]){const s=clone(hourly);s.hours.entries[0].overtime_hours=1;s.policy=policy;one(id,snap(result(simple(),[],exp),s));}
// Classification priorities and mapping behavior, driven only by actual component rules.
const withOvertime=clone(hourly);withOvertime.hours.entries[0].overtime_hours=1;
for(const code of ['overtime','part ot shift'])one('earning-classified-with-overtime-weight-'+code.replaceAll(' ','-'),snap(result(simple({gross_minor:110,net_minor:110}),[{code,kind:'earning',amountMinor:10}]),withOvertime));
for(const code of ['service-charge-bonus-ot','BONUS-OT','INCENTIVE-OVERTIME','overtime','part ot shift','other earning'])one('earning-'+code.replaceAll(' ','-'),result(simple({gross_minor:110,net_minor:110}),[{code,kind:'earning',amountMinor:10}]));
for(const code of ['payroll-advance_tax','benefit_sso','social_tax','tax_PIT','PND1','thai:unmapped','other deduction'])one('deduction-'+code,result(simple({deductions_minor:10,net_minor:90}),[{code,kind:'deduction',amountMinor:10}]));
for(const code of ['payroll-advance_tax','benefit_sso','social_tax','withholding','thai:unmapped','generic employer'])one('employer-'+code,result(simple({employer_cost_minor:10}),[{code,kind:'employer',amountMinor:10}]));
for(const [id,component,missing] of [['map-service',{code:'SERVICE_CHARGE',kind:'earning',amountMinor:10},'serviceChargeExpenseAccountId'],['map-tax',{code:'TAX',kind:'deduction',amountMinor:10},'taxPayableAccountId'],['map-advance',{code:'payroll-advance-a',kind:'deduction',amountMinor:10},'advanceReceivableAccountId'],['map-benefit',{code:'benefit-a',kind:'deduction',amountMinor:10},'benefitPayableAccountId'],['map-other',{code:'other',kind:'deduction',amountMinor:10},'otherDeductionPayableAccountId'],['map-employer',{code:'other',kind:'employer',amountMinor:10},'employerContributionAccountId'],['map-employer-expense',{code:'other',kind:'employer',amountMinor:10},'employerExpenseAccountId']]){const a={...accounts};delete a[missing];const changes=component.kind==='earning'?{gross_minor:110,net_minor:110}:component.kind==='deduction'?{deductions_minor:10,net_minor:90}:{employer_cost_minor:10};one(id,result(simple(changes),[component]),a)}
one('map-social-both-missing',result(simple({deductions_minor:10,net_minor:90}),[{code:'SSO',kind:'deduction',amountMinor:10}]),(({socialSecurityPayableAccountId,employerContributionAccountId,...a})=>a)(accounts));
one('map-social-nullish-fallback',result(simple({deductions_minor:10,net_minor:90}),[{code:'SSO',kind:'deduction',amountMinor:10}]),{...accounts,socialSecurityPayableAccountId:null});
one('map-social-empty-no-fallback',result(simple({deductions_minor:10,net_minor:90}),[{code:'SSO',kind:'deduction',amountMinor:10}]),{...accounts,socialSecurityPayableAccountId:''});
one('zero-component-mapping-still-required',result(simple(),[{code:'TAX',kind:'deduction',amountMinor:0}]),{...accounts,taxPayableAccountId:''});
one('same-employer-and-wage',result(simple({employer_cost_minor:10}),[{code:'SSO',kind:'employer',amountMinor:10}]),{...accounts,employerExpenseAccountId:'wages'});
one('empty-primary-wage-and-net-accounts',simple(),{...accounts,expenseAccountId:'',netPayableAccountId:''});
add('repeated-items-grouped',[simple(),simple({id:'item-2'})]);
add('same-account-different-description',[simple(),result(simple({id:'item-2',gross_minor:110,net_minor:110}),[{code:'bonus',kind:'earning',amountMinor:10}])],{...accounts,bonusExpenseAccountId:'wages'});
const unitItem=(branch,department,id)=>{const s=clone(baseSnapshot);s.period={start:'2026-09-01',end:'2026-09-01'};s.costAssignments=[assignment('a','2026-09-01',null,branch,department)];return snap(simple({id}),s)};
add('dimension-null-empty-case-thai-unicode-order',[unitItem(null,null,'null'),unitItem('', '', 'empty'),unitItem('a',null,'a'),unitItem('A',null,'A'),unitItem('2',null,'two'),unitItem('ไทย',null,'thai'),unitItem('é','dept','composed'),unitItem('e\u0301','dept','decomposed')]);
for(const [id,change] of [['gross-net-mismatch',i=>{i.net_minor=99}],['deduction-total-mismatch',i=>{i.deductions_minor=1}],['employer-total-mismatch',i=>{i.employer_cost_minor=1}],['negative-net',i=>{i.net_minor=-1}],['zero-net',i=>{i.gross_minor=0;i.net_minor=0}],['max-safe',i=>{i.gross_minor=Number.MAX_SAFE_INTEGER;i.net_minor=Number.MAX_SAFE_INTEGER}],['unsafe-gross',i=>{i.gross_minor=Number.MAX_SAFE_INTEGER+1;i.net_minor=Number.MAX_SAFE_INTEGER+1}]]){const i=simple();change(i);one(id,i)}
add('grouped-safe-sum-overflow',[simple({gross_minor:Number.MAX_SAFE_INTEGER,net_minor:Number.MAX_SAFE_INTEGER}),simple({id:'second',gross_minor:1,net_minor:1})]);
one('unknown-component-kind',result(simple(),[{code:'unknown',kind:'other',amountMinor:10}]));
one('zero-earning-no-line',result(simple(),[{code:'bonus',kind:'earning',amountMinor:0}]));
const fiveHundred=Array.from({length:250},(_,i)=>unitItem('b'+String(i).padStart(3,'0'),null,'i'+i));add('exact-500-grouped-lines',fiveHundred);
const fiveOhOne=clone(fiveHundred);fiveOhOne[0]=result({...fiveOhOne[0],gross_minor:101,net_minor:101},[{code:'BONUS',kind:'earning',amountMinor:1}]);add('exact-501-grouped-lines',fiveOhOne);
for(const [id,item] of [
 ['invalid-snapshot-before-result',{...simple(),input_snapshot_json:'{',result_json:'{'}],
 ['invalid-result-before-assignment',{...simple(),input_snapshot_json:JSON.stringify({...baseSnapshot,costAssignments:[]}),result_json:'{'}],
 ['missing-components-before-assignments',{...simple(),input_snapshot_json:JSON.stringify({...baseSnapshot,costAssignments:[]}),result_json:'{}'}],
 ['invalid-components-before-dated-evidence',snap(result(simple({deductions_minor:1}),[],{}),{...baseSnapshot,costAssignments:[]})],
 ['allocation-before-mapping',snap(result(simple({deductions_minor:10,net_minor:90}),[{code:'TAX',kind:'deduction',amountMinor:10}]),{...baseSnapshot,costAssignments:[]})],
 ['missing-explanation',{...simple(),result_json:'{}'}],['null-components',{...simple(),result_json:'{"explanation":{"components":null}}'}],
 ['component-amount-string',result(simple({deductions_minor:10,net_minor:90}),[{code:'tax',kind:'deduction',amountMinor:'10'}])],
 ['component-amount-null',result(simple(),[{code:'bonus',kind:'earning',amountMinor:null}])],
 ['component-amount-missing',result(simple(),[{code:'bonus',kind:'earning'}])],
 ['component-code-missing',result(simple({gross_minor:110,net_minor:110}),[{kind:'earning',amountMinor:10}])],
 ['numeric-string-totals',simple({gross_minor:'100',deductions_minor:'0',employer_cost_minor:'0',net_minor:'100'})],
 ['null-totals',simple({gross_minor:null,net_minor:null})],
 ['missing-gross',(({gross_minor,...i})=>i)(simple())],
 ['numeric-string-explanation',result(simple(),[],{hourlyRateMinor:'10',overtimeHours:'1',overtimeMinor:'0'})],
 ['invalid-number-explanation',result(simple(),[],{hourlyRateMinor:'not-number',overtimeHours:1})],
])one(id,item);
// Inputs are retained as raw JSON texts; these preserve -0 and JSON numeric overflow.
const rawMinus=JSON.stringify([simple()]).replace('"gross_minor":100','"gross_minor":-0').replace('"net_minor":100','"net_minor":-0');cases.push({id:'raw-negative-zero-totals',itemsJson:rawMinus,accountsJson:JSON.stringify(accounts)});
const rawInfinity=JSON.stringify([simple()]).replace('"gross_minor":100','"gross_minor":1e400').replace('"net_minor":100','"net_minor":1e400');cases.push({id:'raw-overflow-number-infinity',itemsJson:rawInfinity,accountsJson:JSON.stringify(accounts)});
one('raw-component-negative-zero',{...simple(),result_json:'{"explanation":{"components":[{"code":"BONUS","kind":"earning","amountMinor":-0}]}}'});
one('raw-component-infinity',{...simple(),result_json:'{"explanation":{"components":[{"code":"BONUS","kind":"earning","amountMinor":1e400}]}}'});
add('empty-items',[]);
// R2: deliberately observable date and rounding constructions; original source remains unchanged.
const clipped=clone(baseSnapshot);clipped.engagement.start_date='2026-09-05';clipped.engagement.end_date='2026-09-20';clipped.costAssignments=[assignment('A','2026-09-01','2026-09-15','A'),assignment('B','2026-09-16','2026-09-30','B')];one('observable-clipped-engagement',snap(simple({gross_minor:160,net_minor:160}),clipped));
const leap=clone(baseSnapshot);leap.engagement.start_date='2024-02-28';leap.engagement.end_date='2024-03-01';leap.period={start:'2024-02-28',end:'2024-03-01'};leap.costAssignments=[assignment('A','2024-02-28','2024-02-28','A'),assignment('B','2024-02-29','2024-03-01','B')];one('observable-leap-day-inclusive',snap(simple({gross_minor:300,net_minor:300}),leap));
const offset=clone(baseSnapshot);offset.period={start:'2026-09-01T23:00:00-02:00',end:'2026-09-03T00:00:00Z'};offset.costAssignments=[assignment('A','2026-09-01','2026-09-01','A'),assignment('B','2026-09-02','2026-09-03','B')];one('observable-offset-calendar-selection',snap(simple(),offset));
function handoffs(a,b){const s=clone(baseSnapshot);s.engagement.employment_type='hourly';s.period={start:'2026-09-01',end:'2026-09-02'};s.costAssignments=[assignment('A','2026-09-01','2026-09-01','A'),assignment('B','2026-09-02','2026-09-02','B')];s.hours={entries:[],handoffs:[{work_date:'2026-09-01',minutes:a},{work_date:'2026-09-02',minutes:b}]};return snap(result(simple(),[],{overtimeMinor:100}),s)}
one('observable-handoff-positive-half',handoffs(0.00003,0.00006));
one('observable-handoff-below-half',handoffs(0.0000294,0.00006));
one('observable-handoff-negative-half',handoffs(-0.00003,0.00006));
one('observable-handoff-negative-net-refusal',handoffs(-1,2));
// R3: source-observed signed weights and Unicode classification, no policy changes.
const signed=clone(baseSnapshot);signed.period={start:'2026-09-01',end:'2026-09-02'};signed.costAssignments=[assignment('A','2026-09-01','2026-09-01','A'),assignment('B','2026-09-02','2026-09-02','B')];signed.hours={entries:[],handoffs:[{work_date:'2026-09-01',minutes:-60},{work_date:'2026-09-02',minutes:120}]};one('observable-signed-overtime-positive-net',snap(result(simple({gross_minor:200,net_minor:200}),[],{overtimeMinor:10}),signed));
one('observable-unicode-long-s-social-deduction',result(simple({deductions_minor:10,net_minor:90}),[{code:'ſocial',kind:'deduction',amountMinor:10}]));
// JSON-safe structural capture preserving exceptional numbers and ordered object keys.
function capture(value,path='$',special=[]){if(typeof value==='number'&&(!Number.isFinite(value)||Object.is(value,-0))){const kind=Object.is(value,-0)?'-0':Number.isNaN(value)?'NaN':value>0?'+Infinity':'-Infinity';special.push({path,kind});return {$number:kind}}if(Array.isArray(value))return value.map((v,i)=>capture(v,`${path}[${i}]`,special));if(value&&typeof value==='object'){const out={};for(const k of Object.keys(value))out[k]=capture(value[k],`${path}.${k}`,special);return out}return value}
function observed(fn){try{const value=fn(),special=[];return {ok:true,value:capture(value,'$',special),specialNumbers:special}}catch(e){return {ok:false,error:{name:e?.name??null,status:e?.status??null,code:e?.code??null,message:e?.message??String(e)}}}}
const fixtureCases=cases.map(c=>{const items=JSON.parse(c.itemsJson),acct=JSON.parse(c.accountsJson),inputSpecial=[];capture(items,'$items',inputSpecial);for(const [i,item] of items.entries())for(const name of ['input_snapshot_json','result_json']){try{capture(JSON.parse(item[name]),`$items[${i}].${name}:decoded`,inputSpecial)}catch{}}return {...c,inputSpecialNumbers:inputSpecial,accounting:observed(()=>domain.payrollAccountingLines(items,acct)),shares:items.map((item,index)=>({index,...observed(()=>domain.payrollNetCostShares(item))}))}});
if(new Set(cases.map(c=>c.id)).size!==cases.length)throw Error('DUPLICATE_CASE_ID');
const find=id=>fixtureCases.find(c=>c.id===id);
const existing=find('existing-group-components').accounting.value;
if(JSON.stringify(existing?.totals)!==JSON.stringify({grossMinor:120000,deductionsMinor:14000,employerMinor:5000,netMinor:106000,debitMinor:125000,creditMinor:125000})||JSON.stringify(existing.lines.map(l=>[l.accountId,l.debit,l.credit]))!==JSON.stringify([['wages','1000.00','0.00'],['bonus','200.00','0.00'],['tax','0.00','70.00'],['sso','0.00','100.00'],['advances','0.00','20.00'],['sso-expense','50.00','0.00'],['net','0.00','1060.00']]))throw Error('EXISTING_GROUP_TEST_DRIFT');
if(find('existing-missing-employer-expense').accounting.error?.code!=='PAYROLL_ACCOUNT_MAPPING_REQUIRED'||find('existing-unknown-deduction-unmapped').accounting.error?.code!=='PAYROLL_ACCOUNT_MAPPING_REQUIRED'||JSON.stringify(find('existing-dated-salary-split').shares[0].value?.map(x=>x.amountMinor))!==JSON.stringify([53000,53000])||JSON.stringify(find('existing-hourly-split').shares[0].value?.map(x=>x.amountMinor))!==JSON.stringify([50000,50000])||find('existing-hourly-id-mismatch').accounting.error?.code!=='PAYROLL_TIME_COST_EVIDENCE_MISSING')throw Error('EXISTING_TEST_DRIFT');
const baselinePath=resolve(fixtureRoot,'projection-108.v1.json');const baselineBytes=readFileSync(baselinePath);if(hash(baselineBytes)!=='5538edb12a5980e9b46cde065c3b68e6c40a369d1a7827af1498b4d3e760a5ee')throw Error('R1_FIXTURE_DRIFT');const baseline=JSON.parse(baselineBytes);if(baseline.cases.length!==108||JSON.stringify(fixtureCases.slice(0,108))!==JSON.stringify(baseline.cases))throw Error('R1_OBSERVATION_DRIFT');
const r2Path=resolve(fixtureRoot,'projection-115.v1.json');const r2Bytes=readFileSync(r2Path);if(hash(r2Bytes)!=='1e7c947a6fd3f82beb8eb3db82099823b17da061ca5a677badc7ee59ad33da81')throw Error('R2_FIXTURE_DRIFT');const r2Baseline=JSON.parse(r2Bytes);if(r2Baseline.cases.length!==115||JSON.stringify(fixtureCases.slice(0,115))!==JSON.stringify(r2Baseline.cases))throw Error('R2_OBSERVATION_DRIFT');
const get=id=>fixtureCases.find(c=>c.id===id);if(get('exact-500-grouped-lines').accounting.value?.lines?.length!==500||get('exact-501-grouped-lines').accounting.error?.code!=='PAYROLL_JOURNAL_TOO_LARGE')throw Error('GROUPED_LINE_CUTOFF_CONSTRUCTION_FAILED');
for(const [p,h] of Object.entries(expectedInputs))if(hash(readFileSync(p))!==h)throw Error('INPUT_CHANGED:'+p);
if(treeDigest('node_modules/@typescript/old')!==engineTree)throw Error('ENGINE_TREE_CHANGED');
// Explicit hand-derived whole-output checks. These are fixed expectations, not an allocation algorithm.
const dimensionsA={branchId:'A'},dimensionsB={branchId:'B'};
const wage=(d,amount)=>({accountId:'wages',description:'Payroll wages',debit:amount,credit:'0.00',dimensions:d});
const overtimeLine=(d,amount)=>({accountId:'overtime',description:'Payroll overtime',debit:amount,credit:'0.00',dimensions:d});
const payable=(d,amount)=>({accountId:'net',description:'Net payroll payable',debit:'0.00',credit:amount,dimensions:d});
const expectedCases=[
 ['observable-clipped-engagement',160,[wage(dimensionsA,'1.10'),payable(dimensionsA,'1.10'),wage(dimensionsB,'0.50'),payable(dimensionsB,'0.50')],[{amountMinor:110,dimensions:dimensionsA},{amountMinor:50,dimensions:dimensionsB}]],
 ['observable-leap-day-inclusive',300,[wage(dimensionsA,'1.00'),payable(dimensionsA,'1.00'),wage(dimensionsB,'2.00'),payable(dimensionsB,'2.00')],[{amountMinor:100,dimensions:dimensionsA},{amountMinor:200,dimensions:dimensionsB}]],
 ['observable-offset-calendar-selection',100,[wage(dimensionsB,'1.00'),payable(dimensionsB,'1.00')],[{amountMinor:100,dimensions:dimensionsB}]],
 ['observable-handoff-positive-half',100,[overtimeLine(dimensionsA,'0.50'),payable(dimensionsA,'0.50'),overtimeLine(dimensionsB,'0.50'),payable(dimensionsB,'0.50')],[{amountMinor:50,dimensions:dimensionsA},{amountMinor:50,dimensions:dimensionsB}]],
 ['observable-handoff-below-half',100,[overtimeLine(dimensionsB,'1.00'),payable(dimensionsB,'1.00')],[{amountMinor:100,dimensions:dimensionsB}]],
 ['observable-handoff-negative-half',100,[overtimeLine(dimensionsB,'1.00'),payable(dimensionsB,'1.00')],[{amountMinor:100,dimensions:dimensionsB}]]
];
for(const [id,n,lines,shares] of expectedCases){const c=get(id), expected={lines,totals:{grossMinor:n,deductionsMinor:0,employerMinor:0,netMinor:n,debitMinor:n,creditMinor:n},categories:{}};if(!c.accounting.ok||JSON.stringify(c.accounting.value)!==JSON.stringify(expected)||!c.shares[0].ok||JSON.stringify(c.shares[0].value)!==JSON.stringify(shares)){throw Error('HAND_DERIVED_MISMATCH:'+id+':'+JSON.stringify({accounting:c.accounting,shares:c.shares}))}}
const negative=get('observable-handoff-negative-net-refusal');for(const v of [negative.accounting,negative.shares[0]])if(v.ok||v.error.status!==409||v.error.code!=='PAYROLL_COST_ALLOCATION_INVALID'||v.error.message!=='Cost-unit payable amounts do not reconcile.')throw Error('HAND_DERIVED_MISMATCH:negative-net:'+JSON.stringify(v));
const signedExpected={lines:[wage(dimensionsA,'0.95'),overtimeLine(dimensionsA,'-0.10'),payable(dimensionsA,'0.85'),wage(dimensionsB,'0.95'),overtimeLine(dimensionsB,'0.20'),payable(dimensionsB,'1.15')],totals:{grossMinor:200,deductionsMinor:0,employerMinor:0,netMinor:200,debitMinor:200,creditMinor:200},categories:{}};
const signedShares=[{amountMinor:85,dimensions:dimensionsA},{amountMinor:115,dimensions:dimensionsB}];
const socialDimensions={branchId:'branch-1',departmentId:'department-1'};
const socialExpected={lines:[wage(socialDimensions,'1.00'),{accountId:'sso',description:'Payroll social security',debit:'0.00',credit:'0.10',dimensions:socialDimensions},payable(socialDimensions,'0.90')],totals:{grossMinor:100,deductionsMinor:10,employerMinor:0,netMinor:90,debitMinor:100,creditMinor:100},categories:{social_security:10}};
const socialShares=[{amountMinor:90,dimensions:socialDimensions}];
for(const [id,expected,shares] of [['observable-signed-overtime-positive-net',signedExpected,signedShares],['observable-unicode-long-s-social-deduction',socialExpected,socialShares]]){const c=get(id);if(!c.accounting.ok||JSON.stringify(c.accounting.value)!==JSON.stringify(expected)||!c.shares[0].ok||JSON.stringify(c.shares[0].value)!==JSON.stringify(shares))throw Error('R3_HAND_DERIVED_MISMATCH:'+id+':'+JSON.stringify({accounting:c.accounting,shares:c.shares}))}
const output={version:1,acceptance:false,description:'Actual pure Payroll domain exports with actual decimal and BusinessRuleError; finite JSON inputs; no DB, authority, posting or statutory acceptance.',base:b0,inputs:expectedInputs,typescriptEngineTreeSha256:engineTree,runtime:{node:process.version,v8:process.versions.v8,icu:process.versions.icu,unicode:process.versions.unicode,defaultLocale:Intl.DateTimeFormat().resolvedOptions().locale,timeZone:Intl.DateTimeFormat().resolvedOptions().timeZone,collator:new Intl.Collator().resolvedOptions(),typescriptWrapper:wrapper.version,typescriptEngine:ts.version},cases:fixtureCases};
const target=resolve(fixtureRoot,'projection.v1.json'),bytes=JSON.stringify(output,null,2)+'\n';
if(process.argv.includes('--check')){if(readFileSync(target,'utf8')!==bytes)throw Error('ORACLE_DRIFT')}else writeFileSync(target,bytes,{mode:0o600});
process.stdout.write(JSON.stringify({cases:cases.length,check:process.argv.includes('--check'),sha256:hash(bytes),runtime:output.runtime})+'\n');
