import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { qualityDueSoonHoldId?: string } };

async function openRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo records independent inspection and third-person quality release", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const holdId = state.fixture?.qualityDueSoonHoldId;
  expect(holdId).toBeTruthy();
  const inspectorContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  try {
    const inspectorPage = await inspectorContext.newPage();
    await openRole(inspectorContext, inspectorPage, credentials, "quality_inspector", `/holdings/simulation/quality/${holdId}`);
    await expect(inspectorPage.getByRole("heading", { name: "Record inspection", exact: true })).toBeVisible();
    await inspectorPage.getByRole("combobox", { name: "Result", exact: true }).selectOption("pass");
    await inspectorPage.getByLabel("Inspection evidence", { exact: true }).fill("Independent inspector verified the held demo stock and found it within release criteria.");
    const inspectionResponse = inspectorPage.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/quality/${holdId}/inspect`));
    await inspectorPage.getByRole("button", { name: "Record inspection", exact: true }).click();
    expect((await inspectionResponse).ok()).toBe(true);
    const inspected = await inspectorContext.request.get(`/api/v1/holdings/simulation/quality/${holdId}`);
    expect(inspected.ok(), await inspected.text()).toBe(true);
    expect((await inspected.json()).data.hold).toMatchObject({ status: "awaiting_release", last_result: "pass" });

    const managerPage = await managerContext.newPage();
    await openRole(managerContext, managerPage, credentials, "quality_manager", `/holdings/simulation/quality/${holdId}`);
    await expect(managerPage.getByRole("heading", { name: "Approve release", exact: true })).toBeVisible();
    await managerPage.getByLabel("Release reason", { exact: true }).fill("Independent Quality manager reviewed the passing inspection and releases the stock.");
    const releaseResponse = managerPage.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname.endsWith(`/quality/${holdId}/release`));
    await managerPage.getByRole("button", { name: "Release this hold", exact: true }).click();
    expect((await releaseResponse).ok()).toBe(true);
    await expect(managerPage.locator("#workspace-content")).toContainText("This hold is released.");
    const released = await managerContext.request.get(`/api/v1/holdings/simulation/quality/${holdId}`);
    expect(released.ok(), await released.text()).toBe(true);
    expect((await released.json()).data.hold).toMatchObject({ status: "released", last_result: "pass" });
    await testInfo.attach("quality-inspection-release-result", {
      contentType: "application/json",
      body: JSON.stringify({ run, holdId, inspection: "pass", release: "released", inspectorRole: "quality_inspector", managerRole: "quality_manager" }, null, 2),
    });
  } finally {
    await Promise.all([inspectorContext.close(), managerContext.close()]);
  }
});
