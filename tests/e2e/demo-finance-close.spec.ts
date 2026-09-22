import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type Credentials={password:string;users:Array<{email:string;role:string}>};
type DemoState={fixture:{closePeriodIds:string[]}};

async function signInAs(context:BrowserContext,page:Page,credentials:Credentials,role:"finance_preparer"|"finance_approver") {
  const user=credentials.users.find(item=>item.role===role);
  expect(user,`${role} demo account`).toBeTruthy();
  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:user!.email,password:credentials.password}});
  expect(login.ok()).toBe(true);
  const endpoint="/api/v1/holdings/simulation/assignments/active";
  let active=(await(await context.request.get(endpoint)).json()).data;
  const assignment=active.options.find((item:{role:string})=>item.role===role);
  expect(assignment,`${role} assignment`).toBeTruthy();
  if(active.assignmentId!==assignment.id){
    const selected=await context.request.post(endpoint,{data:{assignmentId:assignment.id,revision:active.revision}});
    expect(selected.ok()).toBe(true);
    active=(await(await context.request.get(endpoint)).json()).data;
  }
  expect(active.assignmentId).toBe(assignment.id);
  await page.goto("/holdings/simulation/finance/close");
  await expect(page.getByRole("heading",{name:"Period close center"})).toBeVisible();
}

function period(page:Page,name:string){return page.locator("article").filter({has:page.getByRole("heading",{name,exact:true})});}

async function waitForPeriodState(context:BrowserContext,periodId:string,state:string){
  await expect.poll(async()=>{
    const response=await context.request.get(`/api/v1/holdings/simulation/finance/periods`);
    expect(response.ok()).toBe(true);
    const payload=(await response.json()) as {data:Array<{id:string;state:string}>};
    return payload.data.find(item=>item.id===periodId)?.state;
  },{timeout:20_000}).toBe(state);
}

test("isolated demo closes two reconciled periods with independent users",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(90_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`),"utf8")) as DemoState;
  const preparer=await browser.newContext(testInfo.project.use),approver=await browser.newContext(testInfo.project.use);
  try{
    const preparePage=await preparer.newPage();
    await signInAs(preparer,preparePage,credentials,"finance_preparer");
    const january=period(preparePage,"January 2027");
    await expect(january).toContainText("THB 0.00");
    await expect(january).toContainText("Statement DEMO-JAN-2027 at period end");
    await january.getByRole("button",{name:"Check readiness and begin close"}).click();
    await expect(january).toContainText("closing");
    await waitForPeriodState(preparer,state.fixture.closePeriodIds[0]!, "closing");

    // Open the approval workspace only after the preparer has transitioned the
    // period. A fresh page avoids carrying an earlier server-rendered `open`
    // state into the approval step when the two independent sessions race.
    const approvePage=await approver.newPage();
    await signInAs(approver,approvePage,credentials,"finance_approver");
    const januaryApproval=period(approvePage,"January 2027");
    await expect(januaryApproval).toContainText("closing");
    await januaryApproval.getByLabel("Review evidence").fill("Independent review of January GL, AR/AP schedules, and period-end bank statement");
    await januaryApproval.getByRole("button",{name:"Approve period close"}).click();
    await expect(januaryApproval).toContainText("closed");

    await preparePage.reload();
    const february=period(preparePage,"February 2027");
    await expect(february).toContainText("Statement DEMO-FEB-2027 at period end");
    await february.getByRole("button",{name:"Check readiness and begin close"}).click();
    await expect(february).toContainText("closing");
    await waitForPeriodState(preparer,state.fixture.closePeriodIds[1]!, "closing");

    await approvePage.goto("/holdings/simulation/finance/close",{waitUntil:"domcontentloaded"});
    const februaryApproval=period(approvePage,"February 2027");
    await februaryApproval.getByLabel("Review evidence").fill("Independent review of February carry-forward and period-end bank statement");
    await februaryApproval.getByRole("button",{name:"Approve period close"}).click();
    await expect(februaryApproval).toContainText("closed");
    await expect(period(approvePage,"January 2027")).toContainText("closed");
  } finally { await Promise.all([preparer.close(),approver.close()]); }
});
