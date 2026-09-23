import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { Pool } from "pg";

type Credentials={password:string;users:{email:string;role:string}[]};
type State={fixture:{companyId:string;hospitality:{companyId:string;propertyId:string;registerId:string;warehouseId:string;openingJournalId:string;openingInventoryAccountId:string}}};

async function signIn(context:BrowserContext,credentials:Credentials,role:string) {
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
      const context=await browser.newContext({...testInfo.project.use,viewport:role==="pos_cashier"?{width:390,height:844}:undefined});
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
      const ledger=await db.query("SELECT COALESCE(SUM(debit_minor-credit_minor),0)::bigint value FROM finance_journal_lines WHERE journal_id=$1 AND account_id=$2",[state.fixture.hospitality.openingJournalId,state.fixture.hospitality.openingInventoryAccountId]);
      expect(Number(stock.rows[0].value)).toBe(Number(ledger.rows[0].value));
      expect(Number(stock.rows[0].value)).toBeGreaterThan(0);
      const approval=await db.query("SELECT status,created_by,posted_by FROM finance_journals WHERE id=$1",[state.fixture.hospitality.openingJournalId]);
      expect(approval.rows[0].status).toBe("posted");
      expect(approval.rows[0].created_by).not.toBe(approval.rows[0].posted_by);
    } finally {await db.end();}
  } finally {await Promise.all(contexts.map(context=>context.close()));}
});

test("cashier and kitchen complete a restaurant bill on the existing hotel folio",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(90_000);
  const directory=resolve(`.runtime/browser/${run}`);
  const credentials=JSON.parse(await readFile(resolve(directory,"credentials.json"),"utf8")) as Credentials;
  const state=JSON.parse(await readFile(resolve(directory,"state.json"),"utf8")) as State;
  const cashier=await browser.newContext(testInfo.project.use),kitchen=await browser.newContext(testInfo.project.use),frontDesk=await browser.newContext(testInfo.project.use);
  try {
    await Promise.all([signIn(cashier,credentials,"pos_cashier"),signIn(kitchen,credentials,"pos_kitchen"),signIn(frontDesk,credentials,"pms_frontdesk")]);
    const cashierPage=await cashier.newPage();
    await cashierPage.goto("/holdings/riverside-lantern/pos?tab=shifts");
    const openShifts=(await(await cashier.request.get("/api/v1/holdings/riverside-lantern/pos?tab=shifts")).json()).data.shifts.filter((shift:{state:string})=>shift.state==="open");
    if(!openShifts.length)await cashierPage.getByRole("button",{name:"Open cash session",exact:true}).click();
    await expect(cashierPage.getByRole("button",{name:"Close cash session"})).toBeVisible();
    await cashierPage.goto("/holdings/riverside-lantern/pos");
    await cashierPage.getByRole("button",{name:"Open new sale"}).click();
    await cashierPage.getByRole("button",{name:/Basil chicken rice/}).click();
    await cashierPage.getByRole("button",{name:"Add to sale"}).click();
    await expect(cashierPage.getByRole("heading",{name:"Charge full bill to room"})).toBeVisible();
    const posApi="/api/v1/holdings/riverside-lantern/pos";
    const saleList=(await(await cashier.request.get(posApi)).json()).data;
    const saleId=String(saleList.sales[0].id);
    const kitchenPage=await kitchen.newPage();
    await kitchenPage.goto("/holdings/riverside-lantern/pos?tab=kitchen");
    await kitchenPage.getByRole("button",{name:"Send to kitchen"}).click();
    await kitchenPage.getByRole("button",{name:/Start · consume ingredients/}).click();
    await kitchenPage.getByRole("button",{name:"Ready to serve"}).click();
    await kitchenPage.getByRole("button",{name:"Delivered"}).click();
    await cashierPage.goto(`/holdings/riverside-lantern/pos?sale=${saleId}`);
    await cashierPage.getByLabel("In-house guest and room").selectOption({label:"Room 202 · Existing Demo Guest"});
    await cashierPage.getByRole("button",{name:"Charge to room"}).click();
    await expect(cashierPage.getByText("Charged to hotel folio").first()).toBeVisible();
    await cashierPage.reload();
    await expect(cashierPage.getByText("Charged to hotel folio").first()).toBeVisible();
    const sale=(await(await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
    expect(sale.sale.state).toBe("completed");
    expect(sale.roomCharge.folio_entry_id).toBeTruthy();
    const hotel=(await(await frontDesk.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${state.fixture.hospitality.propertyId}`)).json()).data;
    expect(hotel.snapshot.records.some((record:{id:string;kind:string})=>record.kind==="entry"&&record.id===sale.roomCharge.folio_entry_id)).toBe(true);
    const manifest=JSON.parse(await readFile(resolve(directory,"databases.json"),"utf8")) as {databases:{H002_DB:string}};
    const databaseUrl=new URL(manifest.databases.H002_DB);
    expect(databaseUrl.hostname).toBe("127.0.0.1");
    expect(databaseUrl.port).toBe("5432");
    expect(databaseUrl.pathname).toBe(`/erpx_browser_${run}_h002_db`);
    const db=new Pool({connectionString:databaseUrl.href,max:1});
    try {
      const source=await db.query("SELECT count(*)::int n FROM pms_sources WHERE property_id=$1 AND source_key=$2",[state.fixture.hospitality.propertyId,`pos:${saleId}:${saleId}`]);
      expect(source.rows[0].n).toBe(1);
      const posting=await db.query("SELECT p.journal_id,COALESCE(SUM(l.debit_minor),0)::bigint debit,COALESCE(SUM(l.credit_minor),0)::bigint credit FROM pms_postings p JOIN finance_journal_lines l ON l.journal_id=p.journal_id WHERE p.property_id=$1 AND p.record_id=$2 GROUP BY p.journal_id",[state.fixture.hospitality.propertyId,sale.roomCharge.folio_entry_id]);
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

test("a typed transfer stays pending and the cashier can recover the bill",async({browser},testInfo)=>{
  const run=process.env.ERPX_DEMO_RUN;
  test.skip(!run,"Prepare and start a disposable demo run first.");
  test.setTimeout(60_000);
  const credentials=JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`),"utf8")) as Credentials;
  const cashier=await browser.newContext(testInfo.project.use);
  try {
    await signIn(cashier,credentials,"pos_cashier");
    const page=await cashier.newPage();
    await page.goto("/holdings/riverside-lantern/pos");
    await page.getByRole("button",{name:"Open new sale"}).click();
    await page.getByRole("button",{name:/Bottled water/}).click();
    await page.getByRole("button",{name:"Add to sale"}).click();
    await expect(page.getByLabel("Payment amount")).toHaveValue("42.80");
    await page.getByLabel("Method").selectOption("transfer");
    await page.getByLabel("Transfer reference").fill("SANDBOX-TRANSFER-PENDING");
    await expect(page.getByLabel("Method")).toHaveValue("transfer");
    await page.getByRole("button",{name:"Record payment / create link"}).click();
    await expect(page.getByText("Bank transfer recorded — awaiting verification")).toBeVisible();
    await expect(page.getByRole("button",{name:"Confirm and close sale"})).toBeDisabled();
    await page.getByRole("button",{name:"Cancel unverified reference"}).click();
    await expect(page.getByText("Bank transfer recorded — awaiting verification")).toHaveCount(0);
    await page.getByRole("button",{name:"Cancel sale",exact:true}).click();
    await expect(page.getByText("SANDBOX-TRANSFER-PENDING")).toHaveCount(0);
  } finally {await cashier.close();}
});
