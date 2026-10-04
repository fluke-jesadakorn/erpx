import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
};
type Credentials = { password: string; users: DemoUser[] };

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
) {
  const user = credentials.users.find(
    (item) => item.roles?.includes(role) || item.role === role,
  );
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === role,
  );
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`/holdings/simulation/inbox?refresh=${Date.now()}`, {
    waitUntil: "domcontentloaded",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
}

test("isolated demo exchanges a conversation attachment between members", async ({
  browser,
}, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  const employeeContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const ownerPage = await ownerContext.newPage();
    const employeePage = await employeeContext.newPage();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner");
    await signInRole(employeeContext, employeePage, credentials, "employee");

    await ownerPage.getByRole("button", { name: "New chat", exact: true }).click();
    await ownerPage
      .getByRole("radio", { name: /Support agent support-agent@simulation\.test/ })
      .check();
    await ownerPage.getByRole("button", { name: "Start chat", exact: true }).click();
    await expect(ownerPage.getByRole("heading", { name: "Support agent", exact: true })).toBeVisible();
    const greeting = `Attachment handoff ${Date.now()}`;
    await ownerPage.locator("#workspace-content").getByLabel("Message", { exact: true }).fill(greeting);
    await ownerPage.getByRole("button", { name: "Send", exact: true }).click();
    await expect(ownerPage.getByRole("log").getByText(greeting, { exact: true })).toBeVisible();

    const fileName = "handoff-evidence.csv";
    await ownerPage.locator("#workspace-content").getByLabel("Attach files").setInputFiles({
      name: fileName,
      mimeType: "text/csv",
      buffer: Buffer.from("step,result\nattachment,verified\n"),
    });
    const attachmentResponse = ownerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/messaging"),
    );
    await ownerPage.getByRole("button", { name: "Send", exact: true }).click();
    expect((await attachmentResponse).status()).toBe(200);
    await expect(ownerPage.getByRole("link", { name: new RegExp(fileName) })).toBeVisible();

    await employeePage.getByRole("button", { name: /Source operator/ }).click();
    await expect(employeePage.getByRole("log").getByText(greeting, { exact: true })).toBeVisible();
    const received = employeePage.getByRole("link", { name: new RegExp(fileName) });
    await expect(received).toBeVisible();
    const downloadUrl = await received.getAttribute("href");
    expect(downloadUrl).toBeTruthy();
    const downloaded = await employeeContext.request.get(downloadUrl!);
    expect(downloaded.status()).toBe(200);
    expect(await downloaded.text()).toContain("attachment,verified");
    await employeePage.reload({ waitUntil: "domcontentloaded" });
    await employeePage.getByRole("button", { name: /Source operator/ }).click();
    await expect(employeePage.getByRole("link", { name: new RegExp(fileName) })).toBeVisible();
  } finally {
    await ownerContext.close();
    await employeeContext.close();
  }
});
