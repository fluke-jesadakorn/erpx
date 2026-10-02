import { test, expect, type BrowserContext, type Page, type APIResponse } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import type { LegalUatUser } from '../../scripts/fixtures/legal-browser-demo';

type Credentials={run:string;password:string;users:LegalUatUser[]};
type Guide={companies:Record<string,{id:string;holdingSlug:string;branchIds:string[];departmentIds:string[]}>;records:Record<string,Record<string,string>>;signedPdfPath:string};
const run=process.env.ERPX_DEMO_RUN;
const origin=process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104';
const key=()=>crypto.randomUUID();
test.describe.configure({mode:'serial'});
test.beforeEach(async()=>{test.skip(!run,'Set ERPX_DEMO_RUN to an exact Legal browser-simulation run.');});
async function fixture(){
  if(!run||!/^[a-f0-9]{12}$/.test(run)||origin!=='http://127.0.0.1:3104')throw new Error('EXACT_ISOLATED_LEGAL_UAT_REQUIRED');
  const directory=resolve('.runtime/browser',run);
  const state=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8'));
  if(state.run!==run||state.origin!==origin||state.prefix!==`erpx_browser_${run}`||!state.fixture?.legal)throw new Error('LEGAL_UAT_STATE_MISMATCH');
  const credentials=JSON.parse(await readFile(resolve(directory,'legal-credentials.json'),'utf8')) as Credentials;
  const guide=JSON.parse(await readFile(resolve(directory,'legal-guide.json'),'utf8')) as Guide;
  return {credentials,guide};
}
function base(user:LegalUatUser){return `/holdings/${user.holdingSlug}${user.scope==='branch'?`/branches/${user.scopeNodeId}`:''}`;}
async function checked(response:APIResponse){expect(response.status(),await response.text()).toBe(200);return (await response.json()).data;}
async function signIn(context:BrowserContext,user:LegalUatUser,password:string){
  await checked(await context.request.post('/api/auth/sign-in/email',{headers:{Origin:origin},data:{email:user.email,password}}));
  const endpoint=`/api/v1/holdings/${user.holdingSlug}/assignments/active`,active=await checked(await context.request.get(endpoint));
  expect(active.options.some((option:{id:string})=>option.id===user.assignmentId)).toBe(true);
  await checked(await context.request.post(endpoint,{headers:{Origin:origin},data:{assignmentId:user.assignmentId,revision:active.revision}}));
}
async function apiCommand(context:BrowserContext,user:LegalUatUser,body:Record<string,unknown>){return checked(await context.request.post(`/api/v1${base(user)}/legal`,{headers:{Origin:origin,'Idempotency-Key':key()},data:body}));}
async function open(page:Page,user:LegalUatUser,id?:string){await page.goto(`${base(user)}/legal${id?`?record=${id}`:''}`);await expect(page.getByRole('heading',{name:'Legal',exact:true})).toBeVisible();}
async function submitUi(page:Page,name:string){
  const result=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname.endsWith('/legal'));
  await page.getByRole('button',{name,exact:true}).click();await checked(await result);
}

test('UAT-02/03/09/23: request from Today restores a Thai draft, routes to Legal and uses current scope',async({browser},testInfo)=>{
  test.setTimeout(180_000);const {credentials,guide}=await fixture();
  const requester=credentials.users.find(user=>user.key==='A:requester')!,manager=credentials.users.find(user=>user.key==='A:manager')!;
  const context=await browser.newContext(testInfo.project.use),team=await browser.newContext(testInfo.project.use);
  try{
    await signIn(context,requester,credentials.password);await signIn(team,manager,credentials.password);
    const page=await context.newPage();await page.goto(`${base(requester)}/home`);
    await page.getByRole('link',{name:'Request legal advice',exact:true}).click();
    await expect(page.getByRole('heading',{name:'New request',exact:true})).toBeVisible();
    const title=`Thai Legal UAT ${testInfo.project.name} ${key()}`;
    await page.getByLabel('Title',{exact:true}).fill(title);
    await page.getByLabel('Request type',{exact:true}).fill('Synthetic contract question');
    await page.getByLabel('Question / advice requested',{exact:true}).fill('กรุณาตรวจข้อกำหนดสัญญาสมมติ ข้อมูลสำหรับการทดสอบเท่านั้น');
    const ownOptions=await page.getByLabel('Owner',{exact:true}).locator('option').allTextContents();
    expect(ownOptions.some(value=>value.includes('other branch'))).toBe(false);
    await page.reload();await expect(page.getByLabel('Title',{exact:true})).toHaveValue(title);
    await expect(page.getByText('Your unsent details were restored in this tab. Review them before saving.',{exact:true})).toBeVisible();
    const saved=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname.endsWith('/legal'));
    await page.getByRole('button',{name:'Save record',exact:true}).click();const created=await checked(await saved);
    await expect(page.getByRole('heading',{name:title,exact:true})).toBeVisible();
    await page.getByRole('button',{name:'Workflow',exact:true}).click();
    await page.getByLabel('Next state',{exact:true}).selectOption('review');
    const transition=page.getByRole('button',{name:'Change state',exact:true}).locator('..').locator('..');
    await transition.getByLabel('Reason',{exact:true}).fill('Submit a synthetic question for legal triage');
    await submitUi(page,'Change state');
    const managerPage=await team.newPage();await open(managerPage,manager,created.id);
    await expect(managerPage.getByRole('heading',{name:title,exact:true})).toBeVisible();
    const live=await checked(await team.request.get(`/api/v1${base(manager)}/legal/${created.id}`));
    expect(live.item.state).toBe('review');expect(live.item.branch_id).toBe(guide.companies.A.branchIds[0]);
    const forbiddenOwner=credentials.users.find(user=>user.key==='A:other-branch')!;
    const denied=await team.request.post(`/api/v1${base(manager)}/legal`,{headers:{Origin:origin,'Idempotency-Key':key()},data:{action:'member',id:created.id,expectedVersion:live.item.version,userId:forbiddenOwner.userId,access:'edit'}});
    expect(denied.status()).toBe(422);
    await page.screenshot({path:testInfo.outputPath('thai-request-and-restored-draft.png'),fullPage:true});
  }finally{await context.close();await team.close();}
});

test('UAT-04/05/06/07: ordered independent approval and signed-copy review preserve distinct originals',async({browser},testInfo)=>{
  test.setTimeout(240_000);const {credentials,guide}=await fixture();
  const manager=credentials.users.find(user=>user.key==='A:manager')!,first=credentials.users.find(user=>user.key==='A:reviewer-1')!,second=credentials.users.find(user=>user.key==='A:reviewer-2')!;
  const contexts=await Promise.all([manager,first,second].map(()=>browser.newContext(testInfo.project.use)));
  const [owner,reviewer1,reviewer2]=contexts;
  try{
    await Promise.all([manager,first,second].map((user,index)=>signIn(contexts[index],user,credentials.password)));
    const title=`Reviewed UAT contract ${testInfo.project.name} ${key()}`;
    const created=await apiCommand(owner,manager,{action:'create',item:{kind:'contract',title,ownerId:manager.userId,data:{contractType:'Synthetic supply',parties:'Company A and Synthetic Supplier'}}});
    const id=created.id;
    const record=async(context=owner,user=manager)=>checked(await context.request.get(`/api/v1${base(user)}/legal/${id}`));
    let current=await record();await apiCommand(owner,manager,{action:'document',id,expectedVersion:current.item.version,name:'Approval draft',body:'Synthetic agreement: deliver ten training items within thirty days.'});
    current=await record();const draft=current.documents[0].id;
    await apiCommand(owner,manager,{action:'document',id,expectedVersion:current.item.version,name:'Signatory authority',body:'Synthetic training signer authority. UAT only.'});
    current=await record();const authority=current.documents.find((document:{name:string})=>document.name==='Signatory authority').id;
    for(const user of [first,second]){current=await record();await apiCommand(owner,manager,{action:'member',id,expectedVersion:current.item.version,userId:user.userId,access:'read'});}
    const page=await owner.newPage();await open(page,manager,id);await page.getByRole('button',{name:'Workflow',exact:true}).click();
    const route=page.getByRole('heading',{name:'Contract approval route'}).locator('..');
    await route.getByLabel('Document version',{exact:true}).selectOption(draft);
    await route.getByLabel('Reviewer 1',{exact:true}).selectOption(first.userId);
    await route.getByRole('button',{name:'Add reviewer',exact:true}).click();
    await route.getByLabel('Reviewer 2',{exact:true}).selectOption(second.userId);
    await route.getByLabel('Expected signer 1',{exact:true}).selectOption(manager.userId);
    await route.getByLabel('Reason',{exact:true}).fill('Independent ordered review of a fictional contract');
    await submitUi(page,'Open approval route');
    current=await record();const outOfOrder=await reviewer2.request.post(`/api/v1${base(second)}/legal`,{headers:{Origin:origin,'Idempotency-Key':key()},data:{action:'approval.decide',id,expectedVersion:current.item.version,decision:'approve',reason:'Cannot skip the independent first reviewer'}});
    expect(outOfOrder.status()).toBeGreaterThanOrEqual(400);
    for(const [context,user] of [[reviewer1,first],[reviewer2,second]] as const){
      const reviewPage=await context.newPage();await open(reviewPage,user,id);await reviewPage.getByRole('button',{name:'Workflow',exact:true}).click();
      const decision=reviewPage.getByRole('button',{name:'Record my decision',exact:true}).locator('..');
      await decision.getByLabel('Reason',{exact:true}).fill('Independent review of exact fictional locked version');
      await submitUi(reviewPage,'Record my decision');
    }
    await page.reload();await page.getByRole('button',{name:'Documents',exact:true}).click();
    await page.getByText('Import a signed original or corrected revision',{exact:true}).click();
    const copies=page.getByRole('region',{name:'Executed copies'});
    await copies.getByLabel('Approved document',{exact:true}).selectOption(draft);
    await copies.getByLabel('Signing date (Gregorian)',{exact:true}).fill('2026-09-30');
    await copies.getByLabel('Parties, one per line',{exact:true}).fill('Company A\nSynthetic Supplier');
    await copies.getByLabel('Number of pages, including annexes',{exact:true}).fill('1');
    await copies.getByLabel('Signer name',{exact:true}).fill('Training signer');
    await copies.getByLabel('Party represented',{exact:true}).fill('Company A');
    await copies.getByLabel('Authority evidence for signer 1',{exact:true}).selectOption(authority);
    await copies.getByLabel('Signed PDF or scanned image',{exact:true}).setInputFiles(guide.signedPdfPath);
    const imported=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname.endsWith(`/items/${id}/executed-copies`));
    await copies.getByRole('button',{name:'Import for independent review',exact:true}).click();await checked(await imported);
    await expect(copies.getByText(/pending review/)).toBeVisible();
    const reviewPage=await reviewer2.newPage();await open(reviewPage,second,id);await reviewPage.getByRole('button',{name:'Documents',exact:true}).click();
    const independent=reviewPage.getByRole('region',{name:'Executed copies'});
    await independent.getByLabel('Review decision',{exact:true}).selectOption('confirm');
    await independent.getByLabel('Review reason and evidence',{exact:true}).fill('Missing pages cannot be certified in the fictional rehearsal');
    const incomplete=reviewPage.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname.endsWith('/legal'));
    await independent.getByRole('button',{name:'Record independent copy review',exact:true}).click();expect((await incomplete).status()).toBe(409);
    for(const label of ['Content matches the approved version','All pages and annexes are present','All parties and their details are correct','Every required signature is present','Signatory authority evidence is complete and valid'])await independent.getByLabel(label,{exact:true}).check();
    await independent.getByLabel('Review reason and evidence',{exact:true}).fill('Synthetic copy, pages, parties and authority independently checked');
    await submitUi(reviewPage,'Record independent copy review');
    await expect(independent.getByText(/· confirmed/)).toBeVisible();
    const final=await record();expect(final.item.state).toBe('executed');
    const originals=final.documents.filter((document:{id:string;locked:boolean})=>document.id===draft||document.name==='signed-original.pdf');
    expect(originals).toHaveLength(2);expect(originals[0].checksum).not.toBe(originals[1].checksum);
    await reviewPage.screenshot({path:testInfo.outputPath('independent-executed-copy.png'),fullPage:true});
  }finally{await Promise.all(contexts.map(context=>context.close()));}
});

test('UAT-08/09/20/21/25: Companies and Holdings stay scoped, pending law stays out of search and readiness stays pending',async({browser},testInfo)=>{
  test.setTimeout(180_000);const {credentials,guide}=await fixture();
  const manager=credentials.users.find(user=>user.key==='A:manager')!,other=credentials.users.find(user=>user.key==='C:manager')!,owner=credentials.users.find(user=>user.key==='A:owner')!;
  const contexts=await Promise.all([manager,other,owner].map(()=>browser.newContext(testInfo.project.use)));
  try{
    await Promise.all([manager,other,owner].map((user,index)=>signIn(contexts[index],user,credentials.password)));
    const [context,crossHolding,ownerContext]=contexts;
    expect((await ownerContext.request.get(`/api/v1${base(owner)}/legal/${guide.records.A.contract}`)).status()).toBe(404);
    expect((await context.request.get(`/api/v1${base(manager)}/legal/${guide.records.B.contract}`)).status()).toBe(404);
    const holdingDenied=await crossHolding.request.get(`/api/v1${base(manager)}/legal/${guide.records.A.contract}`);
    expect(holdingDenied.status()).toBeGreaterThanOrEqual(400);
    const page=await context.newPage();await open(page,manager);
    await page.getByRole('button',{name:'Official law sources',exact:true}).click();
    await expect(page.getByText('SYNTHETIC-PENDING',{exact:false})).toBeVisible();
    const knowledge=await checked(await context.request.get(`/api/v1${base(manager)}/legal/knowledge?q=${encodeURIComponent('ข้อความสมมติ')}`));
    expect(knowledge.sources.filter((source:{kind:string})=>source.kind==='law')).toHaveLength(0);
    await page.getByRole('button',{name:'Company readiness',exact:true}).click();
    await expect(page.getByText('Company release pending',{exact:true})).toBeVisible();
    await expect(page.getByText(/Named legal specialist accepts rules/)).toBeVisible();
    const status=await checked(await context.request.get(`/api/v1${base(manager)}/legal/release-acceptance`));
    expect(status.state).toBe('pending');expect(status.gates.every((gate:{state:string})=>gate.state==='pending')).toBe(true);
    const stamp=await page.locator('[data-work-revision]').first().evaluate(node=>({revision:(node as HTMLElement).dataset.workRevision!,company:(node as HTMLElement).dataset.workCompany!,assignment:(node as HTMLElement).dataset.workAssignment!}));
    const endpoint=`/api/v1/holdings/${manager.holdingSlug}/assignments/active`,active=await checked(await context.request.get(endpoint));
    const companyB=active.options.find((option:{companyId:string;role:string})=>option.companyId===guide.companies.B.id&&option.role==='legal_manager');
    expect(companyB).toBeTruthy();await checked(await context.request.post(endpoint,{headers:{Origin:origin},data:{assignmentId:companyB.id,revision:active.revision}}));
    const stale=await context.request.post(`/api/v1${base(manager)}/legal`,{headers:{Origin:origin,'Idempotency-Key':key(),'x-erpx-context-revision':stamp.revision,'x-erpx-context-company':stamp.company,'x-erpx-context-assignment':stamp.assignment},data:{action:'comment',id:guide.records.A.contract,expectedVersion:1,body:'Stale Company tab'}});
    expect(stale.status()).toBe(409);
    expect((await context.request.get(`/api/v1${base(manager)}/legal/${guide.records.A.contract}`)).status()).toBe(404);
    await page.screenshot({path:testInfo.outputPath('company-readiness-pending.png'),fullPage:true});
  }finally{await Promise.all(contexts.map(context=>context.close()));}
});
