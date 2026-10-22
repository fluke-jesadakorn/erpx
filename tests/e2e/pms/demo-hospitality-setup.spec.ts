import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Locator, type Page, type Route } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { pinnedBunBinary } from "../../../scripts/testing/runtime-tooling";
import { browserChildEnvironment } from "../../../scripts/testing/runner-environment";
import { Pool } from "pg";
import { frontOrigin, frontWorkspacePath, signInFrontRole, type SimulatedFrontCredentials } from "../../support/helpers/front-simulation-session";
import {records, plusDays, type Snapshot} from "@erpx/server-pms/public/rules/pms-model";

test('keeps hotel drafts scoped while refresh fences stale cash and an inspected higher room type upgrades through the drawer',async({browser},info)=>{
  test.setTimeout(90_000);
  const run=process.env.ERPX_DEMO_RUN;
  expect(run,'Requires the exact disposable hospitality cohort.').toMatch(/^[a-f0-9]{12}$/);
  const directory=resolve(`.runtime/browser/${run}`),origin=process.env.ERPX_E2E_BASE_URL!;
  expect(origin).toBe('http://127.0.0.1:3104');
  const credentials=JSON.parse(await readFile(resolve(directory,'credentials.json'),'utf8')) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,'state.json'),'utf8')) as State;
  expect(state.release?.id,'Run this supplemental workflow against the compiled candidate.').toBeTruthy();
  const propertyA=state.fixture.hospitality.propertyId,companyId=state.fixture.hospitality.companyId;
  const contexts:BrowserContext[]=[],stamps=new Map<BrowserContext,Record<string,string>>();
  async function actor(role:string){
    const context=await createEngineeringBrowserContext(browser,info);contexts.push(context);
    await signIn(context,credentials,role);
    const page=await context.newPage();await page.goto('/holdings/riverside-lantern/home');
    const node=page.locator('[data-work-revision]').first();await expect(node).toBeVisible();
    expect(await node.getAttribute('data-work-company')).toBe(companyId);
    stamps.set(context,{'x-erpx-context-revision':(await node.getAttribute('data-work-revision'))!,'x-erpx-context-company':companyId,'x-erpx-context-assignment':(await node.getAttribute('data-work-assignment'))!});
    return context;
  }
  const api='/api/v1/holdings/riverside-lantern/pms';
  async function snapshot(context:BrowserContext,id:string){const response=await context.request.get(`${api}?property=${id}`);expect(response.ok(),await response.text()).toBe(true);return (await response.json()).data.snapshot as Snapshot;}
  async function post(context:BrowserContext,path:string,input:unknown){const response=await context.request.post(path,{headers:{Origin:origin,...stamps.get(context),'Idempotency-Key':crypto.randomUUID()},data:input});expect(response.ok(),await response.text()).toBe(true);return (await response.json()).data as {id:string;nodeId:string;version:number};}
  async function command(context:BrowserContext,id:string|null,operation:string,input:unknown,reader=context){const current=id?await snapshot(reader,id):null;return post(context,api,{operation,...(current?{propertyId:id,expectedVersion:current.property.version}:{}),input});}
  try{
    const owner=await actor('holding_owner'),finance=await actor('finance_approver'),hr=await actor('hr_admin');
    const original=await snapshot(owner,propertyA),config=original.property.config;
    const org='/api/v1/holdings/riverside-lantern/people/organization/changes';
    const proposal=await post(owner,`${org}/submit`,{action:'create',kind:'branch',slug:`mock-second-${run}`,displayName:'Fictional second hotel branch',expectedVersion:0,reason:'Mock two-property UI isolation fixture, independently reviewed.'});
    const approved=await post(hr,`${org}/${proposal.id}/review`,{expectedVersion:proposal.version??1,decision:'approve',reason:'Independent mock branch approval for hotel isolation.'});
    const propertyB=(await command(owner,null,'property.create',{name:'Fictional second hotel · draft isolation',branchId:approved.nodeId,businessDate:original.property.businessDate})).id;
    await command(owner,propertyB,'accounting.save',{chartId:config.chartId,accounts:config.accounts,taxBps:config.taxBps,serviceBps:config.serviceBps,taxInclusive:config.taxInclusive,advanceTax:config.advanceTax});
    await command(finance,propertyB,'accounting.approve',{},owner);
    const typeB=(await command(owner,propertyB,'room_type.save',{name:'Mock Standard B',capacity:2,amenities:'Shower'})).id;
    await command(owner,propertyB,'room.save',{number:'B101',typeId:typeB,active:true});
    await command(owner,propertyB,'rate.save',{name:'Mock Flexible B',typeId:typeB,from:original.property.businessDate,to:plusDays(original.property.businessDate,365),price:120000});
    await command(owner,propertyB,'property.enable',{});
    const front=await createEngineeringBrowserContext(browser,info);contexts.push(front);const page=await front.newPage();
    let releasePolling!:()=>void,heldPollRequests=0;
    let shift!:Locator,shiftIds!:string[];
    const pollingGate=new Promise<void>(resolve=>{releasePolling=resolve;});
    const pollingRoute=(url:URL)=>url.pathname===api;
    const holdPolling=async(route:Route)=>{if(route.request().method()==='GET'){heldPollRequests++;await pollingGate;}await route.continue();};
    await page.route(pollingRoute,holdPolling);
    try{
    await signInFrontRole(front,page,credentials as SimulatedFrontCredentials,'pms_frontdesk','pms',companyId,frontWorkspacePath(companyId,'cashier')+`?property=${propertyA}`);
    if(await page.getByRole('button',{name:'EN',exact:true}).count())await page.getByRole('button',{name:'EN',exact:true}).click();
    shift=page.locator('details').filter({has:page.locator('summary').filter({hasText:/^Open cash shift$/})});
    await expect(shift).toHaveCount(1);await shift.locator('summary').click();
    await expect(shift.getByRole('button',{name:'Open cash shift',exact:true})).toHaveAttribute('data-client-managed-ready','true');
    await shift.locator('input[name="name"]').fill('A private draft must stay with A');await shift.locator('input[name="opening"]').fill('321.09');
    const before=await snapshot(owner,propertyA);shiftIds=records(before,'shift').map(r=>r.id).sort();
    const refreshedName='Fictional refreshed hotel A · same Property';
    await command(owner,propertyA,'property.save',{name:refreshedName,country:config.country,timezone:config.timezone,checkIn:config.checkIn,checkOut:config.checkOut,cancellationPolicy:config.cancellationPolicy});
    const refresh=page.waitForResponse(r=>r.request().headers().rsc==='1'&&new URL(r.url()).pathname===new URL(page.url()).pathname,{timeout:20_000});
    await page.getByRole('button',{name:'Refresh',exact:true}).click();expect((await refresh).status()).toBe(200);
    await expect(page.getByRole('heading',{name:refreshedName,exact:true})).toBeVisible({timeout:20_000});
    }finally{releasePolling();await page.unroute(pollingRoute,holdPolling);}
    await page.evaluate(()=>new Promise<void>(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve()))));
    await expect(shift.locator('input[name="name"]')).toHaveValue('A private draft must stay with A');await expect(shift.locator('input[name="opening"]')).toHaveValue('321.09');
    const failed=page.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname.endsWith('/pms'));
    await shift.getByRole('button',{name:'Open cash shift',exact:true}).click();const refusal=await failed;expect(refusal.status()).toBe(409);expect((await refusal.json()).error.code).toBe('PMS_VERSION_CONFLICT');
    await expect(shift.getByRole('alert')).toBeVisible();await expect(shift.getByRole('button',{name:'Review and use latest version',exact:true})).toBeVisible();
    expect(records(await snapshot(owner,propertyA),'shift').map(r=>r.id).sort()).toEqual(shiftIds);
    const hotel=page.locator('select').filter({has:page.locator(`option[value="${propertyA}"]`)}).first();await hotel.selectOption(propertyB);
    await expect(page.getByRole('heading',{name:'Fictional second hotel · draft isolation',exact:true})).toBeVisible();
    const next=page.locator('details').filter({has:page.locator('summary').filter({hasText:/^Open cash shift$/})});if((await next.getAttribute('open'))===null)await next.locator('summary').click();
    await expect(next.locator('input[name="name"]')).toHaveValue('');await expect(next.locator('input[name="opening"]')).toHaveValue('0');
    expect(records(await snapshot(owner,propertyB),'shift')).toHaveLength(0);
    await page.screenshot({path:info.outputPath('second-property-empty-cash-draft.png')});
    const standard=records(original,'room_type').find(r=>r.data.name==='Standard')!,deluxe=records(original,'room_type').find(r=>r.data.name==='Deluxe')!;
    expect(standard).toBeTruthy();expect(deluxe).toBeTruthy();
    const startRoom=records(original,'room').find(r=>r.data.typeId===standard.id&&r.data.cleanliness==='inspected')!,target=records(original,'room').find(r=>r.data.typeId===deluxe.id&&r.data.cleanliness==='inspected')!;
    const startRate=records(original,'rate').find(r=>r.data.typeId===standard.id&&r.data.active)!,targetRate=records(original,'rate').find(r=>r.data.typeId===deluxe.id&&r.data.active)!;
    expect(targetRate.data.price).toBeGreaterThan(startRate.data.price);
    const guestId=(await command(owner,propertyA,'guest.save',{name:'Fictional higher room type guest'})).id;
    const reservation=(await command(owner,propertyA,'reservation.create',{guestId,source:'staff',rooms:[{typeId:standard.id,rateId:startRate.id,arrival:original.property.businessDate,departure:plusDays(original.property.businessDate,2),adults:2,quantity:1}]})).id;
    const stay=records(await snapshot(owner,propertyA),'stay').find(r=>r.data.reservationId===reservation)!;
    await command(owner,propertyA,'stay.assign',{id:stay.id,roomId:startRoom.id});await command(owner,propertyA,'stay.check_in',{id:stay.id});
    await gotoFrontPms(page,companyId,'front-desk',{property:propertyA});
    const card=page.locator(`[data-pms-record="${stay.id}"]`);await card.locator('summary').filter({hasText:/^Upgrade room$/}).click();
    const form=await pmsDrawerForm(page,'Upgrade room'),drawer=page.getByRole('dialog',{name:'Upgrade room',exact:true}),bounds=(await drawer.boundingBox())!,viewport=page.viewportSize()!;
    expect(Math.abs(bounds.x+bounds.width-viewport.width)).toBeLessThanOrEqual(2);if(info.project.name==='mobile')expect(bounds.width).toBeGreaterThanOrEqual(viewport.width-2);
    await form.locator('select[name="roomId"]').selectOption(target.id);await form.locator('select[name="rateId"]').selectOption(targetRate.id);await form.locator('input[name="reason"]').fill('Fictional guest accepts the reviewed higher room type.');
    await page.screenshot({path:info.outputPath('higher-room-type-upgrade-drawer.png')});
    const saved=page.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname.endsWith('/pms'));await form.getByRole('button',{name:'Upgrade room',exact:true}).click();expect((await saved).status()).toBe(200);
    const after=await snapshot(owner,propertyA),upgraded=records(after,'stay').find(r=>r.id===stay.id)!;expect(upgraded.data.typeId).toBe(deluxe.id);expect(upgraded.data.upgrades?.at(-1)?.reason).toBe('Fictional guest accepts the reviewed higher room type.');expect(records(after,'room').find(r=>r.id===startRoom.id)?.data.cleanliness).toBe('dirty');
    await gotoFrontPms(page,companyId,'front-desk',{property:propertyA});const history=page.locator(`[data-pms-record="${stay.id}"]`);await history.locator('summary').filter({hasText:/^Room upgrade history$/}).click();await expect(history).toContainText('Fictional guest accepts the reviewed higher room type.');
    await info.attach('property-draft-and-upgrade-proof',{body:JSON.stringify({sourceFingerprint:state.sourceFingerprint,release:state.release,propertyA,propertyB,samePropertyRefreshHeadingSeen:true,refreshWhilePollingPaused:true,heldPollRequests,staleWriteStatus:refusal.status(),shiftIdsUnchanged:true,higherRate:true,drawerBounds:bounds,viewport,upgrade:upgraded.data.upgrades?.at(-1)}),contentType:'application/json'});
  }finally{await Promise.all(contexts.map(context=>context.close()));}
});

type Credentials={password:string;users:{email:string;role:string;userId?:string}[]};
type State={sourceFingerprint?:string;softwareVersion?:string;jobsPid?:number;release?:{id:string;fileManifestSha256:string};fixture:{companyId:string;hospitality:{companyId:string;propertyId:string;registerId:string;warehouseId:string;openingJournalId:string;openingInventoryAccountId:string}}};

async function gotoFrontPms(page:Page,companyId:string,section:string,query:Record<string,string>) {
  const target=`${frontOrigin("pms")}${frontWorkspacePath(companyId,section)}?${new URLSearchParams(query)}`;
  await page.goto(target,{waitUntil:"domcontentloaded"});
  await expect(page).toHaveURL(target);
  await expect(page.locator("[data-work-company]").first()).toHaveAttribute("data-work-company",companyId);
}

async function pmsDrawerForm(page:Page,title:string){
  const drawer=page.getByRole('dialog',{name:title,exact:true});await expect(drawer).toHaveCount(1);await expect(drawer).toBeVisible();
  const form=drawer.locator('form');await expect(form).toHaveCount(1);
  await expect(form.getByRole('button',{name:title,exact:true})).toHaveAttribute('data-client-managed-ready','true');
  return form;
}

async function signIn(context:BrowserContext,credentials:Credentials,role:string) {
  const frontApp=role.startsWith("pms_")?"pms":role.startsWith("pos_")?"pos":null;
  if(frontApp){
    const run=process.env.ERPX_DEMO_RUN;
    expect(run,"Prepare an isolated hospitality browser simulation first.").toMatch(/^[a-f0-9]{12}$/);
    const state=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`),"utf8")) as State;
    const page=await context.newPage();
    try {
      await signInFrontRole(context,page,credentials as SimulatedFrontCredentials,role,frontApp,state.fixture.hospitality.companyId,frontWorkspacePath(state.fixture.hospitality.companyId));
    } finally { await page.close(); }
    return;
  }
  const user=credentials.users.find(item=>item.role===role&&item.email.endsWith("@riverside-lantern.example"));
  expect(user,`${role} Riverside account`).toBeTruthy();
  const login=await context.request.post("/api/auth/sign-in/email",{data:{email:user!.email,password:credentials.password}});
  expect(login.ok()).toBe(true);
  const endpoint="/api/v1/holdings/riverside-lantern/assignments/active";
  const active=(await(await context.request.get(endpoint)).json()).data;
  const assignment=active.options.find((option:{role:string})=>option.role===role);
  expect(assignment,`${role} Riverside assignment`).toBeTruthy();
  if(active.assignmentId!==assignment.id)expect((await context.request.post(endpoint,{data:{assignmentId:assignment.id,revision:active.revision}})).ok()).toBe(true);
}

test("Riverside hotel and restaurant remain isolated and render for independent pilot roles",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  expect(state.fixture.hospitality.companyId).not.toBe(state.fixture.companyId);
  const roles=["pms_frontdesk","pms_housekeeping","pos_cashier","pos_kitchen","finance_approver","pms_night_auditor"];
  const contexts:BrowserContext[]=[];
  try {
    for(const role of roles) {
      const context=await createEngineeringBrowserContext(browser,testInfo,{viewport:role==="pos_cashier"?{width:390,height:844}:undefined});
      contexts.push(context);
      await signIn(context,credentials,role);
      const page:Page=await context.newPage();
      const path=role.startsWith("pos_")?"/pos":role==="finance_approver"?"/finance":"/pms";
      await page.goto(`/holdings/riverside-lantern${path}?property=${state.fixture.hospitality.propertyId}`);
      if(path==="/pms")await expect(page.getByRole("heading",{name:"Riverside Lantern Hotel & Kitchen"})).toBeVisible();
      else if(path==="/pos")await expect(page.getByRole("heading",{name:"Point of sale"})).toBeVisible();
      else await expect(page).toHaveURL(/\/finance/);
      if(role==="pos_cashier")expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    }
    const frontDesk=contexts[0]!;
    const crossHolding=await contexts[2]!.request.get("/api/v1/holdings/simulation/pms");
    expect(crossHolding.ok()).toBe(false);
    const hotel=await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${state.fixture.hospitality.propertyId}`);
    expect(hotel.ok()).toBe(true);
    const payload=(await hotel.json()).data;
    expect(payload.snapshot.property.id).toBe(state.fixture.hospitality.propertyId);
    expect(payload.snapshot.records.filter((record:{kind:string})=>record.kind==="room")).toHaveLength(8);
    expect(payload.snapshot.records.filter((record:{kind:string})=>record.kind==="room_type")).toHaveLength(2);
    const manifest=JSON.parse(await readFile(resolve(directory,"databases.json"),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const stock=await db.query("SELECT COALESCE(SUM(value_minor),0)::bigint value FROM commerce_stock WHERE warehouse_id=$1",[state.fixture.hospitality.warehouseId]);
      const ledger=await db.query("SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0)::bigint value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=$1 AND j.status='posted' AND l.account_id=$2",[state.fixture.hospitality.companyId,state.fixture.hospitality.openingInventoryAccountId]);
      expect(Number(stock.rows[0].value)).toBe(Number(ledger.rows[0].value));
      expect(Number(stock.rows[0].value)).toBeGreaterThan(0);
      const approval=await db.query("SELECT status,created_by,posted_by FROM finance_journals WHERE id=$1",[state.fixture.hospitality.openingJournalId]);
      expect(approval.rows[0].status).toBe("posted");
      expect(approval.rows[0].created_by).not.toBe(approval.rows[0].posted_by);
    } finally {await db.end();}
  } finally {await Promise.all(contexts.map(context=>context.close()));}
});

test("cashier and kitchen complete a restaurant bill on the existing hotel guest account",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  const cashier=await createEngineeringBrowserContext(browser,testInfo),kitchen=await createEngineeringBrowserContext(browser,testInfo),frontDesk=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await Promise.all([signIn(cashier,credentials,"pos_cashier"),signIn(kitchen,credentials,"pos_kitchen"),signIn(frontDesk,credentials,"pms_frontdesk")]);
    const cashierPage=await cashier.newPage();
    await cashierPage.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const openShifts=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts.filter((shift:{state:string})=>shift.state==="open");
    if(!openShifts.length)await cashierPage.getByRole("button",{name:"Open cash shift",exact:true}).click();
    await expect(cashierPage.getByRole("button",{name:"Close cash shift"})).toBeVisible();
    await cashierPage.goto("/holdings/riverside-lantern/pos");
    await cashierPage.getByRole("button",{name:"New sale / takeaway",exact:true}).click();
    await cashierPage.getByRole("button",{name:/Basil chicken rice/}).click();
    await cashierPage.getByRole("button",{name:"Add to cart",exact:true}).click();
    await expect(cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Charge to room"})).toBeVisible();
    const posApi="/api/v1/holdings/riverside-lantern/pos";
    const saleList=(await(await cashier.request.get(posApi)).json()).data;
    const saleId=String(saleList.sales[0].id);
    await cashierPage.goto('/holdings/riverside-lantern/pos?tab=kitchen');
    await expect(cashierPage.getByRole('button',{name:/Start · consume ingredients/})).toHaveCount(0);
    await cashierPage.getByRole('button',{name:'Send to kitchen',exact:true}).click();
    await expect(cashierPage.getByRole('button',{name:'Send to kitchen',exact:true})).toHaveCount(0);
    const kitchenPage=await kitchen.newPage();
    await kitchenPage.goto("/holdings/riverside-lantern/pos?tab=kitchen");
    await expect(kitchenPage.getByRole('button',{name:'Send to kitchen',exact:true})).toHaveCount(0);
    await kitchenPage.getByRole("button",{name:/Start · consume ingredients/}).click();
    await kitchenPage.getByRole("button",{name:"Ready to serve"}).click();
    await expect(kitchenPage.getByRole('button',{name:'Delivered',exact:true})).toHaveCount(0);
    await cashierPage.reload();
    await cashierPage.getByRole('button',{name:'Delivered',exact:true}).click();
    await expect(cashierPage.getByRole('button',{name:'Delivered',exact:true})).toHaveCount(0);
    await cashierPage.goto(`/holdings/riverside-lantern/pos?sale=${saleId}`);
    await cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Charge to room"}).click();
    await cashierPage.getByRole("combobox",{name:"Verify room and guest",exact:true}).selectOption({label:"202 · Existing Demo Guest"});
    await cashierPage.getByRole("button",{name:"Charge full bill",exact:true}).click();
    await expect(cashierPage.locator('#workspace-content, #front-content').getByText(/Charged to hotel guest account/).first()).toBeVisible();
    await cashierPage.reload();
    await expect(cashierPage.locator('#workspace-content, #front-content').getByText(/Charged to hotel guest account/).first()).toBeVisible();
    const sale=(await(await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
    expect(sale.sale.state).toBe("completed");
    expect(sale.roomCharge.guest_account_entry_id).toBeTruthy();
    const hotel=(await(await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${state.fixture.hospitality.propertyId}`)).json()).data;
    expect(hotel.snapshot.records.some((record:{id:string;kind:string})=>record.kind==="entry"&&record.id===sale.roomCharge.guest_account_entry_id)).toBe(true);
    const manifest=JSON.parse(await readFile(resolve(directory,"databases.json"),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const source=await db.query("SELECT count(*)::int n FROM pms_sources WHERE property_id=$1 AND source_key=$2",[state.fixture.hospitality.propertyId,`pos:${saleId}:${saleId}`]);
      expect(source.rows[0].n).toBe(1);
      const posting=await db.query("SELECT p.journal_id,COALESCE(SUM(l.debit_minor),0)::bigint debit,COALESCE(SUM(l.credit_minor),0)::bigint credit FROM pms_postings p JOIN finance_journal_lines l ON l.journal_id=p.journal_id WHERE p.property_id=$1 AND p.record_id=$2 GROUP BY p.journal_id",[state.fixture.hospitality.propertyId,sale.roomCharge.guest_account_entry_id]);
      expect(posting.rows).toHaveLength(1);
      expect(Number(posting.rows[0].debit)).toBe(Number(posting.rows[0].credit));
      expect(Number(posting.rows[0].debit)).toBeGreaterThan(0);
      const duplicate=await db.query("SELECT count(*)::int n FROM pos_postings WHERE source_key=$1 OR source_key=$2",[`sale:${saleId}`,`receipt:${saleId}`]);
      expect(duplicate.rows[0].n).toBe(0);
      const payment=await db.query("SELECT count(*)::int n FROM pos_payments WHERE sale_id=$1",[saleId]);
      expect(payment.rows[0].n).toBe(0);
    } finally {await db.end();}
  } finally {await Promise.all([cashier.close(),kitchen.close(),frontDesk.close()]);}
});

test("wrong-room, stale and revoked-cashier room charges leave the bill recoverable",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  const cashier=await createEngineeringBrowserContext(browser,testInfo),frontDesk=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await Promise.all([signIn(cashier,credentials,"pos_cashier"),signIn(frontDesk,credentials,"pms_frontdesk")]);
    const page=await cashier.newPage();
    await page.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const posApi="/api/v1/holdings/riverside-lantern/pos";
    const shifts=(await(await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts;
    if(!shifts.some((shift:{state:string})=>shift.state==="open"))await page.getByRole("button",{name:"Open cash shift",exact:true}).click();
    await expect(page.getByRole("button",{name:"Close cash shift"})).toBeVisible();
    await page.goto("/holdings/riverside-lantern/pos");
    await page.getByRole("button",{name:"New sale / takeaway",exact:true}).click();
    await page.getByRole("button",{name:/Bottled water/}).click();
    await page.getByRole("button",{name:"Add to cart",exact:true}).click();
    const saleId=String((await(await cashier.request.get(posApi)).json()).data.sales[0].id);
    await page.goto(`/holdings/riverside-lantern/pos?sale=${saleId}`);
    await page.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Charge to room"}).click();
    const roomChoices=page.getByRole("combobox",{name:"Verify room and guest",exact:true});
    await expect(roomChoices).toBeVisible();
    const choices=await roomChoices.locator("option").allTextContents();
    expect(choices).toContain("202 · Existing Demo Guest");
    expect(choices.some(choice=>/^(101|104) ·/.test(choice))).toBe(false);
    const workHeaders=await page.locator("[data-work-revision]").first().evaluate(node=>({
      "x-erpx-context-revision":(node as HTMLElement).dataset.workRevision??"",
      "x-erpx-context-company":(node as HTMLElement).dataset.workCompany??"",
      "x-erpx-context-assignment":(node as HTMLElement).dataset.workAssignment??"",
    }));
    const sale=(await(await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data.sale;
    const hotel=(await(await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${state.fixture.hospitality.propertyId}`)).json()).data;
    const inHouseStay=hotel.snapshot.records.find((record:{kind:string;data:{state?:string}})=>record.kind==="stay"&&record.data.state==="in_house");
    expect(inHouseStay).toBeTruthy();
    const manifest=JSON.parse(await readFile(resolve(directory,"databases.json"),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const before=await db.query("SELECT COALESCE(SUM(quantity_milli),0)::bigint quantity FROM commerce_stock WHERE warehouse_id=$1",[state.fixture.hospitality.warehouseId]);
      const command=async(input:Record<string,unknown>)=>page.evaluate(async({path,headers,data})=>{
        const response=await fetch(path,{method:'POST',headers,body:JSON.stringify(data)});
        return {status:response.status,body:await response.json()};
      },{path:`/api/v1/front/riverside-lantern/${encodeURIComponent(state.fixture.hospitality.companyId)}/pos/sale/charge_to_room`,headers:{'Content-Type':'application/json','x-erpx-front-location':'holding','Idempotency-Key':crypto.randomUUID(),...workHeaders},data:{id:saleId,expectedVersion:sale.version,propertyId:state.fixture.hospitality.propertyId,stayId:inHouseStay.id,expectedPropertyVersion:hotel.snapshot.property.version,...input}});
      const cases=[
        {input:{stayId:"not-a-stay"},code:"PMS_CHARGE_STAY"},
        {input:{expectedVersion:sale.version+1},code:"POS_VERSION_CONFLICT"},
        {input:{expectedPropertyVersion:hotel.snapshot.property.version+1},code:"PMS_VERSION_CONFLICT"},
      ];
      for(const scenario of cases){
        const response=await command(scenario.input);
        expect(response.status).toBe(409);
        expect(response.body.error.code).toBe(scenario.code);
        const latest=(await(await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
        expect(latest.sale.state).toBe("open");
        expect(latest.sale.version).toBe(sale.version);
        expect(latest.roomCharge).toBeFalsy();
        const sources=await db.query("SELECT count(*)::int n FROM pms_sources WHERE property_id=$1 AND source_key=$2",[state.fixture.hospitality.propertyId,`pos:${saleId}:${saleId}`]);
        expect(sources.rows[0].n).toBe(0);
        const after=await db.query("SELECT COALESCE(SUM(quantity_milli),0)::bigint quantity FROM commerce_stock WHERE warehouse_id=$1",[state.fixture.hospitality.warehouseId]);
        expect(after.rows[0].quantity).toBe(before.rows[0].quantity);
      }
      await page.reload();
      await page.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Charge to room"}).click();
      await expect(page.getByRole("button",{name:"Charge full bill",exact:true})).toBeEnabled();
      const assignmentId=workHeaders["x-erpx-context-assignment"];
      expect(assignmentId).toBeTruthy();
      await db.query("UPDATE assignments SET revoked_at=$1 WHERE id=$2",[Date.now(),assignmentId]);
      try {
        const revoked=await command({});
        expect(revoked.status).toBe(403);
        expect(revoked.body.error.code).toBe("FRONT_ACCESS_DENIED");
        const persisted=await db.query("SELECT state,version FROM pos_sales WHERE id=$1",[saleId]);
        expect(persisted.rows[0].state).toBe("open");
        expect(Number(persisted.rows[0].version)).toBe(sale.version);
        const sources=await db.query("SELECT count(*)::int n FROM pms_sources WHERE property_id=$1 AND source_key=$2",[state.fixture.hospitality.propertyId,`pos:${saleId}:${saleId}`]);
        expect(sources.rows[0].n).toBe(0);
        const after=await db.query("SELECT COALESCE(SUM(quantity_milli),0)::bigint quantity FROM commerce_stock WHERE warehouse_id=$1",[state.fixture.hospitality.warehouseId]);
        expect(after.rows[0].quantity).toBe(before.rows[0].quantity);
      } finally {await db.query("UPDATE assignments SET revoked_at=NULL WHERE id=$1",[assignmentId]);}
    } finally {await db.end();}
  } finally {await Promise.all([cashier.close(),frontDesk.close()]);}
});

test("hotel cashier records a transfer and independent Finance matches bank evidence",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  const cashier=await createEngineeringBrowserContext(browser,testInfo),financePreparer=await createEngineeringBrowserContext(browser,testInfo),finance=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await Promise.all([signIn(cashier,credentials,"pms_cashier"),signIn(financePreparer,credentials,"finance_preparer"),signIn(finance,credentials,"finance_approver")]);
    const propertyId=state.fixture.hospitality.propertyId;
    const transferReference=`SANDBOX-HOTEL-${crypto.randomUUID()}`;
    const cashierPage=await cashier.newPage();
    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"cashier",{property:propertyId});
    const initial=(await(await cashier.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    const cashierId=credentials.users.find(user=>user.role==="pms_cashier"&&user.email.endsWith("@riverside-lantern.example"))!.userId;
    if(!initial.snapshot.records.some((row:{kind:string;data:{state?:string;userId?:string}})=>row.kind==="shift"&&row.data.state==="open"&&row.data.userId===cashierId)){
      await cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Open cash shift"}).click();
      await cashierPage.locator('#workspace-content, #front-content').getByLabel("Shift / till name").fill("Hotel transfer desk");
      await cashierPage.getByRole("button",{name:"Open cash shift",exact:true}).click();
    }
    const hotel=(await(await cashier.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    const guestAccount=hotel.snapshot.records.find((row:{kind:string;data:{state?:string}})=>row.kind==="guest_account"&&row.data.state==="open");
    expect(guestAccount).toBeTruthy();
    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"guest-accounts",{property:propertyId,guestAccount:guestAccount.id});
    await cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Record bank transfer pending verification"}).click();
    const transferForm=await pmsDrawerForm(cashierPage,'Record bank transfer pending verification');
    await transferForm.getByLabel("Transfer amount").fill("42.80");
    await transferForm.getByLabel("Bank transfer reference").fill(transferReference);
    const transferSaved=cashierPage.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===`/api/v1/front/riverside-lantern/${encodeURIComponent(state.fixture.hospitality.companyId)}/pms`);
    await transferForm.getByRole("button",{name:"Record bank transfer pending verification",exact:true}).click();
    expect((await transferSaved).ok()).toBe(true);
    const pendingTransferCard=cashierPage.locator('#workspace-content, #front-content').locator("article").filter({hasText:transferReference}).last();
    await expect(pendingTransferCard.getByText("Pending",{exact:true})).toBeVisible();
    const manifest=JSON.parse(await readFile(resolve(directory,"databases.json"),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const pending=await db.query("SELECT id,state,amount_minor FROM pms_transfer_requests WHERE property_id=$1 AND reference=$2",[propertyId,transferReference]);
      expect(pending.rows).toHaveLength(1);
      expect(pending.rows[0].state).toBe("pending");
      const account=await db.query("SELECT config_json::jsonb->'accounts'->>'bank' bank FROM pms_properties WHERE id=$1",[propertyId]);
      expect(account.rows[0].bank).toBeTruthy();
      const statementId=crypto.randomUUID();
      const preparerPage=await financePreparer.newPage();
      await preparerPage.goto("/holdings/riverside-lantern/finance/bank?new=1");
      await expect(preparerPage.getByRole("heading",{name:"Import bank statement",exact:true})).toBeVisible();
      const importCard=preparerPage.locator('#workspace-content [data-feature-widget="finance-bank-import-statement"]:visible');
      await expect(importCard).toHaveCount(1);await expect(importCard).toBeVisible();
      await importCard.locator('select[name="bankAccount"]').selectOption(account.rows[0].bank);
      await importCard.getByLabel("Statement number",{exact:true}).fill(`SANDBOX-PMS-${statementId}`);
      await importCard.getByLabel("Statement date",{exact:true}).fill(hotel.snapshot.property.businessDate);
      await importCard.getByLabel("Opening balance",{exact:true}).fill("0");
      await importCard.getByLabel("Closing balance",{exact:true}).fill("42.80");
      await importCard.getByLabel("Bank reference 1",{exact:true}).fill(transferReference);
      await importCard.getByLabel("Transaction date 1",{exact:true}).fill(hotel.snapshot.property.businessDate);
      await importCard.getByLabel("Description 1",{exact:true}).fill("Fictional hotel guest transfer");
      await importCard.getByLabel("Signed amount 1",{exact:true}).fill("42.80");
      const bankImport=preparerPage.waitForResponse(response=>response.request().method()==="POST"&&response.url().endsWith("/finance/bank/statements/import"));
      await importCard.getByRole("button",{name:"Validate and import",exact:true}).click();
      expect((await bankImport).ok()).toBe(true);
      const imported=await db.query("SELECT t.id FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE s.company_id=$1 AND t.external_id=$2",[state.fixture.hospitality.companyId,transferReference]);
      expect(imported.rows).toHaveLength(1);
      const bankId=imported.rows[0].id;
      const financePage=await finance.newPage();
      await financePage.goto("/holdings/riverside-lantern/finance/hotels");
      const transferSummary=financePage.locator('#workspace-content summary').filter({hasText:transferReference});
      await expect(transferSummary).toBeVisible();
      await transferSummary.click();
      const reviewDrawer=financePage.getByRole("dialog",{name:new RegExp(transferReference)});
      await expect(reviewDrawer).toBeVisible();
      await reviewDrawer.getByLabel("Bank evidence").selectOption(bankId);
      await reviewDrawer.getByRole("button",{name:"Verify and post receipt",exact:true}).click();
      await expect(transferSummary).toHaveCount(0);
      const verified=await db.query("SELECT state,entry_id,finance_payment_id FROM pms_transfer_requests WHERE id=$1",[pending.rows[0].id]);
      expect(verified.rows[0].state).toBe("verified");
      const matched=await db.query("SELECT matched_type,matched_id,match_status FROM finance_bank_transactions WHERE id=$1",[bankId]);
      expect(matched.rows[0]).toMatchObject({matched_type:"payment",matched_id:verified.rows[0].finance_payment_id,match_status:"matched"});
      await financePage.reload();
      await expect(financePage.locator('#workspace-content summary').filter({hasText:transferReference})).toHaveCount(0);
    } finally {await db.end();}
  } finally {await Promise.all([cashier.close(),financePreparer.close(),finance.close()]);}
});

test("a typed transfer stays pending and the cashier can recover the bill",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(60_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const cashier=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await signIn(cashier,credentials,"pos_cashier");
    const page=await cashier.newPage();
    await page.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const shifts=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts;
    if(!shifts.some((shift:{state:string})=>shift.state==="open")){
      const opened=page.waitForResponse(response=>response.request().method()==='POST'&&response.url().endsWith('/pos/shift/open'));
      await page.getByRole('button',{name:'Open cash shift',exact:true}).click();
      expect((await opened).ok()).toBe(true);
    }
    await expect(page.getByRole('button',{name:'Close cash shift',exact:true})).toBeVisible();
    await page.goto("/holdings/riverside-lantern/pos");
    await page.getByRole("button",{name:"New sale / takeaway",exact:true}).click();
    await page.getByRole("button",{name:/Bottled water/}).click();
    await page.getByRole("button",{name:"Add to cart",exact:true}).click();
    await expect(page.locator('#workspace-content, #front-content').getByLabel("Amount · split tender",{exact:true})).toHaveValue("42.8");
    const paymentForm=page.locator('#workspace-content, #front-content').locator("form").filter({has:page.getByRole("button",{name:"Record payment",exact:true})});
    const paymentMethod=paymentForm.getByRole("combobox",{name:"Payment method",exact:true});
    await expect(paymentMethod).toHaveCount(1);await expect(paymentMethod).toBeVisible();
    await paymentMethod.selectOption("transfer");
    await page.locator('#workspace-content, #front-content').getByLabel("Reference",{exact:true}).fill("SANDBOX-TRANSFER-PENDING");
    await expect(paymentMethod).toHaveValue("transfer");
    await page.getByRole("button",{name:"Record payment",exact:true}).click();
    await expect(page.locator('#workspace-content, #front-content').getByText("Bank transfer recorded — awaiting verification")).toBeVisible();
    await expect(page.getByRole("button",{name:"Complete sale",exact:true})).toBeDisabled();
    await page.getByRole("button",{name:"Cancel unverified reference"}).click();
    await expect(page.locator('#workspace-content, #front-content').getByText("Bank transfer recorded — awaiting verification")).toHaveCount(0);
    await page.getByRole("button",{name:"Cancel sale",exact:true}).click();
    await expect(page.locator('#workspace-content, #front-content').getByText("SANDBOX-TRANSFER-PENDING")).toHaveCount(0);
  } finally {await cashier.close();}
});

test("cashier records an external tax reference and independent Finance verifies it",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const cashier=await createEngineeringBrowserContext(browser,testInfo),finance=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await Promise.all([signIn(cashier,credentials,"pos_cashier"),signIn(finance,credentials,"finance_approver")]);
    const page=await cashier.newPage();
    await page.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const shifts=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts;
    if(!shifts.some((shift:{state:string})=>shift.state==="open"))await page.getByRole("button",{name:"Open cash shift",exact:true}).click();
    await expect(page.getByRole("button",{name:"Close cash shift"})).toBeVisible();
    await page.goto("/holdings/riverside-lantern/pos");
    await page.getByRole("button",{name:"New sale / takeaway",exact:true}).click();
    await page.getByRole("button",{name:/Bottled water/}).click();
    await page.getByRole("button",{name:"Add to cart",exact:true}).click();
    await page.getByRole("button",{name:"Record payment",exact:true}).click();
    await page.getByRole("button",{name:"Complete sale",exact:true}).click();
    await expect(page.getByRole("heading",{name:"External tax document"})).toBeVisible();
    const documentHref=await page.getByRole("link",{name:"Accounting document"}).getAttribute("href");
    const invoiceId=String(documentHref?.split("/").at(-1));
    const reference=`DEMO-TAX-${invoiceId.slice(0,8)}`;
    expect(invoiceId).toBeTruthy();
    await page.locator('#workspace-content, #front-content').getByLabel("Verified external issuer").fill("Fictional hotel tax desk");
    await page.getByRole("textbox",{name:"Document reference",exact:true}).fill(reference);
    await page.locator('#workspace-content, #front-content').getByLabel("Issue date").fill("2026-09-23");
    await page.locator('#workspace-content, #front-content').getByLabel("External document total").fill("42.80");
    await page.locator('#workspace-content, #front-content').getByLabel("External document tax").fill("2.80");
    await page.locator('#workspace-content, #front-content').getByLabel("Evidence note").fill("Checked against sample paper invoice");
    await page.getByRole("button",{name:"Record reference for Finance review"}).click();
    await expect(page.locator('#workspace-content, #front-content').getByText(`Fictional hotel tax desk · ${reference}`)).toBeVisible();
    const financePage=await finance.newPage();
    await financePage.goto(`/holdings/riverside-lantern/finance/invoices/${invoiceId}`);
    const externalTax=financePage.getByRole("region",{name:"External tax document reference",exact:true});
    await expect(externalTax).toHaveCount(1);await expect(externalTax).toBeVisible();
    await expect(externalTax.getByRole("heading",{name:"External tax document",exact:true})).toBeVisible();
    await externalTax.getByRole("button",{name:"Finance: verify reference",exact:true}).click();
    await expect(externalTax.getByText("Issued 2026-09-23 · verified")).toBeVisible();
    const history=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=history")).json()).data.sales as Array<{id:string;invoice_id:string}>;
    const saleId=history.find(sale=>sale.invoice_id===invoiceId)?.id;
    expect(saleId).toBeTruthy();
    await page.goto(`/holdings/riverside-lantern/pos?sale=${saleId}`);
    await expect(page.locator('#workspace-content, #front-content').getByText("Issued 2026-09-23 · verified")).toBeVisible();
  } finally {await Promise.all([cashier.close(),finance.close()]);}
});

test("Night Audit shows restaurant blockers and rejects an early close",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  const cashier=await createEngineeringBrowserContext(browser,testInfo),auditor=await createEngineeringBrowserContext(browser,testInfo);
  try {
    await Promise.all([signIn(cashier,credentials,"pos_cashier"),signIn(auditor,credentials,"pms_night_auditor")]);
    const cashierPage=await cashier.newPage();
    await cashierPage.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const shifts=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts;
    if(!shifts.some((shift:{state:string})=>shift.state==="open"))await cashierPage.getByRole("button",{name:"Open cash shift",exact:true}).click();
    await expect(cashierPage.getByRole("button",{name:"Close cash shift"})).toBeVisible();
    const auditPage=await auditor.newPage();
    await gotoFrontPms(auditPage,state.fixture.hospitality.companyId,"night-audit",{property:state.fixture.hospitality.propertyId});
    await expect(auditPage.getByRole("heading",{name:"Restaurant day controls"})).toBeVisible();
    await expect(auditPage.locator('#workspace-content, #front-content').getByText(/restaurant cash session\(s\) still open/)).toBeVisible();
    await expect(auditPage.getByRole("button",{name:"Start day close"})).toHaveCount(0);
    const hotel=(await(await auditor.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${state.fixture.hospitality.propertyId}&dayControls=1`)).json()).data;
    expect(hotel.dayControls.openShifts).toBeGreaterThan(0);
    expect(hotel.dayControls.stockValueMinor).toBe(hotel.dayControls.stockLedgerMinor);
    const workHeaders=await auditPage.locator("[data-work-revision]").first().evaluate(node=>({
      "x-erpx-context-revision":(node as HTMLElement).dataset.workRevision??"",
      "x-erpx-context-company":(node as HTMLElement).dataset.workCompany??"",
      "x-erpx-context-assignment":(node as HTMLElement).dataset.workAssignment??"",
    }));
    const close=await auditPage.evaluate(async({path,headers,data})=>{
      const response=await fetch(path,{method:'POST',headers,body:JSON.stringify(data)});
      return {status:response.status,body:await response.json()};
    },{path:`/api/v1/front/riverside-lantern/${encodeURIComponent(state.fixture.hospitality.companyId)}/pms`,headers:{'Content-Type':'application/json','x-erpx-front-location':'holding','Idempotency-Key':crypto.randomUUID(),...workHeaders},data:{operation:"night.start",propertyId:state.fixture.hospitality.propertyId,expectedVersion:hotel.snapshot.property.version,input:{}}});
    expect(close.status).toBe(409);
    expect(close.body.error.code).toBe("PMS_DAY_RECONCILIATION_BLOCKED");
  } finally {await Promise.all([cashier.close(),auditor.close()]);}
});

test("front desk and cashier settle checkout and recover Night Audit",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(120_000);
  const state=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`),"utf8")) as State;
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const frontDesk=await createEngineeringBrowserContext(browser,testInfo),cashier=await createEngineeringBrowserContext(browser,testInfo),restaurantCashier=await createEngineeringBrowserContext(browser,testInfo),auditor=await createEngineeringBrowserContext(browser,testInfo),administrator=await createEngineeringBrowserContext(browser,testInfo);
  [frontDesk,cashier,restaurantCashier,auditor,administrator].forEach(context=>context.setDefaultTimeout(15_000));
  const administratorPage=await administrator.newPage();
  let jobsPaused=false;
  const workerControl=(action:'pause-jobs'|'resume-jobs')=>{
    const port=new URL(process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3102').port;
    const result=JSON.parse(execFileSync(pinnedBunBinary(resolve('.')),['--env-file=/dev/null','scripts/local/run-server-script.ts','scripts/local/browser-simulation.ts',action,run!],{cwd:resolve('.'),timeout:30_000,env:browserChildEnvironment(process.env,{ERPX_BROWSER_SIMULATION_PORT:port,ERPX_BROWSER_SIMULATION_ATTACHMENTS:'local',ERPX_BROWSER_SIMULATION_JOBS:'1'})}).toString());
    expect(result.run).toBe(run);expect(result.jobsPaused).toBe(action==='pause-jobs');jobsPaused=action==='pause-jobs';return result;
  };
  try {
    await Promise.all([
      signIn(frontDesk,credentials,"pms_frontdesk"),
      signIn(cashier,credentials,"pms_cashier"),
      signIn(restaurantCashier,credentials,"pos_cashier"),
      signIn(auditor,credentials,"pms_night_auditor"),
      signInFrontRole(administrator,administratorPage,credentials as SimulatedFrontCredentials,"holding_owner","pms",state.fixture.hospitality.companyId,frontWorkspacePath(state.fixture.hospitality.companyId)),
    ]);
    const propertyId=state.fixture.hospitality.propertyId;
    const pmsApi=`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`;
    const frontPmsCommand=`/api/v1/front/riverside-lantern/${encodeURIComponent(state.fixture.hospitality.companyId)}/pms`;
    const submitPms=async(page:Page,name:string)=>{
      const response=page.waitForResponse(row=>row.request().method()==="POST"&&new URL(row.url()).pathname===frontPmsCommand);
      await page.getByRole("button",{name,exact:true}).click();
      expect((await response).ok(),`${name} command`).toBe(true);
    };
    const initial=(await(await frontDesk.request.get(pmsApi)).json()).data;
    const stay=initial.snapshot.records.find((row:{kind:string;data:{state?:string}})=>row.kind==="stay"&&row.data.state==="in_house");
    expect(stay).toBeTruthy();
    const guestAccountId=stay.data.guestAccountId;
    const frontDeskPage=await frontDesk.newPage();
    await gotoFrontPms(frontDeskPage,state.fixture.hospitality.companyId,"front-desk",{property:propertyId});
    await frontDeskPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Post room charge before check-out"}).click();
    await submitPms(frontDeskPage,"Post room charge before check-out");
    const afterNights=(await(await frontDesk.request.get(pmsApi)).json()).data;
    const guestAccount=afterNights.snapshot.records.find((row:{id:string})=>row.id===guestAccountId);
    expect(guestAccount.data.due).toBeGreaterThan(0);

    const cashierPage=await cashier.newPage();
    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"cashier",{property:propertyId});
    const cashierId=credentials.users.find(user=>user.role==="pms_cashier"&&user.email.endsWith("@riverside-lantern.example"))!.userId;
    if(!afterNights.snapshot.records.some((row:{kind:string;data:{state?:string;userId?:string}})=>row.kind==="shift"&&row.data.state==="open"&&row.data.userId===cashierId)){
      await cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Open cash shift"}).click();
      await cashierPage.locator('#workspace-content, #front-content').getByLabel("Shift / till name").fill("Checkout till");
      await submitPms(cashierPage,"Open cash shift");
    }
    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"guest-accounts",{property:propertyId,guestAccount:guestAccountId});
    await cashierPage.locator('#workspace-content, #front-content').locator("summary").filter({hasText:"Receive payment / advance"}).click();
    const receiptForm=await pmsDrawerForm(cashierPage,'Receive payment / advance');
    const receiptAmount=receiptForm.locator('input[name="amount"]');
    await expect(receiptAmount).toHaveCount(1);await expect(receiptAmount).toHaveAttribute('type','number');
    await receiptAmount.fill((guestAccount.data.due/100).toFixed(2));
    await submitPms(cashierPage,"Receive payment / advance");
    const paid=(await(await cashier.request.get(pmsApi)).json()).data;
    expect(paid.snapshot.records.find((row:{id:string})=>row.id===guestAccountId).data.due).toBe(0);

    await frontDeskPage.reload();
    const departingStay=frontDeskPage.locator(`#front-content article[data-pms-record="${stay.id}"]`);
    await expect(departingStay).toHaveCount(1);await expect(departingStay).toBeVisible();
    await departingStay.locator("summary").filter({hasText:"Check out"}).click();
    await submitPms(frontDeskPage,"Check out");
    await expect(departingStay).toHaveCount(0);
    const checkedOut=(await(await frontDesk.request.get(pmsApi)).json()).data;
    expect(checkedOut.snapshot.records.find((row:{id:string})=>row.id===stay.id)?.data).toMatchObject({state:'checked_out',guestAccountId});
    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"guest-accounts",{property:propertyId,guestAccount:guestAccountId});
    const closeGuestAccount=cashierPage.locator('#workspace-content, #front-content').locator("details").filter({hasText:"Close guest account"}).last();
    await expect(closeGuestAccount).toBeVisible();
    await closeGuestAccount.locator("summary").click();
    await submitPms(cashierPage,"Close guest account");
    const closed=(await(await cashier.request.get(pmsApi)).json()).data;
    const closedGuestAccount=closed.snapshot.records.find((row:{id:string})=>row.id===guestAccountId);
    expect(closedGuestAccount.data).toMatchObject({state:"closed",due:0});
    expect(closedGuestAccount.data.invoiceId).toBeTruthy();

    await gotoFrontPms(cashierPage,state.fixture.hospitality.companyId,"cashier",{property:propertyId});
    const openHotelShifts=closed.snapshot.records.filter((row:{kind:string;data:{state?:string;userId?:string}})=>row.kind==="shift"&&row.data.state==="open"&&row.data.userId===cashierId);
    for(const shift of openHotelShifts){
      await cashierPage.locator('#workspace-content, #front-content').locator("article").filter({hasText:shift.data.name}).last().locator("summary").filter({hasText:"Close cash shift"}).click();
      await submitPms(cashierPage,"Close cash shift");
    }
    const posPage=await restaurantCashier.newPage();
    const posApi="/api/v1/holdings/riverside-lantern/pos";
    const openPosSales=(await(await restaurantCashier.request.get(posApi)).json()).data.sales;
    for(const sale of openPosSales.filter((row:{state:string})=>["open","held","payment_pending"].includes(row.state))){
      await posPage.goto(`/holdings/riverside-lantern/pos?sale=${sale.id}`);
      const cancellation=posPage.waitForResponse(response=>response.request().method()==="POST"&&response.url().endsWith("/pos/sale/cancel"));
      await posPage.getByRole("button",{name:"Cancel sale",exact:true}).click();
      expect((await cancellation).ok()).toBe(true);
    }
    await posPage.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const posShifts=(await(await restaurantCashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts;
    const manifest=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/databases.json`),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
    for(const shift of posShifts.filter((row:{state:string})=>row.state==="open")){
      const receipts=await db.query("SELECT COALESCE(SUM(p.amount_minor),0)::bigint cash FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.shift_id=$1 AND p.method='cash' AND p.state='succeeded'",[shift.id]);
      const counted=Number(shift.opening_minor)+Number(receipts.rows[0].cash);
      const shiftCard=posPage.locator('#workspace-content, #front-content').locator("article:visible").filter({has:posPage.getByRole("heading",{name:String(shift.register_name),exact:true})}).filter({has:posPage.getByRole("button",{name:"Close cash shift",exact:true})});
      await expect(shiftCard).toHaveCount(1);await expect(shiftCard).toBeVisible();
      await shiftCard.getByRole("checkbox",{name:"Enter counted total",exact:true}).check();
      await shiftCard.getByLabel("Counted cash",{exact:true}).fill((counted/100).toFixed(2));
      const closedShift=posPage.waitForResponse(response=>response.request().method()==="POST"&&response.url().endsWith("/pos/shift/close"));
      await shiftCard.getByRole("button",{name:"Close cash shift",exact:true}).click();
      const shiftResponse=await closedShift;
      expect(shiftResponse.request().postDataJSON().id).toBe(shift.id);
      expect(shiftResponse.ok()).toBe(true);
    }
    } finally {await db.end();}
    const auditPage=await auditor.newPage();
    await gotoFrontPms(auditPage,state.fixture.hospitality.companyId,"night-audit",{property:propertyId});
    const startDayClose=auditPage.locator('#workspace-content, #front-content').locator("summary:visible").filter({hasText:"Start day close"}).first();
    await expect(startDayClose).toBeVisible();
    await startDayClose.click();
    // Stop only this owned cohort worker before queueing the fault case,
    // so the real worker cannot complete the close before access is revoked.
    expect(state.jobsPid,'the real isolated jobs worker is required for Night Audit recovery').toBeGreaterThan(0);
    workerControl('pause-jobs');
    await submitPms(auditPage,"Start day close");
    const closing=(await(await auditor.request.get(pmsApi)).json()).data;
    expect(closing.snapshot.property.state).toBe("closing");
    expect(closing.snapshot.records.some((row:{kind:string;data:{state?:string}})=>row.kind==="night"&&row.data.state==="pending")).toBe(true);
    const recoveryDb=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const runWorker=async(expected:'blocked'|'completed')=>{
        if(jobsPaused)workerControl('resume-jobs');
        const latestJob=async()=>(await recoveryDb.query('SELECT state,error_code FROM pms_jobs WHERE property_id=$1 ORDER BY updated_at DESC LIMIT 1',[propertyId])).rows[0];
        await expect.poll(async()=> (await latestJob())?.state,{timeout:30_000,intervals:[250,500,1000]}).toBe(expected);
        return {job:await latestJob(),property:(await recoveryDb.query('SELECT state,business_date FROM pms_properties WHERE id=$1',[propertyId])).rows[0]};
      };
      const auditorAssignment=await auditPage.locator("[data-work-revision]").first().evaluate(node=>(node as HTMLElement).dataset.workAssignment??"");
      expect(auditorAssignment).toBeTruthy();
      const revoked=await recoveryDb.query("UPDATE assignments SET revoked_at=$1 WHERE id=$2 AND revoked_at IS NULL RETURNING id",[Date.now(),auditorAssignment]);
      expect(revoked.rows).toHaveLength(1);
      const blocked=await runWorker('blocked');
      expect(blocked.job.state).toBe("blocked");
      expect(blocked.job.error_code).toBeTruthy();
      expect(blocked.property.state).toBe("closing");
      await gotoFrontPms(administratorPage,state.fixture.hospitality.companyId,"night-audit",{property:propertyId});
      const resume=administratorPage.locator('#workspace-content, #front-content').locator("summary:visible").filter({hasText:"Resolve and continue"}).first();
      await expect(resume).toBeVisible();
      await resume.click();
      await submitPms(administratorPage,"Resolve and continue");
      const completed=await runWorker('completed');
      expect(completed).toMatchObject({job:{state:"completed",error_code:null},property:{state:"open"}});
      expect(completed.property.business_date).not.toBe(closing.snapshot.property.businessDate);
      const invoices=await recoveryDb.query("SELECT total_minor,outstanding_minor,status FROM finance_invoices WHERE company_id=$1 AND source_type='pms'",[state.fixture.hospitality.companyId]);
      expect(invoices.rows).toHaveLength(1);
      expect(invoices.rows[0]).toMatchObject({outstanding_minor:"0",status:"paid"});
      const journals=await recoveryDb.query("SELECT COALESCE(SUM(l.debit_minor),0)::bigint debit,COALESCE(SUM(l.credit_minor),0)::bigint credit FROM pms_postings pp JOIN finance_journal_lines l ON l.journal_id=pp.journal_id WHERE pp.property_id=$1",[propertyId]);
      expect(journals.rows[0].debit).toBe(journals.rows[0].credit);
      expect(Number(journals.rows[0].debit)).toBeGreaterThan(0);
      const restaurant=await recoveryDb.query("SELECT state,expected_minor,counted_minor,variance_minor FROM pos_shifts WHERE company_id=$1",[state.fixture.hospitality.companyId]);
      expect(restaurant.rows.every((row:{state:string;expected_minor:string;counted_minor:string;variance_minor:string})=>row.state==='closed'&&row.expected_minor===row.counted_minor&&Number(row.variance_minor)===0)).toBe(true);
      await administratorPage.reload({waitUntil:'domcontentloaded'});
      await expect(administratorPage.getByText(`Business date ${completed.property.business_date} · THB`,{exact:true})).toBeVisible();
      await expect(administratorPage.getByText('This hotel is closing its business day. Ask the hotel administrator to finish the day close.',{exact:true})).toHaveCount(0);
      await administratorPage.screenshot({path:testInfo.outputPath('completed-business-date.png'),fullPage:true});
      if(state.release){
        const restarted=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`),'utf8')) as State;
        expect(restarted.sourceFingerprint).toBe(state.sourceFingerprint);expect(restarted.softwareVersion).toBe(state.softwareVersion);expect(restarted.release).toEqual(state.release);expect(restarted.jobsPid).toBeGreaterThan(0);expect(restarted.jobsPid).not.toBe(state.jobsPid);
        await testInfo.attach('compiled-pms-night-audit-recovery',{body:JSON.stringify({sourceFingerprint:state.sourceFingerprint,softwareVersion:state.softwareVersion,fileManifestSha256:state.release.fileManifestSha256,firstWorkerPid:state.jobsPid,restartedWorkerPid:restarted.jobsPid,blocked,completed}),contentType:'application/json'});
      }
    } finally {await recoveryDb.end();}
  } finally {if(jobsPaused)workerControl('resume-jobs');await Promise.all([frontDesk.close(),cashier.close(),restaurantCashier.close(),auditor.close(),administrator.close()]);}
});
