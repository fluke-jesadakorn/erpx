import {expect,test,type Browser,type BrowserContext,type Page,type TestInfo} from '@playwright/test';
import {readFile,writeFile} from 'node:fs/promises';
import {relative,resolve} from 'node:path';
import {chooseApplicationsWorkspace,prepareProductPurchases,verifyProductHost} from '../../support/browser/product-host-session';
import {signIn} from '../../support/helpers/context-switching-browser';
import {companyAdminOwnerReadsProductHostCase,financeSalesInvoiceProductHostCase,productHostReadOnlyWorkflowForCase,readProductHostLease,workRequestDraftProductHostCase} from '../../../scripts/testing/product-host-cohort';
import {createHash,randomUUID} from 'node:crypto';
import {productIds} from '@erpx/contracts/product-registry';

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

test(financeSalesInvoiceProductHostCase,async({browser},testInfo)=>{
 test.setTimeout(300_000);if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'finance',async({page,origin,companyId,userId,scopedBusinessWorkflow})=>{
  const scope='finance.sales-invoice',fixture=await loadProductHostFixture(),run=fixture.state.run,customerId=fixture.state.fixture.customerId,lease=await readProductHostLease(process.cwd(),process.env.ERPX_PRODUCT_HOST_LEASE!),stateBytes=await readFile(resolve(fixture.directory,'state.json'));
  const stateSha256=createHash('sha256').update(stateBytes).digest('hex');expect(lease.ui?.backend.state.sha256).toBe(stateSha256);expect(lease.ui?.backend.state.bytes).toBe(stateBytes.length);
  expect(lease.run).toBe(run);expect(lease.scopedBusinessWorkflow).toBe(scope);expect(lease.readOnlyBusiness).toBe(false);expect(lease.isolatedNormalCommercial).toBe(true);
  expect(origin).toBe('https://finance-dev-erpx.jesadakorn.com');expect(scopedBusinessWorkflow).toBe(scope);expect(customerId).toBeTruthy();
  const invoicePath='/api/v1/holdings/simulation/finance/sales/invoices',invoiceUrl=origin+invoicePath,description=`Fictional Finance Product Host completed service ${run}`,completionNote=`Fictional Finance Product Host completion evidence ${run}`;
  const routedRequests:{idempotencyKey:string;body:string;contentType:string}[]=[];
  const markInvoiceWorkflow=async(route:import('@playwright/test').Route)=>{
   const request=route.request();if(request.method()!=='POST')return route.continue();
   const headers=request.headers(),body=request.postData()??'',idempotencyKey=headers['idempotency-key']??'';routedRequests.push({idempotencyKey,body,contentType:headers['content-type']??''});
   return route.continue({headers:{...request.headers(),'x-erpx-isolated-product-workflow':scope}});
  };
  await page.route(invoiceUrl,markInvoiceWorkflow);
  const owner=page.locator('[data-work-revision]').first();await expect(owner).toHaveAttribute('data-work-company',companyId);await expect(owner).toHaveAttribute('data-work-user',userId);await expect(owner).toHaveAttribute('data-work-stale','false');
  await page.goto(origin+'/holdings/simulation/finance/documents/sales/new?kind=invoice',{waitUntil:'domcontentloaded'});
  await expect(page.getByRole('heading',{name:'Invoice completed services',exact:true})).toBeVisible();await expect(page.getByRole('dialog')).toHaveCount(0);
  const form=page.getByRole('main');await form.getByRole('combobox',{name:'Customer',exact:true}).selectOption(customerId);
  await form.getByRole('textbox',{name:'Description',exact:true}).fill(description);await form.getByLabel('Quantity',{exact:true}).fill('2');await form.getByLabel('Unit price',{exact:true}).fill('100');
  const invoiceDate=await form.getByLabel('Invoice date',{exact:true}).inputValue(),dueDate=await form.getByLabel('Due date',{exact:true}).inputValue();expect(dueDate).toBe(invoiceDate);
  await form.getByRole('textbox',{name:'Completion evidence',exact:true}).fill(completionNote);await form.getByRole('checkbox').check();await expect(page.getByText('Total THB 214.00',{exact:true})).toBeVisible();
  const responsePromise=page.waitForResponse(response=>response.url()===invoiceUrl&&response.request().method()==='POST');await form.getByRole('button',{name:'Prepare invoice',exact:true}).click();const response=await responsePromise;expect(response.status()).toBe(200);
  const result=(await response.json()).data;expect(result.number).toMatch(/^INV-\d{4}-\d{6,}$/);expect(result.total).toBe('214.00');expect(result.completionEvidence.note).toBe(completionNote);
  expect(routedRequests).toHaveLength(1);const submitted=JSON.parse(routedRequests[0]!.body);expect(routedRequests[0]!.idempotencyKey).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  expect(Object.keys(submitted).sort()).toEqual(['completionConfirmed','completionNote','currency','dueDate','invoiceDate','lines','partyId'].sort());expect(submitted).toMatchObject({partyId:customerId,invoiceDate,dueDate,currency:'THB',completionConfirmed:true,completionNote});expect(submitted.lines).toEqual([{description,quantity:'2',unitPrice:'100',taxBps:700}]);
  await expect(page.getByTestId('record-detail-drawer').locator('h1')).toHaveText(result.number);await expect(owner).toHaveAttribute('data-work-company',companyId);await page.locator('summary').filter({hasText:'Service completion evidence'}).click();await expect(page.getByText(completionNote,{exact:true})).toBeVisible();
  const screenshotPath=testInfo.outputPath('finance-product-host-sales-invoice-screen.png'),screenshotBytes=await page.screenshot({path:screenshotPath,fullPage:true}),screenshotAttachmentIndex=testInfo.attachments.length;
  await testInfo.attach('finance-product-host-sales-invoice-screen',{path:screenshotPath,contentType:'image/png'});
  const screenshotAttachment=testInfo.attachments[screenshotAttachmentIndex];if(!screenshotAttachment?.path)throw Error('FINANCE_PRODUCT_HOST_SCREENSHOT_ATTACHMENT_REQUIRED');
  await testInfo.attach('finance-product-host-simulation-state',{body:stateBytes,contentType:'application/json'});
  expect(routedRequests[0]!.contentType).toBe('application/json');
  await testInfo.attach('finance-product-host-sales-invoice',{body:Buffer.from(JSON.stringify({schema:'erpx-product-host-finance-sales-invoice/v1',workflow:scope,run,sourceFingerprint:lease.sourceFingerprint,verificationFingerprint:lease.verificationFingerprint,release:lease.release,project:testInfo.project.name,origin,companyId,userId,fixturePartyId:customerId,invoiceId:result.id,invoiceNumber:result.number,currency:'THB',total:result.total,idempotencyKey:routedRequests[0]!.idempotencyKey,contentType:routedRequests[0]!.contentType,requestBody:routedRequests[0]!.body,requestSha256:createHash('sha256').update(routedRequests[0]!.body).digest('hex'),completionEvidence:result.completionEvidence,disposableCohortCleanup:true,wholeProductAcceptance:false})),contentType:'application/json'});
  const ownerContext={companyId:await owner.getAttribute('data-work-company'),userId:await owner.getAttribute('data-work-user'),assignmentId:await owner.getAttribute('data-work-assignment'),revision:await owner.getAttribute('data-work-revision')};expect(ownerContext).toEqual({companyId,userId,assignmentId:expect.any(String),revision:expect.any(String)});
  const observationPath=testInfo.outputPath('finance-product-host-sales-invoice-observation.json');await writeFile(observationPath,JSON.stringify({schema:'erpx-ui-browser-observation/v1',result:'passed',run,sourceFingerprint:lease.sourceFingerprint,verificationFingerprint:lease.verificationFingerprint,url:page.url(),ownerContext,screenshot:{path:relative(process.cwd(),screenshotAttachment.path).replaceAll('\\','/'),sha256:createHash('sha256').update(screenshotBytes).digest('hex'),bytes:screenshotBytes.length}},null,2)+'\n',{flag:'wx',mode:0o600});
  await testInfo.attach('erpx-ui-observation',{path:observationPath,contentType:'application/json'});
 });
});

test(workRequestDraftProductHostCase,async({browser},testInfo)=>{
 test.setTimeout(240_000);if(!process.env.ERPX_PRODUCT_HOST_LEASE)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 await verifyProductHost(browser,testInfo,'apps',async({page,context,origin,companyId,scopedBusinessWorkflow})=>{
  expect(origin).toBe('https://apps-dev-erpx.jesadakorn.com');expect(scopedBusinessWorkflow).toBe('work.request-draft');
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

test(companyAdminOwnerReadsProductHostCase,async({browser},testInfo)=>{
 if(!process.env.ERPX_PRODUCT_HOST_LEASE||!process.env.ERPX_DEMO_RUN)throw Error('ARMED_OFFICIAL_PRODUCT_HOST_MODE_REQUIRED');
 const fixture=await loadProductHostFixture(),user=fixture.credentials.users.find(candidate=>candidate.email==='source@simulation.test'),lease=await readProductHostLease(root,process.env.ERPX_PRODUCT_HOST_LEASE);
 expect(user).toBeTruthy();expect(user!.role).toBe('holding_owner');
 expect(productHostReadOnlyWorkflowForCase('tests/e2e/identity_access/product-hosts.spec.ts',companyAdminOwnerReadsProductHostCase)).toBe('admin.company-brand-organization-iam');
 expect(lease.readOnlyWorkflow).toBe('admin.company-brand-organization-iam');expect(lease.scopedBusinessWorkflow).toBeNull();expect(lease.readOnlyBusiness).toBe(true);expect(lease.isolatedNormalCommercial).toBe(false);
 const companyId=fixture.state.fixture.companyId,context=await signedInAccountContext(browser,testInfo,fixture.credentials,user!);
 const forbiddenWrites:string[]=[];context.on('request',request=>{
  const method=request.method().toUpperCase();if(method==='GET'||method==='HEAD')return;
  const url=new URL(request.url()),authPost=url.origin===accountOrigin&&method==='POST'&&['/api/auth/sign-in/email','/api/auth/sign-out'].includes(url.pathname);
  if(!authPost)forbiddenWrites.push(`${method} ${url.origin}${url.pathname}`);
 });
 try{
  const page=await context.newPage(),observedPages:{owner:'brand'|'organization'|'iam';path:string;observation:string;screenshot:string}[]=[],paths={
   brand:'/holdings/simulation/administration/brand/identity',
   organization:'/holdings/simulation/administration/technology/organization',
   iam:'/holdings/simulation/administration/technology/settings/roles',
  };
  await page.goto(`${accountOrigin}/my-access`,{waitUntil:'domcontentloaded'});
  await expect(page.getByRole('heading',{name:'My access',exact:true})).toBeVisible();
  await expect(page.getByRole('heading',{name:'Roles & permissions',exact:true})).toBeVisible();
  await expect(page.getByRole('heading',{name:user!.role,exact:true})).toBeVisible();
  await expect(page.getByRole('link',{name:'Open Apps',exact:true})).toBeVisible();
  await attachProductHostUiObservation(page,testInfo,lease,'account-my-access','The signed-in Account owner’s current roles and workspace entry',[
   'My access displays the authenticated Account owner’s current role.',
   'The Account page exposes Apps without changing role, package or grant authority.',
  ]);
  await page.getByRole('link',{name:'Open Apps',exact:true}).click();
  await expect(page).toHaveURL(new RegExp(`^${appsOrigin.replaceAll('.','\\.')}/`));
  await chooseApplicationsWorkspace(page,companyId,'/holdings/simulation');
  const adminOrigin='https://admin-dev-erpx.jesadakorn.com',companyTarget='/holdings/simulation/administration',tiles=[] as {href:string;target:string}[];
  for(const anchor of await page.locator('a[data-product-switch]').all()){
   const rawHref=await anchor.getAttribute('href');if(!rawHref)continue;const href=new URL(rawHref,page.url());
   if(href.origin!==adminOrigin||href.pathname!=='/sso/start')continue;const next=href.searchParams.get('next');if(!next)continue;
   const target=new URL(next,adminOrigin);if(target.pathname==='/admin/launch'&&target.searchParams.get('companyId')===companyId&&target.searchParams.get('next')===companyTarget)tiles.push({href:href.href,target:target.href});
  }
  expect(tiles).toHaveLength(1);
  await attachProductHostUiObservation(page,testInfo,lease,'account-authorized-admin-app','The existing Company Admin product available to the signed-in Account owner',[
   'Apps exposes the Company Admin destination under the same Company as the owner’s current assignment.',
   'The Account and Apps UI reads current authority without a Commercial grant or business write.',
  ]);
  await selectVisibleTile(page,tiles[0]!.href);
  await page.waitForURL(url=>url.origin===adminOrigin&&(url.pathname===companyTarget||url.pathname==='/admin/choose-work-role'));
  expect(new URL(page.url()).pathname,'Existing Company authority must land without choosing a role').toBe(companyTarget);
  let assignmentId:string|undefined;
  const assertCompanyAuthority=async()=>{
   const owner=page.locator('[data-work-company]').first();await expect(owner).toHaveAttribute('data-work-company',companyId);await expect(owner).toHaveAttribute('data-work-user',user!.userId);await expect(owner).toHaveAttribute('data-work-stale','false');
   const current=await owner.getAttribute('data-work-assignment');expect(current).toBeTruthy();if(assignmentId)expect(current).toBe(assignmentId);else assignmentId=current??undefined;return owner;
  };
  const capture=async(name:string)=>{const imagePath=testInfo.outputPath(`${name}.png`);await page.screenshot({path:imagePath,fullPage:true});await testInfo.attach(name,{path:imagePath,contentType:'image/png'});};
  await page.goto(adminOrigin+paths.brand,{waitUntil:'domcontentloaded'});await expect(page).toHaveURL(adminOrigin+paths.brand);await assertCompanyAuthority();
  await expect(page.getByRole('heading',{name:'Published identity'})).toBeVisible();
  const brandSurface=page.locator('[data-owner-embedded]').first(),brandName=await brandSurface.locator('h3').first().textContent(),brandText=(brandName??(await brandSurface.innerText())).trim();
  expect(brandText).toBeTruthy();await capture('company-admin-brand-identity');observedPages.push({owner:'brand',path:paths.brand,observation:brandText,screenshot:'company-admin-brand-identity'});
  await page.goto(adminOrigin+paths.organization,{waitUntil:'domcontentloaded'});await expect(page.getByRole('heading',{name:'Organization & Access'})).toBeVisible();await assertCompanyAuthority();
  await expect(page.getByTestId('organization-structure-surface')).toBeVisible();const companyNode=page.getByTestId(`org-company-${companyId}`);await expect(companyNode).toBeVisible();const organizationText=(await companyNode.innerText()).trim();expect(organizationText).toBeTruthy();
  await capture('company-admin-organization');observedPages.push({owner:'organization',path:paths.organization,observation:organizationText,screenshot:'company-admin-organization'});
  await page.goto(adminOrigin+paths.iam,{waitUntil:'domcontentloaded'});await expect(page.getByRole('heading',{name:'Access settings'})).toBeVisible();await assertCompanyAuthority();
  const roles=page.getByRole('region',{name:'Roles & permissions'});await expect(roles).toBeVisible();const roleRows=roles.getByRole('button');await expect(roleRows.first()).toBeVisible();const roleText=(await roleRows.first().innerText()).trim();
  const iamResponse=await context.request.get(`${adminOrigin}/api/v1/holdings/simulation/iam`);let iam:Record<string,unknown>;
  try{expect(iamResponse.status()).toBe(200);const payload=await iamResponse.json();iam=payload.data as Record<string,unknown>;}finally{await iamResponse.dispose();}
  expect(iam).toMatchObject({companyId,currentUserId:user!.userId,currentAssignmentId:assignmentId,scopeKind:'holding'});
  expect(iam.assignments).toContainEqual(expect.objectContaining({id:assignmentId,user_id:user!.userId,role:'holding_owner'}));
  await capture('company-admin-iam-roles');observedPages.push({owner:'iam',path:paths.iam,observation:`${roleText}; current assignment ${String(iam.currentAssignmentId)}`,screenshot:'company-admin-iam-roles'});
  expect(forbiddenWrites).toEqual([]);
  await testInfo.attach('company-admin-owner-reads',{body:Buffer.from(JSON.stringify({
   schema:'erpx-product-host-company-admin-owner-reads/v1',result:'passed',workflow:'admin.company-brand-organization-iam',run:lease.run,
   sourceFingerprint:lease.sourceFingerprint,verificationFingerprint:lease.verificationFingerprint,release:lease.release,project:testInfo.project.name,
   authority:{companyId,userId:user!.userId,assignmentId,role:'holding_owner',sameOwnerAssignmentAcrossPages:true,ownerAssignmentUnchanged:true},pages:observedPages,blockedWrites:forbiddenWrites,
  },null,2)+'\n'),contentType:'application/json'});
 }finally{await signOutAndClose(context);}
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
    customerId: string;
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
  const links = await page.locator("a[data-product-switch]").all();
  const result: { href: string; companyId: string; target: string; origin: string }[] = [];
  for (const link of links) {
    const rawHref = await link.getAttribute("href");
    if (!rawHref) continue;
    const parsed = productTargetFromAppsHref(new URL(rawHref, page.url()).href);
    if (parsed) result.push({ href: new URL(rawHref, page.url()).href, ...parsed });
  }
  return result;
}

async function openApps(page: Page, companyId: string, scope: "branch" | "department" = "branch") {
  await page.goto(`${accountOrigin}/my-profile`);
  await expect(page.getByRole("heading", { name: /Your account|บัญชีของคุณ/ })).toBeVisible();
  await page.getByRole("link", { name: "Apps", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`^${appsOrigin.replaceAll(".", "\\.")}/`));
  await chooseApplicationsWorkspace(page, companyId, base => scope === "branch" ? base.includes("/branches/") && !base.includes("/departments/") : base.includes("/departments/"));
}

async function selectVisibleTile(page: Page, href: string) {
  const links = await page.locator("a[data-product-switch]").all();
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

async function attachProductHostUiObservation(
  page: Page,
  testInfo: TestInfo,
  lease: Awaited<ReturnType<typeof readProductHostLease>>,
  label: string,
  subject: string,
  checks: string[],
) {
  if (process.env.ERPX_DEMO_RUN !== lease.run || !checks.length) throw new Error("PRODUCT_HOST_UI_OBSERVATION_RUN_REQUIRED");
  const screenshotPath = testInfo.outputPath(`${label}.png`);
  const screenshotBytes = await page.screenshot({ path: screenshotPath, fullPage: true, animations: "disabled" });
  const screenshotIndex = testInfo.attachments.length;
  await testInfo.attach("product-host-ui-screenshot", { path: screenshotPath, contentType: "image/png" });
  const screenshotAttachment = testInfo.attachments[screenshotIndex];
  if (!screenshotAttachment?.path) throw new Error("PRODUCT_HOST_UI_SCREENSHOT_ATTACHMENT_REQUIRED");
  const screenshot = {
    path: relative(process.cwd(), screenshotAttachment.path).replaceAll("\\", "/"),
    sha256: createHash("sha256").update(screenshotBytes).digest("hex"),
    bytes: screenshotBytes.length,
  };
  const observationPath = testInfo.outputPath(`${label}-observation.json`);
  await writeFile(observationPath, JSON.stringify({
    schema: "erpx-ui-browser-observation/v1",
    result: "passed",
    run: lease.run,
    sourceFingerprint: lease.sourceFingerprint,
    verificationFingerprint: lease.verificationFingerprint,
    releaseId: lease.release.id,
    releaseManifestSha256: lease.release.fileManifestSha256,
    url: page.url(),
    subject,
    audience: "signed-in Account owner",
    authority: "current Account self-access and existing scoped workspace assignment",
    checks,
    screenshot,
  }, null, 2) + "\n", { flag: "wx", mode: 0o600 });
  await testInfo.attach("erpx-ui-observation", { path: observationPath, contentType: "application/json" });
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
      await openApps(page, fixture.state.fixture.companyId, scope);
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
    await openApps(page, fixture.state.fixture.companyId);
    const visibleCompanies = (await productTiles(page)).map(tile => tile.companyId);
    expect(visibleCompanies).not.toContain(fixture.state.fixture.hospitality.companyId);

    const denied = new URL("/launch", "https://apps-dev-erpx.jesadakorn.com");
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
