import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect,test,type BrowserContext,type Page,type Request } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type Role="finance_preparer"|"finance_approver"|"treasury";
type Credentials={password:string;users:Array<{email:string;role:string;roles?:string[]}>};
type Fixture={controlledPaymentInvoiceId:string;controlledPaymentPeriodId:string;controlledPaymentBankTransactionId:string;closeBankAccountId:string;bankMatchPaymentId:string};

type LostCommand={key:string;body:string;receipt:Record<string,unknown>};
type LostOutcome={ok:true;value:LostCommand}|{ok:false;failure:unknown};
type ObservedPost={key:string;body:string};

async function signInAs(context:BrowserContext,page:Page,credentials:Credentials,role:Role,path:string){
 const user=credentials.users.find(item=>item.role===role||item.roles?.includes(role));expect(user,`${role} demo account`).toBeTruthy();
 const login=await context.request.post("/api/auth/sign-in/email",{data:{email:user!.email,password:credentials.password}});expect(login.ok()).toBe(true);
 const endpoint="/api/v1/holdings/simulation/assignments/active";let active=(await(await context.request.get(endpoint)).json()).data;
 const assignment=active.options.find((item:{role:string})=>item.role===role);expect(assignment,`${role} assignment`).toBeTruthy();
 if(active.assignmentId!==assignment.id){const selected=await context.request.post(endpoint,{data:{assignmentId:assignment.id,revision:active.revision}});expect(selected.ok()).toBe(true);active=(await(await context.request.get(endpoint)).json()).data;}
 expect(active.assignmentId).toBe(assignment.id);await page.goto(path);await expect(page.getByText(new RegExp(`${role.replaceAll("_"," ")} ·`,"i")).first()).toBeVisible();
}

function observePosts(page:Page,path:string){
 const posts:ObservedPost[]=[],listener=(request:Request)=>{if(request.method()==="POST"&&new URL(request.url()).pathname===path)posts.push({key:request.headers()["idempotency-key"]??"",body:request.postData()??""});};
 page.on("request",listener);return {posts,stop:()=>page.off("request",listener)};
}

async function commitThenAbort(page:Page,path:string,status:number){
 let finishCommitted!:(outcome:LostOutcome)=>void,finishDelivered!:(outcome:LostOutcome)=>void,releaseResponse!:()=>void,committedDone=false,deliveredDone=false;
 const committed=new Promise<LostOutcome>(resolve=>{finishCommitted=resolve;}),delivered=new Promise<LostOutcome>(resolve=>{finishDelivered=resolve;}),holdResponse=new Promise<void>(resolve=>{releaseResponse=resolve;});
 const finishFailure=(failure:unknown)=>{const outcome:LostOutcome={ok:false,failure};if(!committedDone){committedDone=true;finishCommitted(outcome);}if(!deliveredDone){deliveredDone=true;finishDelivered(outcome);}releaseResponse();};
 const timer=setTimeout(()=>finishFailure(new Error(`COMMAND_REQUEST_NOT_OBSERVED:${path}`)),30_000);
 const handler=async(route:import("@playwright/test").Route)=>{
  const request=route.request();
  try{
   expect(request.method()).toBe("POST");expect(new URL(request.url()).pathname).toBe(path);
   const key=request.headers()["idempotency-key"]??"",body=request.postData()??"";expect(key).not.toBe("");
   const native=await route.fetch({maxRedirects:0,maxRetries:0});let receipt:Record<string,unknown>;
   try{const text=await native.text();expect(native.status(),text).toBe(status);const payload=JSON.parse(text) as {data?:unknown};expect(payload.data).toBeTruthy();receipt=payload.data as Record<string,unknown>;}
   finally{await native.dispose();}
   const outcome:LostOutcome={ok:true,value:{key,body,receipt}};committedDone=true;finishCommitted(outcome);
   await holdResponse;
   await route.abort("failed");deliveredDone=true;clearTimeout(timer);finishDelivered(outcome);
  }catch(failure){try{await route.abort("failed");}catch{/* Preserve the native request failure. */}clearTimeout(timer);finishFailure(failure);}
 };
 const url=`**${path}`;await page.route(url,handler);return {committed,delivered,release:releaseResponse,handler,url};
}

async function assertPersistedBankRow(page:Page,externalId:string,statementNumber:string,status:string){
 await page.goto(`/holdings/simulation/finance/cash/bank?q=${encodeURIComponent(externalId)}`);
 const row=page.getByRole("row").filter({hasText:externalId});await expect(row).toHaveCount(1);await expect(row).toContainText(statementNumber);await expect(row).toContainText(status);
}

test("isolated demo completes a three-person generic payment through exact bank evidence",async({browser},testInfo)=>{
 const run=process.env.ERPX_DEMO_RUN;test.skip(!run,"Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");test.setTimeout(300_000);
 const directory=resolve(`.runtime/browser/${run}`),credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials,state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as {fixture:Fixture},invoiceId=state.fixture.controlledPaymentInvoiceId;
 const preparer=await createEngineeringBrowserContext(browser,testInfo),approver=await createEngineeringBrowserContext(browser,testInfo),treasury=await createEngineeringBrowserContext(browser,testInfo);
 try{
  const preparePage=await preparer.newPage();await signInAs(preparer,preparePage,credentials,"finance_preparer",`/holdings/simulation/finance/documents/invoices/${invoiceId}`);
  const invoiceDrawer=preparePage.getByTestId("record-detail-drawer");await expect(invoiceDrawer).toBeVisible();await expect(invoiceDrawer).toHaveAttribute("data-presentation","form-drawer");
  const prepareToggle=invoiceDrawer.locator("summary").filter({hasText:"Prepare controlled payment"});await expect(prepareToggle).toHaveCount(1);await prepareToggle.click();
  const paymentEditor=invoiceDrawer.locator("[data-record-editor]");await expect(paymentEditor).toBeVisible();await expect(paymentEditor.getByRole("heading",{name:"Prepare controlled payment",exact:true}).first()).toBeVisible();
  const paymentReference=paymentEditor.getByLabel("Payment reference",{exact:true});await expect(paymentReference).toBeVisible();const prepareForm=paymentReference.locator("xpath=ancestor::form");await expect(prepareForm).toHaveCount(1);await paymentReference.fill("DEMO-PAYMENT-125");await prepareForm.getByLabel("Planned date",{exact:true}).fill("2027-03-15");await prepareForm.getByRole("combobox",{name:"Bank ledger account",exact:true}).selectOption({label:"CLOSE-BANK · Demo operating bank"});
  const [prepared]=await Promise.all([preparePage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname==="/api/v1/holdings/simulation/finance/payments"),preparePage.waitForURL(url=>/^\/holdings\/simulation\/finance\/cash\/payments\/[^/]+\/overview$/.test(url.pathname)),prepareForm.getByRole("button",{name:"Prepare for approval",exact:true}).click()]);const preparedBody=await prepared.text();expect(prepared.ok(),preparedBody).toBe(true);const preparedPayload=JSON.parse(preparedBody) as {data?:{id?:unknown}};expect(typeof preparedPayload.data?.id).toBe("string");expect(preparedPayload.data).toMatchObject({amount:"125.00",outstanding:"125.00",status:"draft",version:1,settlementInvoiceId:invoiceId});
  const paymentPath=new URL(preparePage.url()).pathname,paymentId=String(preparedPayload.data!.id);expect(paymentPath).toBe(`/holdings/simulation/finance/cash/payments/${paymentId}/overview`);const paymentDrawer=preparePage.getByTestId("record-detail-drawer");await expect(paymentDrawer).toBeVisible();await expect(paymentDrawer).toContainText("DEMO-PAYMENT-125");await expect(paymentDrawer.getByText("draft",{exact:true})).toBeVisible();

  const approvePage=await approver.newPage();await signInAs(approver,approvePage,credentials,"finance_approver",paymentPath);
  const authorityResponse=await approver.request.get("/api/v1/holdings/simulation/iam/my-access");expect(authorityResponse.ok(),await authorityResponse.text()).toBe(true);
  const authority=(await authorityResponse.json()).data as {currentUserId:string;scopeKind:string;permissions:string[]};expect(authority.scopeKind).toBe("holding");expect(authority.permissions).toContain("finance.approve");
  const paymentRead=await approver.request.get(`/api/v1/holdings/simulation/finance/payments/${paymentId}`);expect(paymentRead.ok(),await paymentRead.text()).toBe(true);
  const paymentState=(await paymentRead.json()).data as {payment:{status:string;created_by:string}};expect(paymentState.payment.status).toBe("draft");expect(paymentState.payment.created_by).not.toBe(authority.currentUserId);
  const approveDrawer=approvePage.getByTestId("record-detail-drawer"),approveForm=approveDrawer.locator("#finance-payments-detail-lifecycle form");await expect(approveForm).toHaveCount(1);await approveForm.getByLabel("Decision evidence",{exact:true}).fill("Controller reviewed the supplier, amount, date and bank control account");const [approval]=await Promise.all([approvePage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname===`/api/v1/holdings/simulation/finance/payments/${paymentId}/approve`),approveForm.getByRole("button",{name:"Approve payment",exact:true}).click()]);expect(approval.ok(),await approval.text()).toBe(true);await expect(approveDrawer.getByText("approved",{exact:true})).toBeVisible();

  const treasuryPage=await treasury.newPage();await signInAs(treasury,treasuryPage,credentials,"treasury",paymentPath);const treasuryDrawer=treasuryPage.getByTestId("record-detail-drawer"),releaseForm=treasuryDrawer.locator("#finance-payments-detail-lifecycle form");await expect(releaseForm).toHaveCount(1);await releaseForm.getByLabel("Decision evidence",{exact:true}).fill("Treasury checked the independently approved beneficiary and payment instruction");const [release]=await Promise.all([treasuryPage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname===`/api/v1/holdings/simulation/finance/payments/${paymentId}/release`),releaseForm.getByRole("button",{name:"Release payment",exact:true}).click()]);expect(release.ok(),await release.text()).toBe(true);await expect(treasuryDrawer.getByText("released",{exact:true})).toBeVisible();
  await treasuryPage.reload();const confirmation=treasuryDrawer.locator("#finance-payments-detail-lifecycle form");await expect(confirmation).toHaveCount(1);await confirmation.getByRole("combobox",{name:"Posting period",exact:true}).selectOption(state.fixture.controlledPaymentPeriodId);await confirmation.getByRole("combobox",{name:"Exact bank transaction",exact:true}).selectOption(`${state.fixture.controlledPaymentBankTransactionId}|1`);await confirmation.getByLabel("Bank reference",{exact:true}).fill("DEMO-BANK-PAY-125");await confirmation.getByLabel("Confirmation evidence",{exact:true}).fill("Exact sandbox bank row reconciled to the released instruction");const [confirmed]=await Promise.all([treasuryPage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname===`/api/v1/holdings/simulation/finance/payments/${paymentId}/confirm`),confirmation.getByRole("button",{name:"Confirm, post and reconcile",exact:true}).click()]);expect(confirmed.ok(),await confirmed.text()).toBe(true);
  await expect(treasuryDrawer.getByText("settled",{exact:true})).toBeVisible();await expect(treasuryDrawer.getByText("THB 0.00",{exact:true})).toBeVisible();await expect(treasuryDrawer.getByText("DEMO-CONTROLLED-PAYMENT",{exact:true})).toBeVisible();await expect(treasuryDrawer.getByRole("link",{name:/Open posted payment journal/})).toBeVisible();
  await treasuryPage.reload();await expect(treasuryDrawer.getByText("settled",{exact:true})).toBeVisible();
  const paymentResponse=await treasury.request.get(`/api/v1/holdings/simulation/finance/payments/${paymentId}`),payment=(await paymentResponse.json()).data;expect(paymentResponse.ok()).toBe(true);expect(payment.payment).toMatchObject({status:"settled",outstanding_minor:0});expect(payment.allocations).toHaveLength(1);
  const invoiceResponse=await treasury.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`),invoice=(await invoiceResponse.json()).data;expect(invoice.invoice).toMatchObject({status:"paid",outstanding_minor:0});
 } finally {await Promise.all([preparer.close(),approver.close(),treasury.close()]);}
});

test("isolated Finance BankImport and BankMatch replay committed commands after lost responses while retaining newer drafts",async({browser},testInfo)=>{
 const run=process.env.ERPX_DEMO_RUN;test.skip(!run,"Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");test.setTimeout(120_000);
 const directory=resolve(`.runtime/browser/${run}`),credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials,state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as {run:string;fixture:Fixture},paymentId=state.fixture.bankMatchPaymentId;
 const preparer=await createEngineeringBrowserContext(browser,testInfo),approver=await createEngineeringBrowserContext(browser,testInfo);
 try{
  const preparePage=await preparer.newPage();await signInAs(preparer,preparePage,credentials,"finance_preparer","/holdings/simulation/finance/cash/bank?new=1");
  const statementNumber=`DEMO-REPLAY-STATEMENT-${run}`,externalId=`DEMO-REPLAY-ROW-${run}`,newerStatementNumber=`${statementNumber}-EDIT`,newerDescription="Newer statement draft retained after recovery",importPath="/api/v1/holdings/simulation/finance/bank/statements/import";
  const importAccount=preparePage.getByRole("combobox",{name:"Bank ledger account",exact:true});await importAccount.selectOption(state.fixture.closeBankAccountId);await preparePage.getByLabel("Statement number",{exact:true}).fill(statementNumber);await preparePage.getByLabel("Statement date",{exact:true}).fill("2027-03-31");await preparePage.getByLabel("Opening balance",{exact:true}).fill("0.00");await preparePage.getByLabel("Closing balance",{exact:true}).fill("125.00");await preparePage.getByLabel("Bank reference 1",{exact:true}).fill(externalId);await preparePage.getByLabel("Transaction date 1",{exact:true}).fill("2027-03-15");await preparePage.getByLabel("Description 1",{exact:true}).fill("Original imported customer receipt");await preparePage.getByLabel("Signed amount 1",{exact:true}).fill("125.00");
  const importAttempts=observePosts(preparePage,importPath),lostImport=await commitThenAbort(preparePage,importPath,201),importCard=preparePage.locator('#workspace-content [data-feature-widget="finance-bank-import-statement"]');
  const importClick=importCard.getByRole("button",{name:"Validate and import",exact:true}).click(),importOutcome=await lostImport.committed;if(!importOutcome.ok)throw importOutcome.failure;const originalImport=importOutcome.value;
  expect(JSON.parse(originalImport.body)).toEqual({bankAccount:state.fixture.closeBankAccountId,statementNumber,statementDate:"2027-03-31",opening:"0.00",closing:"125.00",sourceHash:"server-computed",rows:[{externalId,transactionDate:"2027-03-15",description:"Original imported customer receipt",amount:"125.00",currency:"THB"}]});expect(originalImport.receipt).toMatchObject({id:`statement-${originalImport.key}`,status:"imported",imported:1});
  const statementField=preparePage.getByLabel("Statement number",{exact:true}),descriptionField=preparePage.getByLabel("Description 1",{exact:true});await statementField.fill(newerStatementNumber);await descriptionField.fill(newerDescription);await expect(statementField).toHaveValue(newerStatementNumber);await expect(descriptionField).toHaveValue(newerDescription);expect(importAttempts.posts).toEqual([{key:originalImport.key,body:originalImport.body}]);
  lostImport.release();const importLostOutcome=await lostImport.delivered;if(!importLostOutcome.ok)throw importLostOutcome.failure;await importClick;await preparePage.unroute(lostImport.url,lostImport.handler);await expect(preparePage.getByRole("status").filter({hasText:"This statement request is not confirmed yet."})).toBeVisible();await expect(importCard.getByRole("button",{name:"Retry original bank import",exact:true})).toBeEnabled();
  await expect(importCard.getByRole("button",{name:"Validate and import",exact:true})).toBeDisabled();expect(importAttempts.posts).toEqual([{key:originalImport.key,body:originalImport.body}]);
  preparePage.once("dialog",dialog=>{void dialog.accept();});await preparePage.reload();await expect(importCard.getByRole("button",{name:"Retry original bank import",exact:true})).toBeVisible();await expect(preparePage.getByLabel("Statement number",{exact:true})).toHaveValue(newerStatementNumber);await expect(preparePage.getByLabel("Description 1",{exact:true})).toHaveValue(newerDescription);await expect(importAttempts.posts).toEqual([{key:originalImport.key,body:originalImport.body}]);
  const importReadPage=await preparer.newPage();await assertPersistedBankRow(importReadPage,externalId,statementNumber,"unmatched");await importReadPage.close();
  const importReplayWait=preparePage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname===importPath);await importCard.getByRole("button",{name:"Retry original bank import",exact:true}).click();const importReplay=await importReplayWait,importReplayData=(await importReplay.json()).data;
  expect(importReplay.status()).toBe(201);expect(importReplayData).toEqual(originalImport.receipt);expect(importReplay.request().postData()).toBe(originalImport.body);expect(await importReplay.request().headerValue("Idempotency-Key")).toBe(originalImport.key);expect(importAttempts.posts).toEqual([{key:originalImport.key,body:originalImport.body},{key:originalImport.key,body:originalImport.body}]);
  await expect(importCard.getByRole("status").filter({hasText:"The earlier statement was imported."})).toBeVisible();await expect(statementField).toHaveValue(newerStatementNumber);await expect(descriptionField).toHaveValue(newerDescription);await expect(importCard.getByRole("button",{name:"Validate and import",exact:true})).toBeDisabled();importAttempts.stop();

  const importedBankRowId=`bankrow-${originalImport.key}-0`,matchPath=`/api/v1/holdings/simulation/finance/bank/transactions/${importedBankRowId}/reconcile`,matchPage=await approver.newPage();await signInAs(approver,matchPage,credentials,"finance_approver",`/holdings/simulation/finance/cash/bank?match=${encodeURIComponent(importedBankRowId)}`);
  await expect(matchPage.getByRole("heading",{name:"Review bank match",exact:true})).toBeVisible();await expect(matchPage.getByText(externalId,{exact:true}).first()).toBeVisible();
  const matchForm=matchPage.locator('#workspace-content form').filter({has:matchPage.getByRole("button",{name:"Confirm match",exact:true})}),paymentSelect=matchForm.getByRole("combobox",{name:"Match payment",exact:true}),proofField=matchForm.getByLabel("Review evidence",{exact:true});await expect(matchForm).toHaveCount(1);await paymentSelect.selectOption(paymentId);await proofField.fill("Original review evidence for the exact seeded bank movement.");
  const originalProof="Original review evidence for the exact seeded bank movement.";await expect(proofField).toHaveValue(originalProof);
  const newerProof="Newer review evidence retained after recovering the original match.",matchAttempts=observePosts(matchPage,matchPath),lostMatch=await commitThenAbort(matchPage,matchPath,200),matchClick=matchForm.getByRole("button",{name:"Confirm match",exact:true}).click(),matchOutcome=await lostMatch.committed;if(!matchOutcome.ok)throw matchOutcome.failure;const originalMatch=matchOutcome.value;
  expect(JSON.parse(originalMatch.body)).toEqual({expectedVersion:1,matchedType:"payment",matchedId:paymentId,proof:originalProof,attachmentSetIds:[]});expect(originalMatch.receipt).toEqual({id:importedBankRowId,status:"matched",version:2});
  await expect(paymentSelect).toHaveValue(paymentId);await expect(proofField).toHaveValue(originalProof);await proofField.fill(newerProof);await expect(proofField).toHaveValue(newerProof);expect(matchAttempts.posts).toEqual([{key:originalMatch.key,body:originalMatch.body}]);lostMatch.release();const matchLostOutcome=await lostMatch.delivered;if(!matchLostOutcome.ok)throw matchLostOutcome.failure;await matchClick;await matchPage.unroute(lostMatch.url,lostMatch.handler);await expect(matchPage.getByRole("status").filter({hasText:"The original match request is not confirmed yet."})).toBeVisible();await expect(matchPage.getByRole("button",{name:"Retry original bank match",exact:true})).toBeEnabled();await expect(matchForm.getByRole("button",{name:"Confirm match",exact:true})).toBeDisabled();expect(matchAttempts.posts).toEqual([{key:originalMatch.key,body:originalMatch.body}]);
  matchPage.once("dialog",dialog=>{void dialog.accept();});await matchPage.reload();await expect(matchPage.getByRole("button",{name:"Retry original bank match",exact:true})).toBeVisible();await expect(matchPage.getByRole("combobox",{name:"Match payment",exact:true})).toHaveValue(paymentId);await expect(matchPage.getByLabel("Review evidence",{exact:true})).toHaveValue(newerProof);await expect(matchForm.getByRole("button",{name:"Confirm match",exact:true})).toBeDisabled();await expect(matchAttempts.posts).toEqual([{key:originalMatch.key,body:originalMatch.body}]);
  const matchReadPage=await approver.newPage();await assertPersistedBankRow(matchReadPage,externalId,statementNumber,"matched");await matchReadPage.close();
  const matchReplayWait=matchPage.waitForResponse(response=>response.request().method()==="POST"&&new URL(response.url()).pathname===matchPath);await matchPage.getByRole("button",{name:"Retry original bank match",exact:true}).click();const matchReplay=await matchReplayWait,matchReplayData=(await matchReplay.json()).data;
  expect(matchReplay.status()).toBe(200);expect(matchReplayData).toEqual(originalMatch.receipt);expect(matchReplay.request().postData()).toBe(originalMatch.body);expect(await matchReplay.request().headerValue("Idempotency-Key")).toBe(originalMatch.key);expect(matchAttempts.posts).toEqual([{key:originalMatch.key,body:originalMatch.body},{key:originalMatch.key,body:originalMatch.body}]);
  await expect(matchPage.getByRole("status").filter({hasText:"The earlier match was saved."})).toBeVisible();await expect(proofField).toHaveValue(newerProof);await expect(matchForm.getByRole("button",{name:"Confirm match",exact:true})).toBeDisabled();matchAttempts.stop();
 }finally{await Promise.all([preparer.close(),approver.close()]);}
});
