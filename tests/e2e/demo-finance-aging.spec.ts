import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string };
type Credentials = { password: string; users: DemoUser[] };

async function signInFinance(context: BrowserContext, page: Page, credentials: Credentials, path: string) {
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: "http://127.0.0.1:3102" },
    data: { email: "finance@simulation.test", password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === "finance_approver");
  expect(assignment, "finance approver assignment").toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: "http://127.0.0.1:3102" },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo renders receivables and payables aging from live finance data", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  expect(credentials.users.find((user) => user.role === "finance_approver"), "finance reviewer demo account").toBeTruthy();
  const context = await browser.newContext(testInfo.project.use);
  try {
    const page = await context.newPage();
    await signInFinance(context, page, credentials, "/holdings/simulation/finance/invoices?kind=receivable&queue=open");
    await expect(page.getByRole("heading", { name: "Receivables aging", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Payables aging", exact: true })).toBeVisible();
    await expect(page.getByText("Open posted obligations grouped by days overdue as of", { exact: false }).first()).toBeVisible();
    await expect(page.getByRole("link", { name: "Open receivables →", exact: true })).toHaveAttribute("href", "/holdings/simulation/finance/invoices?kind=receivable&queue=open");
    const aging = await page.request.get("/api/v1/holdings/simulation/finance/aging?kind=receivable&asOf=2026-09-22");
    expect(aging.ok(), await aging.text()).toBe(true);
    const body = await aging.json() as { data: Array<{ bucket: string; outstanding: string }> };
    expect(body.data).toEqual(expect.arrayContaining([expect.objectContaining({ bucket: "current", outstanding: "300.00" })]));
    await expect(page.getByText("300.00", { exact: true }).first()).toBeVisible();
  } finally {
    await context.close();
  }
});
