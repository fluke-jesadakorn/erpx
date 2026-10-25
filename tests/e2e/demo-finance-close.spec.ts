import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { EngineeringFinanceCloseBrowser, EngineeringFinanceCloseCompany } from "../../scripts/fixtures/engineering-finance-close-browser";

async function signInAs(context:BrowserContext,page:Page,credentials:EngineeringFinanceCloseBrowser,company:EngineeringFinanceCloseCompany,role:"finance_preparer"|"finance_approver",periodId:string) {
  const user=company.users[role==="finance_preparer"?"preparer":"approver"];
  expect(user,`${role} demo account`).toBeTruthy();
  const origin=process.env.ERPX_E2E_BASE_URL??"http://127.0.0.1:3104";
  const login=await context.request.post("/api/auth/sign-in/email",{headers:{Origin:origin},data:{email:user.email,password:credentials.password}});
  expect(login.ok()).toBe(true);
  const endpoint="/api/v1/holdings/simulation/assignments/active";
  let active=(await(await context.request.get(endpoint)).json()).data;
  const assignment=active.options.find((item:{id:string;role:string;companyId:string})=>item.role===role&&item.companyId===company.companyId&&item.id===user.assignmentId);
  expect(assignment,`${role} current fictional close Company assignment`).toBeTruthy();
  if(active.assignmentId!==assignment.id){
    const selected=await context.request.post(endpoint,{headers:{Origin:origin},data:{assignmentId:assignment.id,revision:active.revision}});
    expect(selected.ok()).toBe(true);
    active=(await(await context.request.get(endpoint)).json()).data;
  }
  expect(active.assignmentId).toBe(assignment.id);
  await page.goto(`/holdings/simulation/finance/close?periodId=${encodeURIComponent(periodId)}`);
  await expect(page.getByRole("heading",{name:"Period close center"})).toBeVisible();
  await expect(page.locator("[data-work-company]")).toHaveAttribute("data-work-company",company.companyId);
}

function period(page:Page,name:string){return page.locator("#workspace-content").getByRole("heading",{name,exact:true}).locator("xpath=ancestor::article[1]");}

async function openPeriodWorkspace(page:Page,name:string,periodId:string){
  await page.locator('#workspace-content').getByRole('link',{name,exact:true}).click();
  await expect(page).toHaveURL(url=>url.searchParams.get('detailKind')==='finance_period'&&url.searchParams.get('detailId')===periodId);
  const drawer=page.getByTestId('record-detail-drawer');await expect(drawer).toHaveCount(1);await expect(drawer).toBeVisible();
  const fullPage=drawer.getByRole('link',{name:'Open full page ↗',exact:true});await expect(fullPage).toHaveAttribute('href',`/holdings/simulation/finance/close?periodId=${encodeURIComponent(periodId)}`);await fullPage.click();
  await expect(page).toHaveURL(url=>url.pathname==='/holdings/simulation/finance/close'&&url.searchParams.get('periodId')===periodId&&!url.searchParams.has('detailKind'));
  await expect(drawer).toBeHidden();await expect(period(page,name)).toBeVisible();
}

async function expectOpeningReconciled(card:ReturnType<typeof period>){
  for(const [kind,amount] of [["bank","1000.00"],["receivable","300.00"],["payable","200.00"]] as const){
    const row=card.getByRole("cell",{name:kind,exact:true}).locator("xpath=ancestor::tr[1]");
    await expect(row).toHaveCount(1);
    await expect(row.getByRole("cell").nth(2)).toHaveText(`THB ${amount}`);
    await expect(row.getByRole("cell").nth(3)).toHaveText(`THB ${amount}`);
    await expect(row.getByRole("cell").nth(4)).toHaveText("THB 0.00");
  }
}

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
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/engineering-finance-close.json`),"utf8")) as EngineeringFinanceCloseBrowser;
  expect(credentials.run).toBe(run);
  const company=credentials.companies[testInfo.project.name as "desktop"|"mobile"];
  expect(company,"independent close sources for the current browser project").toBeTruthy();
  const periodIds=company.periodIds;
  const preparer=await createEngineeringBrowserContext(browser,testInfo),approver=await createEngineeringBrowserContext(browser,testInfo);
  try{
    const preparePage=await preparer.newPage();
    await signInAs(preparer,preparePage,credentials,company,"finance_preparer",periodIds[0]);
    const january=period(preparePage,"January 2027");
    await expect(january).toContainText("THB 0.00");
    await expect(january).toContainText("Statement DEMO-JAN-2027 at period end");
    await expectOpeningReconciled(january);
    await january.getByRole("button",{name:"Check readiness and begin close"}).click();
    await expect(january).toContainText("closing");
    await waitForPeriodState(preparer,periodIds[0], "closing");

    // Open the approval workspace only after the preparer has transitioned the
    // period. A fresh page avoids carrying an earlier server-rendered `open`
    // state into the approval step when the two independent sessions race.
    const approvePage=await approver.newPage();
    await signInAs(approver,approvePage,credentials,company,"finance_approver",periodIds[0]);
    const januaryApproval=period(approvePage,"January 2027");
    await expect(januaryApproval).toContainText("closing");
    await januaryApproval.getByLabel("Review evidence").fill("Independent review of January GL, AR/AP schedules, and period-end bank statement");
    await januaryApproval.getByRole("button",{name:"Approve period close"}).click();
    await expect(januaryApproval).toContainText("closed");

    await openPeriodWorkspace(preparePage,'February 2027',periodIds[1]);
    const february=period(preparePage,"February 2027");
    await expect(february).toContainText("Statement DEMO-FEB-2027 at period end");
    await expectOpeningReconciled(february);
    await february.getByRole("button",{name:"Check readiness and begin close"}).click();
    await expect(february).toContainText("closing");
    await waitForPeriodState(preparer,periodIds[1], "closing");

    await approvePage.goto(`/holdings/simulation/finance/close?periodId=${encodeURIComponent(periodIds[1])}`,{waitUntil:"domcontentloaded"});
    const februaryApproval=period(approvePage,"February 2027");
    await februaryApproval.getByLabel("Review evidence").fill("Independent review of February carry-forward and period-end bank statement");
    await februaryApproval.getByRole("button",{name:"Approve period close"}).click();
    await expect(februaryApproval).toContainText("closed");
    await waitForPeriodState(approver,periodIds[1],"closed");
    await openPeriodWorkspace(approvePage,'January 2027',periodIds[0]);
    await expect(period(approvePage,"January 2027")).toContainText("closed");
    await expectOpeningReconciled(period(approvePage,"January 2027"));
    await waitForPeriodState(approver,periodIds[0],"closed");
  } finally { await Promise.all([preparer.close(),approver.close()]); }
});
