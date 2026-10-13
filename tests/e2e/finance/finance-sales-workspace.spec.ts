import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createEngineeringBrowserContext } from "../../support/browser/engineering-browser-evidence";
type Credentials = { password: string; users: { email: string; role: string; roles?: string[] }[] };
async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find(user => user.role === role || user.roles?.includes(role)); expect(user).toBeTruthy();
  const origin = process.env.ERPX_E2E_BASE_URL!;
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email: user!.email, password: credentials.password } }); expect(login.ok()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active", active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((row: {role:string}) => row.role === role); expect(assignment).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision } }); expect(selected.ok()).toBe(true);
  await page.goto("/holdings/simulation/finance/sales", { waitUntil: "domcontentloaded" });
}
test("English sales workspace opens drawers, calculates invoices and keeps direct forms available", async ({browser}, info) => {
  const run = process.env.ERPX_DEMO_RUN; expect(run, "An isolated general-company fixture is required").toBeTruthy();
  const credentials = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/credentials.json`), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(`.runtime/browser/${run}/state.json`), "utf8")) as {fixture:{customerId:string}};
  const context = await createEngineeringBrowserContext(browser, info);
  try {
    const page = await context.newPage(); await signIn(context,page,credentials,"finance_preparer");
    await expect(page.getByRole("heading",{name:"Sales & collections",exact:true})).toBeVisible();
    for (const label of ["Awaiting payment","Overdue","Received"]) await expect(page.getByRole("heading",{name:label,exact:true})).toBeVisible();
    for (const label of ["Quotations","Sales orders","Invoices","Receipts"]) await expect(page.getByRole("navigation",{name:"Sales documents"}).getByRole("link",{name:label,exact:true})).toBeVisible();
    await page.getByRole("button",{name:"Search",exact:true}).click();
    await expect(page.getByRole("heading",{name:"Sales & collections",exact:true})).toBeVisible();
    await page.getByRole("link",{name:"Invoice completed services",exact:true}).click();
    const drawer = page.getByRole("dialog",{name:"Invoice completed services",exact:true}); await expect(drawer).toBeVisible();
    const box = await drawer.boundingBox(); expect(box).toBeTruthy(); const width = page.viewportSize()!.width;
    expect(box!.x + box!.width).toBeGreaterThan(width - 10);
    if (info.project.name === "mobile") expect(box!.width).toBeGreaterThan(width - 10);
    await expect(drawer.getByLabel(/debit|credit|ledger account/i)).toHaveCount(0);
    await drawer.getByRole("combobox",{name:"Customer",exact:true}).selectOption(state.fixture.customerId);
    await drawer.getByRole("textbox",{name:"Description",exact:true}).fill("Completed installation acceptance test");
    await drawer.getByLabel("Quantity",{exact:true}).fill("2"); await drawer.getByLabel("Unit price",{exact:true}).fill("100");
    await drawer.getByLabel("Completion evidence",{exact:true}).fill("Fictional installation completed and accepted by the customer");
    await drawer.getByRole("checkbox").check(); await expect(drawer.getByText("Total THB 214.00",{exact:true})).toBeVisible();
    await info.attach("sales-service-drawer",{body:await page.screenshot({fullPage:true}),contentType:"image/png"});
    const prepared = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/finance/sales/invoices"));
    await drawer.getByRole("button",{name:"Prepare invoice",exact:true}).click(); const response = await prepared; expect(response.ok(),await response.text()).toBe(true);
    const data = (await response.json()).data; expect(data.number).toMatch(/^INV-\d{4}-\d{6,}$/);
    await expect(page.getByRole("heading",{name:data.number,exact:true})).toBeVisible();
    await expect(page.getByText("Completed installation acceptance test",{exact:true}).first()).toBeVisible();
    await page.locator("summary").filter({hasText:"Service completion evidence"}).click();
    await expect(page.getByText("Fictional installation completed and accepted by the customer",{exact:true})).toBeVisible();
    await page.goto("/holdings/simulation/finance/sales/new?kind=invoice");
    await expect(page.getByRole("heading",{name:"Invoice completed services",exact:true})).toBeVisible(); await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.goto("/holdings/simulation/finance/sales?tab=receipts");
    await expect(page.getByRole("heading",{name:"Sales & collections",exact:true})).toBeVisible();
    await expect(page.getByRole("navigation",{name:"Sales documents"}).getByRole("link",{name:"Receipts",exact:true})).toHaveAttribute("aria-current","page");
    await signIn(context,page,credentials,"holding_owner");
    await page.getByRole("link",{name:"New quotation",exact:true}).click();
    const quotation=page.getByRole("dialog",{name:"Order details",exact:true});await expect(quotation).toBeVisible();
    await expect(quotation.getByLabel(/debit|credit|ledger account/i)).toHaveCount(0);
    await info.attach("sales-quotation-drawer",{body:await page.screenshot({fullPage:true}),contentType:"image/png"});
  } finally { await context.close(); }
});
