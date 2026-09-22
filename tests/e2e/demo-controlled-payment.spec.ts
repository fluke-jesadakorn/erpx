import { expect,test,type BrowserContext,type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type Role="finance_preparer"|"finance_approver"|"treasury";
type Credentials={password:string;users:Array<{email:string;role:string;roles?:string[]}>};
type Fixture={controlledPaymentInvoiceId:string;controlledPaymentPeriodId:string;controlledPaymentBankTransactionId:string};

async function signInAs(context:BrowserContext,page:Page,credentials:Credentials,role:Role,path:string){
 const user=credentials.users.find(item=>item.role===role||item.roles?.includes(role));expect(user,`${role} demo account`).toBeTruthy();
 const login=await context.request.post("/api/auth/sign-in/email",{data:{email:user!.email,password:credentials.password}});expect(login.ok()).toBe(true);
 const endpoint="/api/v1/holdings/simulation/assignments/active";let active=(await(await context.request.get(endpoint)).json()).data;
 const assignment=active.options.find((item:{role:string})=>item.role===role);expect(assignment,`${role} assignment`).toBeTruthy();
 if(active.assignmentId!==assignment.id){const selected=await context.request.post(endpoint,{data:{assignmentId:assignment.id,revision:active.revision}});expect(selected.ok()).toBe(true);active=(await(await context.request.get(endpoint)).json()).data;}
 expect(active.assignmentId).toBe(assignment.id);await page.goto(path);await expect(page.getByText(new RegExp(`${role.replaceAll("_"," ")} ·`,"i")).first()).toBeVisible();
}

test("isolated demo completes a three-person generic payment through exact bank evidence",async({browser},testInfo)=>{
 const run=process.env.ERPX_DEMO_RUN;test.skip(!run,"Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");test.setTimeout(90_000);
 const directory=resolve(`.runtime/browser/${run}`),credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials,state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as {fixture:Fixture},invoiceId=state.fixture.controlledPaymentInvoiceId;
 const preparer=await browser.newContext(testInfo.project.use),approver=await browser.newContext(testInfo.project.use),treasury=await browser.newContext(testInfo.project.use);
 try{
  const preparePage=await preparer.newPage();await signInAs(preparer,preparePage,credentials,"finance_preparer",`/holdings/simulation/finance/invoices/${invoiceId}`);
  await expect(preparePage.getByRole("heading",{name:"Prepare controlled payment",exact:true})).toBeVisible();
  await preparePage.getByLabel("Payment reference").fill("DEMO-PAYMENT-125");await preparePage.getByLabel("Planned date").fill("2027-03-15");await preparePage.getByLabel("Bank ledger account").selectOption({label:"CLOSE-BANK · Demo operating bank"});
  await Promise.all([preparePage.waitForURL(/\/finance\/payments\//),preparePage.getByRole("button",{name:"Prepare for approval"}).click()]);
  const paymentPath=new URL(preparePage.url()).pathname,paymentId=paymentPath.split("/").at(-1)!;await expect(preparePage.getByText("draft",{exact:true})).toBeVisible();

  const approvePage=await approver.newPage();await signInAs(approver,approvePage,credentials,"finance_approver",paymentPath);await approvePage.getByLabel("Decision evidence").fill("Controller reviewed the supplier, amount, date and bank control account");await approvePage.getByRole("button",{name:"Approve payment"}).click();await expect(approvePage.getByText("approved",{exact:true})).toBeVisible();

  const treasuryPage=await treasury.newPage();await signInAs(treasury,treasuryPage,credentials,"treasury",paymentPath);await treasuryPage.getByLabel("Decision evidence").fill("Treasury checked the independently approved beneficiary and payment instruction");await treasuryPage.getByRole("button",{name:"Release payment"}).click();await expect(treasuryPage.getByText("released",{exact:true})).toBeVisible();
  await treasuryPage.getByLabel("Posting period").selectOption(state.fixture.controlledPaymentPeriodId);await treasuryPage.getByLabel("Exact bank transaction").selectOption(`${state.fixture.controlledPaymentBankTransactionId}|1`);await treasuryPage.getByLabel("Bank reference").fill("DEMO-BANK-PAY-125");await treasuryPage.getByLabel("Confirmation evidence").fill("Exact sandbox bank row reconciled to the released instruction");await treasuryPage.getByRole("button",{name:"Confirm, post and reconcile"}).click();
  await expect(treasuryPage.getByText("settled",{exact:true})).toBeVisible();await expect(treasuryPage.getByText("THB 0.00",{exact:true})).toBeVisible();await expect(treasuryPage.getByText("DEMO-CONTROLLED-PAYMENT",{exact:true})).toBeVisible();await expect(treasuryPage.getByRole("link",{name:/Open posted payment journal/})).toBeVisible();
  await treasuryPage.reload();await expect(treasuryPage.getByText("settled",{exact:true})).toBeVisible();
  const paymentResponse=await treasury.request.get(`/api/v1/holdings/simulation/finance/payments/${paymentId}`),payment=(await paymentResponse.json()).data;expect(paymentResponse.ok()).toBe(true);expect(payment.payment).toMatchObject({status:"settled",outstanding_minor:0});expect(payment.allocations).toHaveLength(1);
  const invoiceResponse=await treasury.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`),invoice=(await invoiceResponse.json()).data;expect(invoice.invoice).toMatchObject({status:"paid",outstanding_minor:0});
 } finally {await Promise.all([preparer.close(),approver.close(),treasury.close()]);}
});
