import {expect,test,type Browser,type BrowserContext,type Page,type TestInfo} from '@playwright/test';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {prepareProductPurchases,verifyProductHost} from '../../support/browser/product-host-session';
import {signIn} from '../../support/helpers/context-switching-browser';
import {readProductHostLease,workRequestDraftProductHostCase} from '../../../scripts/testing/product-host-cohort';
import {randomUUID} from 'node:crypto';
import {productIds} from '../../../src/lib/commercial/product-registry';

// Missing owned mode is a failure, never an accepted skip/Console fallback.
for(const product of productIds)test(`isolated ${product} Account Apps SSO reaches its actual owner landing and API`,async({browser},testInfo)=>{
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,product);
});

import {observeAssetProductHost} from '../../support/browser/asset-ui-observation';

test('isolated Assets native drawer and dirty product switch observations across the appearance matrix',async({browser},testInfo)=>{
 test.setTimeout(360_000);
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'assets',async({page,context,origin,companyId,userId})=>{
  if(origin!=='https://assets-dev-erpx.jesadakorn.com')throw Error('EXACT_ASSETS_PRODUCT_ORIGIN_REQUIRED');
  await observeAssetProductHost(page,context,testInfo,companyId,userId);
 });
});


import {observeFinanceInvoiceDrawer} from '../../support/browser/finance-invoice-ui-observation';

test('isolated Finance native invoice drawer keeps dirty work across the appearance matrix',async({browser},testInfo)=>{
 test.setTimeout(600_000);
 if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'finance',async({page,context,origin,companyId,userId})=>{
  if(origin!=='https://finance-dev-erpx.jesadakorn.com')throw Error('EXACT_FINANCE_PRODUCT_ORIGIN_REQUIRED');
  await observeFinanceInvoiceDrawer(page,context,testInfo,origin,companyId,userId);
 });
});

test(workRequestDraftProductHostCase,async({browser},testInfo)=>{
 test.setTimeout(240_000);if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'work',async({page,context,origin,companyId,scopedBusinessWorkflow})=>{
  expect(origin).toBe('https://work-dev-erpx.jesadakorn.com');expect(scopedBusinessWorkflow).toBe('work.request-draft');
  const title=`Migration Work draft ${randomUUID()}`,description='A service request created and cancelled through the isolated Work product host.';
  const supportOrigin='https://support-dev-erpx.jesadakorn.com',workPath='/holdings/simulation/work/actions',requestsPath='/api/v1/holdings/simulation/requests',requestApi=(host:string,id:string)=>host+requestsPath+'/'+encodeURIComponent(id),workUrl=()=>origin+workPath+`?queue=all&type=service_request&q=${encodeURIComponent(title)}`;
  const keys:string[]=[];let requestId:string|undefined;
  const markIsolatedWorkflow=async(route:import('@playwright/test').Route)=>{
   const request=route.request(),url=new URL(request.url());if(request.method()!=='POST')return route.continue();
   const allowed=url.pathname===requestsPath||url.pathname===requestsPath+'/preview'||/^\/api\/v1\/holdings\/simulation\/requests\/[A-Za-z0-9_-]{1,180}\/actions$/.test(url.pathname);if(!allowed||![origin,supportOrigin].includes(url.origin))return route.continue();
   const headers={...request.headers(),'x-erpx-isolated-product-workflow':'work.request-draft'};
   if(url.origin===origin&&url.pathname===requestsPath){keys.push(request.headers()['idempotency-key']??'');if(keys.length===1)return route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'Isolated Work host retry check'}})});}
   return route.continue({headers});
  };
  await page.route(origin+requestsPath+'**',markIsolatedWorkflow);await page.route(supportOrigin+requestsPath+'**',markIsolatedWorkflow);
  const cancelDraft=async(id:string)=>{await page.goto(workUrl(),{waitUntil:'domcontentloaded'});const row=page.getByRole('region',{name:'Work results'}).getByRole('link',{name:new RegExp(title)});await expect(row).toHaveCount(1);await row.click();const response=page.waitForResponse(reply=>reply.url()===requestApi(supportOrigin,id)+'/actions'&&reply.request().method()==='POST');await page.getByRole('button',{name:'Cancel draft',exact:true}).click();const cancelled=await response;expect(cancelled.ok(),await cancelled.text()).toBe(true);};
  try{
   await page.goto(workUrl(),{waitUntil:'domcontentloaded'});await expect(page.getByRole('heading',{name:'My work',exact:true})).toBeVisible();await page.getByRole('button',{name:'New',exact:true}).click();await page.getByRole('dialog',{name:'New work'}).getByRole('link',{name:'Service request',exact:true}).click();
   await expect(page.getByRole('dialog',{name:'Service request',exact:true})).toBeVisible();await page.getByRole('textbox',{name:'Title',exact:true}).fill(title);await page.getByRole('textbox',{name:'Describe the issue',exact:true}).fill(description);
   await page.getByRole('button',{name:'Save draft',exact:true}).click();await expect(page.getByRole('alert').filter({hasText:'Isolated Work host retry check'})).toBeVisible();await expect(page.getByRole('textbox',{name:'Title',exact:true})).toHaveValue(title);const savedResponse=page.waitForResponse(reply=>reply.url()===origin+requestsPath&&reply.request().method()==='POST');await page.getByRole('button',{name:'Save draft',exact:true}).click();const saved=await savedResponse;expect(saved.ok(),await saved.text()).toBe(true);const result=(await saved.json()).data;requestId=String(result.id);expect(requestId).toBeTruthy();expect(keys).toHaveLength(2);expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(requestId).toBe(`request-${keys[0]}`);await expect(page.getByRole('textbox',{name:'Title',exact:true})).toHaveValue(title);
   const persisted=await context.request.get(requestApi(origin,requestId));try{expect(persisted.status()).toBe(200);expect((await persisted.json()).data.request).toMatchObject({title,description,state:'draft'});}finally{await persisted.dispose();}
   await cancelDraft(requestId);await page.reload({waitUntil:'domcontentloaded'});await expect(page.getByRole('button',{name:'Save draft',exact:true})).toHaveCount(0);const cancelled=await context.request.get(requestApi(origin,requestId));let status:string;try{expect(cancelled.status()).toBe(200);status=String((await cancelled.json()).data.request.state);expect(status).toBe('cancelled');}finally{await cancelled.dispose();}
   await testInfo.attach('work-product-host-request-lifecycle',{body:Buffer.from(JSON.stringify({schema:'erpx-product-host-work-request/v1',origin,companyId,requestId,title,description,idempotencyKeys:keys,retryRetainedOriginalKey:keys[0]===keys[1],state:status,isolatedBusinessWorkflow:scopedBusinessWorkflow,wholeProductAcceptance:false})),contentType:'application/json'});
  }finally{
   if(requestId){const current=await context.request.get(requestApi(origin,requestId));let state:string|undefined;try{if(current.ok())state=String((await current.json()).data.request.state);}finally{await current.dispose();}if(state==='draft')await cancelDraft(requestId);}
  }
 });
});


const root = process.cwd();
const accountOrigin = "https://account-dev-erpx.jesadakorn.com";
const appsOrigin = "https://apps-dev-erpx.jesadakorn.com";

type SimulationUser = {
  email: string;
  userId: string;
  role: string;
  scope?: "branch" | "department";
};
type SimulationState = {
  run: string;
  prefix: string;
  ready?: boolean;
  productHosts?: boolean;
  sourceFingerprint?: string;
  release?: { id: string; fileManifestSha256: string };
  fixture: {
    companyId: string;
    hospitality: { companyId: string };
  };
};
type SimulationCredentials = { password: string; users: SimulationUser[] };
type ProductHostFixture = {
  state: SimulationState;
  credentials: SimulationCredentials;
  directory: string;
};

async function loadProductHostFixture(): Promise<ProductHostFixture> {
  const leasePath = process.env.ERPX_PRODUCT_HOST_LEASE;
  const run = process.env.ERPX_DEMO_RUN;
  if (!leasePath || !run) throw new Error("OWNED_PRODUCT_HOST_SIMULATION_REQUIRED");

  const lease = await readProductHostLease(root, leasePath);
  expect(lease.run).toBe(run);
  const directory = resolve(root, ".runtime/browser", run);
  const [stateText, credentialsText] = await Promise.all([
    readFile(resolve(directory, "state.json"), "utf8"),
    readFile(resolve(directory, "credentials.json"), "utf8"),
  ]);
  const state = JSON.parse(stateText) as SimulationState;
  const credentials = JSON.parse(credentialsText) as SimulationCredentials;

  expect(state).toMatchObject({
    run,
    prefix: `erpx_browser_${run}`,
    ready: true,
    productHosts: true,
    sourceFingerprint: lease.sourceFingerprint,
    release: lease.release,
  });
  expect(credentials.password.length).toBeGreaterThan(0);
  return { state, credentials, directory };
}

async function signedInAccountContext(
  browser: Browser,
  testInfo: TestInfo,
  credentials: SimulationCredentials,
  user: SimulationUser,
): Promise<BrowserContext> {
  const context = await browser.newContext({
    ...testInfo.project.use,
    baseURL: accountOrigin,
    ignoreHTTPSErrors: false,
    serviceWorkers: "block",
    proxy: { server: "http://127.0.0.1:3102" },
  });
  const previousBaseUrl = process.env.ERPX_E2E_BASE_URL;
  process.env.ERPX_E2E_BASE_URL = accountOrigin;
  try {
    const signedIn = await signIn(context, user.email, credentials.password);
    expect(signedIn.id).toBe(user.userId);
  } catch (error) {
    await context.close();
    throw error;
  } finally {
    if (previousBaseUrl === undefined) delete process.env.ERPX_E2E_BASE_URL;
    else process.env.ERPX_E2E_BASE_URL = previousBaseUrl;
  }
  return context;
}

async function signOut(context: BrowserContext) {
  const response = await context.request.post(`${accountOrigin}/api/auth/sign-out`, {
    headers: { Origin: accountOrigin },
    data: {},
  });
  try {
    expect(response.status()).toBe(200);
  } finally {
    await response.dispose();
  }
}

function productTargetFromAppsHref(href: string) {
  const handoff = new URL(href, appsOrigin);
  const next = handoff.searchParams.get("next");
  if (!next) return null;
  const launch = new URL(next, handoff.origin);
  if (launch.pathname !== "/launch") return null;
  const target = launch.searchParams.get("next");
  const companyId = launch.searchParams.get("companyId");
  return target && companyId ? { origin: handoff.origin, target, companyId } : null;
}

async function productTiles(page: Page) {
  const links = await page.locator("a[data-product-switch].tile").all();
  const result: { href: string; companyId: string; target: string; origin: string }[] = [];
  for (const link of links) {
    const rawHref = await link.getAttribute("href");
    if (!rawHref) continue;
    const parsed = productTargetFromAppsHref(new URL(rawHref, page.url()).href);
    if (parsed) result.push({ href: new URL(rawHref, page.url()).href, ...parsed });
  }
  return result;
}

async function openApps(page: Page) {
  await page.goto(`${accountOrigin}/my-profile`);
  await expect(page.getByRole("heading", { name: /Your account|บัญชีของคุณ/ })).toBeVisible();
  await page.getByRole("link", { name: "Apps", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`^${appsOrigin.replaceAll(".", "\\.")}/`));
  await expect(page.getByRole("heading", { name: /Choose a workspace|เลือกพื้นที่ทำงาน/ })).toBeVisible();
}

async function selectVisibleTile(page: Page, href: string) {
  const links = await page.locator("a[data-product-switch].tile").all();
  for (const link of links) {
    const rawHref = await link.getAttribute("href");
    if (rawHref && new URL(rawHref, page.url()).href === href) {
      await link.click();
      return;
    }
  }
  throw new Error("SELECTED_APPS_TILE_DISAPPEARED");
}

async function signOutAndClose(context: BrowserContext) {
  try {
    await signOut(context);
  } finally {
    await context.close();
  }
}

const scopedUsers = [
  { email: "branch@simulation.test", scope: "branch" as const, scopeName: "Branch" },
  { email: "department@simulation.test", scope: "department" as const, scopeName: "Department" },
];

for (const { email, scope, scopeName } of scopedUsers) {
  test(`Apps opens a pre-entitled ${scopeName} workspace for its scoped user`, async ({ browser }, testInfo) => {
    if (!process.env.ERPX_PRODUCT_HOST_LEASE || !process.env.ERPX_DEMO_RUN)
      throw new Error("ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED");
    const fixture = await loadProductHostFixture();
    await prepareProductPurchases(browser, testInfo);
    const user = fixture.credentials.users.find(candidate => candidate.email === email && candidate.scope === scope);
    expect(user, `The isolated fixture includes ${email} with a ${scope} assignment`).toBeTruthy();
    const context = await signedInAccountContext(browser, testInfo, fixture.credentials, user!);
    try {
      const page = await context.newPage();
      await openApps(page);
      const candidate = (await productTiles(page)).find(tile => {
        if (tile.companyId !== fixture.state.fixture.companyId) return false;
        const pathname = new URL(tile.target, tile.origin).pathname;
        return scope === "branch"
          ? pathname.includes("/branches/") && !pathname.includes("/departments/")
          : pathname.includes("/departments/");
      });
      expect(candidate, `The exact isolated ${scope} Company context exposes a product after its disposable grant setup`).toBeTruthy();

      await selectVisibleTile(page, candidate!.href);
      await expect.poll(() => new URL(page.url()).origin).toBe(candidate!.origin);
      let workContext = page.locator("[data-work-company]");
      if (await workContext.count() === 0) {
        await expect(page.getByRole("heading", { name: /Choose a work role|เลือกบทบาททำงาน/ })).toBeVisible();
        await page.getByRole("button", { name: /Use this role|ใช้บทบาทนี้/ }).first().click();
        workContext = page.locator("[data-work-company]");
      }
      await expect(workContext).toHaveAttribute("data-work-company", fixture.state.fixture.companyId);
      await expect(workContext).toHaveAttribute("data-work-user", user!.userId);
      await expect(workContext).toHaveAttribute("data-work-stale", "false");
      const landedPath = new URL(page.url()).pathname;
      expect(landedPath).toContain("/holdings/simulation/");
      if (scope === "branch") {
        expect(landedPath).toContain("/branches/");
        expect(landedPath).not.toContain("/departments/");
      } else {
        expect(landedPath).toContain("/departments/");
      }
    } finally {
      await signOutAndClose(context);
    }
  });
}

test("Branch user cannot open another Company through an Apps product context", async ({ browser }, testInfo) => {
  if (!process.env.ERPX_PRODUCT_HOST_LEASE || !process.env.ERPX_DEMO_RUN)
    throw new Error("ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED");
  const fixture = await loadProductHostFixture();
  await prepareProductPurchases(browser, testInfo);
  const user = fixture.credentials.users.find(candidate => candidate.email === "branch@simulation.test");
  expect(user, "The isolated fixture includes a Branch user").toBeTruthy();
  const context = await signedInAccountContext(browser, testInfo, fixture.credentials, user!);
  try {
    const page = await context.newPage();
    await openApps(page);
    const visibleCompanies = (await productTiles(page)).map(tile => tile.companyId);
    expect(visibleCompanies).not.toContain(fixture.state.fixture.hospitality.companyId);

    const denied = new URL("/launch", "https://work-dev-erpx.jesadakorn.com");
    denied.searchParams.set("companyId", fixture.state.fixture.hospitality.companyId);
    denied.searchParams.set("next", "/holdings/simulation/home");
    await page.goto(denied.href);
    await expect(page).toHaveURL(/\/choose-work-role\?/);
    await expect(page.getByRole("heading", { name: /Choose a work role|เลือกบทบาททำงาน/ })).toBeVisible();
    await expect(page.getByText(/No current role and package can open this page|ไม่มีบทบาทและแพ็กเกจที่พร้อมใช้สำหรับหน้านี้/)).toBeVisible();
    await expect(page.locator(`[data-work-company="${fixture.state.fixture.hospitality.companyId}"]`)).toHaveCount(0);
  } finally {
    await signOutAndClose(context);
  }
});

test("Account profile language and theme preferences persist after navigation and reload", async ({ browser }, testInfo) => {
  if (!process.env.ERPX_PRODUCT_HOST_LEASE || !process.env.ERPX_DEMO_RUN)
    throw new Error("ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED");
  const fixture = await loadProductHostFixture();
  const user = fixture.credentials.users.find(candidate => candidate.email === "source@simulation.test");
  expect(user, "The isolated fixture includes its original Company account").toBeTruthy();
  const context = await signedInAccountContext(browser, testInfo, fixture.credentials, user!);
  const page = await context.newPage();
  let original: { language: string; theme: string } | undefined;
  let saveAttempted = false;
  try {
    await page.goto(`${accountOrigin}/my-profile`);
    await expect(page.getByRole("heading", { name: /Your account|บัญชีของคุณ/ })).toBeVisible();
    const fields = page.locator("form select");
    await expect(fields).toHaveCount(2);
    original = { language: await fields.nth(0).inputValue(), theme: await fields.nth(1).inputValue() };
    const language = original.language === "th" ? "en" : "th";
    const theme = original.theme === "dark" ? "light" : "dark";

    await fields.nth(0).selectOption(language);
    await fields.nth(1).selectOption(theme);
    saveAttempted = true;
    await page.getByRole("button", { name: /Save account settings|บันทึกการตั้งค่า/ }).click();
    await expect(page.getByText(/Account settings saved\.|บันทึกการตั้งค่าบัญชีแล้ว/)).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("lang", language);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);

    await page.getByRole("link", { name: /My access|สิทธิ์ของฉัน/ }).click();
    await expect(page).toHaveURL(/\/my-access$/);
    await page.getByRole("link", { name: /My profile|โปรไฟล์ของฉัน/ }).click();
    await page.reload();
    await expect(page.getByRole("heading", { name: /Your account|บัญชีของคุณ/ })).toBeVisible();
    await expect(page.locator("form select").nth(0)).toHaveValue(language);
    await expect(page.locator("form select").nth(1)).toHaveValue(theme);
    await expect(page.locator("html")).toHaveAttribute("lang", language);
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  } finally {
    try {
      if (saveAttempted && original) {
        await page.goto(`${accountOrigin}/my-profile`);
        const fields = page.locator("form select");
        await expect(fields).toHaveCount(2);
        const current = { language: await fields.nth(0).inputValue(), theme: await fields.nth(1).inputValue() };
        if (current.language !== original.language || current.theme !== original.theme) {
          await fields.nth(0).selectOption(original.language);
          await fields.nth(1).selectOption(original.theme);
          await page.getByRole("button", { name: /Save account settings|บันทึกการตั้งค่า/ }).click();
          await expect(page.locator("form select").nth(0)).toHaveValue(original.language);
          await expect(page.locator("form select").nth(1)).toHaveValue(original.theme);
        }
      }
    } finally {
      await signOutAndClose(context);
    }
  }
});
