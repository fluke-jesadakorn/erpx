import assert from "node:assert/strict";
import { getLocalProxy,ensureFixtureLevel } from "./local/fixtures";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("Local verification only.");
const jars=new Map<string,Map<string,string>>();
const contexts=new Map<string,{revision:number;company:string;assignment:string}>();
const scenarioRun=crypto.randomUUID();
const receiptRef=`SERVICE-RECEIPT-${scenarioRun.slice(0,8)}`;
async function api(who:string,holding:string,path:string,input?:unknown,status=200,phase=crypto.randomUUID(),method=input?"POST":"GET"){
 const key=new Bun.CryptoHasher("sha256").update(`ledger-payroll-v1:${scenarioRun}:${phase}`).digest("hex").slice(0,36);
 const context=contexts.get(who);const r=await fetch(`${origin}${path.startsWith('/api/')?path:`/api/v1/holdings/${holding}/${path}`}`,{method,headers:{Origin:origin,"Content-Type":"application/json","Idempotency-Key":key,Cookie:[...(jars.get(who)??new Map())].map(([k,v])=>`${k}=${v}`).join('; '),...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}: {})},...(input?{body:JSON.stringify(input)}:{})});const jar=jars.get(who)??new Map<string,string>();for(const v of r.headers.getSetCookie()){const pair=v.split(';')[0],i=pair.indexOf('=');jar.set(pair.slice(0,i),pair.slice(i+1));}jars.set(who,jar);const body=await r.json() as {data:Record<string,unknown>;error:unknown};assert.equal(r.status,status,`${path}: ${JSON.stringify(body.error)}`);return body.data;
}
async function selectWorkRole(who:string,holding:string,assignment?:string){const active=await api(who,holding,"assignments/active");const selected=assignment??(active.assignmentId as string|null)??((active.options as Record<string,unknown>[])[0]?.id as string|undefined);assert.ok(selected,`No active assignment for ${who} in ${holding}`);const chosen=await api(who,holding,"assignments/active",{assignmentId:selected,revision:Number(active.revision??0)});contexts.set(who,{revision:Number(chosen.revision),company:holding==="acme"?"acme-co":"northstar-co",assignment:String(chosen.assignmentId)});}
async function assignFlow(who:string,holding:string,kind:string,id:string,owners:Record<string,string>){let flow=await api(who,holding,`flows/${kind}/${id}`);for(const [stepKey,userId] of Object.entries(owners)){const step=(flow.steps as Record<string,unknown>[]).find(item=>item.key===stepKey);if(step?.primary)continue;flow=await api(who,holding,`flows/${kind}/${id}`,{stepKey,userId,expectedVersion:Number(flow.version),recordVersion:Number(flow.recordVersion),reason:`Acceptance routing: ${stepKey} owner is responsible for this handoff.`,saveDefault:false});}}
for(const who of ["sam","mia","luca","daniel","nisa","mali"]){await api(who,"","/api/auth/sign-in/email",{email:`${who}@erpx.dev`,password:`${who[0].toUpperCase()+who.slice(1)}-ERPX-2026!`});}
const platform=await getLocalProxy({persist:true,remoteBindings:false});
await platform.env.H002_DB.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('acceptance-luca-payroll','usr_luca','payroll_preparer','[]','northstar-co',0,?,'holding') ON CONFLICT(id) DO UPDATE SET valid_until=excluded.valid_until,revoked_at=NULL").bind(Date.now()+365*86400000).run();
await selectWorkRole("daniel","acme");await selectWorkRole("nisa","acme","workflow_controller");await selectWorkRole("mali","acme","asg_mali_owner");await selectWorkRole("sam","northstar");await selectWorkRole("mia","northstar");await selectWorkRole("luca","northstar","acceptance-luca-payroll");
try{
 const db=platform.env.H001_DB,hr=platform.env.H002_DB;
 await ensureFixtureLevel(hr,"northstar-co");
 const position=await hr.prepare("SELECT id,level_id FROM organization_positions WHERE company_id='northstar-co' AND branch_id='rotterdam' AND department_id='dept:rotterdam:operations' AND title='Senior Inventory Planner' LIMIT 1").first<{id:string;level_id:string}>();assert.ok(position);
 const manager=await hr.prepare("SELECT id FROM hr_people WHERE company_id='northstar-co' AND user_id='usr_sam' AND merged_into_id IS NULL").first<{id:string}>();assert.ok(manager);
 const levelId=String(position.level_id);
 const period=await db.prepare("SELECT id FROM finance_periods WHERE company_id='acme-co' AND start_date<='2026-09-06' AND end_date>='2026-09-06' AND state='open' ORDER BY id LIMIT 1").first<{id:string}>();assert.ok(period);
 const chart="commerce-chart-acme-co";const input={chartId:chart,periodId:period.id,entryDate:"2026-09-06",currency:"USD",memo:"Office supplies journal acceptance",lines:[{accountId:`${chart}-5000`,debit:"75.00",credit:"0"},{accountId:`${chart}-1100`,debit:"0",credit:"75.00"}]};
 const journal=await api("daniel","acme","finance/journals",input,201,"manual-create");assert.deepEqual(await api("daniel","acme","finance/journals",input,201,"manual-create"),journal);
 await assignFlow("mali","acme","journal",String(journal.id),{draft:"usr_nisa"});
 await api("daniel","acme",`finance/journals/${journal.id}/post`,{expectedVersion:1},403,"manual-self-post");
 await api("nisa","acme",`finance/journals/${journal.id}/post`,{expectedVersion:1},200,"manual-post");
 await api("nisa","acme",`finance/journals/${journal.id}/reverse`,{periodId:period.id,entryDate:"2026-09-06",memo:"Reverse acceptance journal"},201,"manual-reverse");
 const report=await api("nisa","acme",`finance/trial-balance?chartId=${chart}`) as unknown as Array<{id:string;balance:string}>;
 const actual=await db.prepare("SELECT a.id,COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) balance FROM finance_accounts a JOIN finance_journal_lines l ON l.account_id=a.id JOIN finance_journals j ON j.id=l.journal_id WHERE a.chart_id=? AND j.status IN ('posted','reversed') GROUP BY a.id").bind(chart).all<{id:string;balance:number}>();
 for(const row of actual.results)assert.equal(Math.round(Number(report.find(r=>r.id===row.id)?.balance)*100),row.balance,"Trial balance retains original entries alongside reversals");
 const invoice=await api("daniel","acme","finance/invoices",{partyId:"commerce-acme-co-customer",kind:"receivable",invoiceNumber:`SERVICE-ACCEPTANCE-${scenarioRun.slice(0,8)}`,invoiceDate:"2026-09-06",dueDate:"2026-09-30",currency:"USD",subtotal:"100.00",tax:"7.00",debitAccountId:`${chart}-1200`,creditAccountId:`${chart}-4000`,taxAccountId:`${chart}-2200`},201,"service-invoice-v2");
 await assignFlow("mali","acme","invoice",String(invoice.id),{draft:"usr_nisa"});
 await api("daniel","acme",`finance/invoices/${invoice.id}/post`,{expectedVersion:1,periodId:period.id,entryDate:"2026-09-06",fxRate:"1"},403,"service-self-post-v2");
 const post=await api("nisa","acme",`finance/invoices/${invoice.id}/post`,{expectedVersion:1,periodId:period.id,entryDate:"2026-09-06",fxRate:"1"},200,"service-invoice-post-v2");assert.deepEqual(await api("nisa","acme",`finance/invoices/${invoice.id}/post`,{expectedVersion:1,periodId:period.id,entryDate:"2026-09-06",fxRate:"1"},200,"service-invoice-post-v2"),post);
 const amounts=await db.prepare("SELECT a.code,l.debit_minor,l.credit_minor FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE journal_id=? ORDER BY a.code").bind(post.journalId as string).all();assert.deepEqual(amounts.results,[{code:"1200",debit_minor:10700,credit_minor:0},{code:"2200",debit_minor:0,credit_minor:700},{code:"4000",debit_minor:0,credit_minor:10000}]);
 await assignFlow("mali","acme","invoice",String(invoice.id),{posted:"usr_mali"});
 await selectWorkRole("mali","acme","workflow_treasury");
 const paid=await api("mali","acme",`commerce/invoices/${invoice.id}/settle`,{expectedVersion:2,date:"2026-09-06",amount:"107.00",reference:receiptRef},200,"service-receipt-v2");
 const statementInput={bankAccount:`${chart}-1100`,statementNumber:`SERVICE-BANK-${scenarioRun.slice(0,8)}`,statementDate:"2026-09-06",opening:"0.00",closing:"107.00",sourceHash:`acceptance-${scenarioRun.slice(0,8)}`,rows:[{externalId:receiptRef,transactionDate:"2026-09-06",description:"Service invoice receipt",amount:"107.00",currency:"USD"}]};
 await api("daniel","acme","finance/bank/statements/import",{...statementInput,closing:"108.00"},422,"bank-unbalanced");
 const bank=await api("daniel","acme","finance/bank/statements/import",statementInput,201,"bank-statement");assert.deepEqual(await api("daniel","acme","finance/bank/statements/import",statementInput,201,"bank-statement"),bank);
 const bankRow=await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(bank.id as string).first<{id:string}>();assert.ok(bankRow);
 await api("nisa","acme",`finance/bank/transactions/${bankRow.id}/reconcile`,{expectedVersion:1,matchedType:"payment",matchedId:"unknown-payment",proof:"Invalid match attempt"},409,"invalid-bank-match");
 await api("nisa","acme",`finance/bank/transactions/${bankRow.id}/reconcile`,{expectedVersion:1,matchedType:"payment",matchedId:paid.paymentId,proof:"Statement reference and amount verified against receipt"},200,"bank-match");
 const employeeEmail=`sophie.${scenarioRun.slice(0,8)}@example.test`;
 let employee=await hr.prepare("SELECT p.id,e.id engagement_id,e.version FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE p.email=? AND p.status='active'").bind(employeeEmail).first<{id:string;engagement_id:string;version:number}>();
 if(!employee){
   const created=await api("mia","northstar","people/directory",{legalName:"Sophie Vermeer",preferredName:"Sophie",email:employeeEmail,phone:"",countryCode:"NL",branchId:"rotterdam",departmentId:"dept:rotterdam:operations",jobTitle:"Warehouse Planning Analyst",workerType:"employee",employmentType:"salaried",startDate:"2026-09-07",currency:"EUR",managerPersonId:manager.id,levelId,positionId:position.id},201,"employee-create");
   await api("sam","northstar",`people/directory/${created.id}/status`,{status:"active",expectedVersion:1},200,"employee-activate");
   employee=await hr.prepare("SELECT p.id,e.id engagement_id,e.version FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE p.id=?").bind(created.id as string).first<{id:string;engagement_id:string;version:number}>();
 }
 assert.ok(employee,"Employee and engagement persist together");
 const timeInput={engagementId:employee.engagement_id,workDate:"2026-09-08",regularHours:8,overtimeHours:0,note:"Warehouse capacity and delivery planning"};
 await api("mia","northstar","people/time-entries",{...timeInput,regularHours:24,overtimeHours:1},422,"invalid-time");
 const time=await api("mia","northstar","people/time-entries",timeInput,201,"time-create");
 assert.deepEqual(await api("mia","northstar","people/time-entries",timeInput,201,"time-create"),time);
 await api("mia","northstar",`people/time-entries/${time.id}/approve`,{expectedVersion:1},409,"time-self-approve");
 await api("sam","northstar",`people/time-entries/${time.id}/approve`,{expectedVersion:1},200,"time-approve");
 const correctionInput={...timeInput,workDate:"2026-09-09",regularHours:7};
 const correction=await api("mia","northstar","people/time-entries",correctionInput,201,"time-correction-create");
 await api("sam","northstar",`people/time-entries/${correction.id}/reject`,{expectedVersion:1,reason:"Include the final hour of delivery planning."},200,"time-return");
 const {engagementId,...corrected}=correctionInput;assert.ok(engagementId);
 await api("mia","northstar",`people/time-entries/${correction.id}`,{...corrected,regularHours:8,expectedVersion:2},200,"time-resubmit");
 await api("sam","northstar",`people/time-entries/${correction.id}/approve`,{expectedVersion:3},200,"corrected-time-approve");
 await api("mia","northstar",`people/time-entries/${correction.id}`,{...corrected,regularHours:9,expectedVersion:4},409,"approved-time-locked");
 const group=await api("sam","northstar","people/pay-groups",{name:`Operations new hires EUR ${scenarioRun.slice(0,8)}`,countryCode:"NL",currency:"EUR",frequency:"monthly",timezone:"Europe/Amsterdam"},201,"pay-group");
 await api("mia","northstar",`people/engagements/${employee.engagement_id}/pay-group`,{expectedVersion:2,payGroupId:group.id},200,"assign-pay-group");
 const compensation=await api("mia","northstar","people/compensation",{engagementId:employee.engagement_id,currency:"EUR",effectiveFrom:"2026-10-01",payFrequency:"monthly",baseAmount:"3200.00",hourlyRate:"0",expectedHours:160,components:[{code:"ALLOWANCE",label:"Home office allowance",kind:"earning",amount:"150.00"},{code:"BENEFIT",label:"Voluntary benefit deduction",kind:"deduction",amount:"50.00"}]},201,"compensation");
 await api("mia","northstar",`people/compensation/${compensation.id}/transition`,{expectedVersion:1,status:"approved"},403,"compensation-role-denied");
 await api("sam","northstar",`people/compensation/${compensation.id}/transition`,{expectedVersion:1,status:"approved"},200,"compensation-approve");
 const runInput={payGroupId:group.id,periodStart:"2026-10-01",periodEnd:"2026-10-31",payDate:"2026-10-31",currency:"EUR",mode:"preview"};
 const run=await api("sam","northstar","people/payroll/runs",runInput,201,"payroll-run");
 await assignFlow("sam","northstar","payroll",String(run.id),{calculate:"usr_luca",preview:"usr_mia"});
 await api("sam","northstar","people/payroll/runs",{...runInput,mode:"production",periodStart:"2026-11-01",periodEnd:"2026-11-30",payDate:"2026-11-30"},409,"production-gate");
 const calculation=await api("luca","northstar",`people/payroll/runs/${run.id}/calculate`,{},200,"calculate");assert.deepEqual(calculation.totals,{gross:335000,deductions:5000,employer:0,net:330000,headcount:1});
 assert.deepEqual(await api("luca","northstar",`people/payroll/runs/${run.id}/calculate`,{},200,"calculate"),calculation);
 await api("sam","northstar",`people/payroll/runs/${run.id}/approve`,{expectedVersion:2},403,"self-approve");
 await api("mia","northstar",`people/payroll/runs/${run.id}/approve`,{expectedVersion:2},200,"approve-preview");
 await api("sam","northstar",`people/payroll/runs/${run.id}/post`,{chartId:"commerce-chart-northstar-co",periodId:"unused",entryDate:"2026-10-31",expenseAccountId:"unused",netPayableAccountId:"unused"},409,"preview-post-gate");
 const evidence=await hr.prepare("SELECT COUNT(*) n FROM decision_evidence d JOIN command_receipts c ON c.command_id=d.command_id WHERE d.record_id=?").bind(run.id as string).first<{n:number}>();assert.equal(evidence?.n,1);
 console.log(JSON.stringify({result:"PASS",journalId:journal.id,invoiceId:invoice.id,payrollRunId:run.id,checks:["journal draft replay","independent posting","posted reversal","trial balance includes reversed originals","standalone invoice net/tax split","invoice replay","linked pay group","independent compensation approval","gross includes earnings","payroll snapshot replay","self-approval rejection","approval receipt and evidence","production payroll gate","standalone invoice settlement","bank statement arithmetic","bank import replay","invalid match rollback","bank payment reconciliation","employee fixture bootstrap","time hours validation","time replay and independent approval","preview posting gate","time correction and resubmission","approved time locked"]},null,2));
}finally{await platform.env.H002_DB.prepare("UPDATE assignments SET revoked_at=? WHERE id='acceptance-luca-payroll'").bind(Date.now()).run();await platform.dispose();}
