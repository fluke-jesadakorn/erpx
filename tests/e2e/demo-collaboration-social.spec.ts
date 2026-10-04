import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  userId: string;
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
  await page.goto(`/holdings/simulation/home?refresh=${Date.now()}`, {
    waitUntil: "domcontentloaded",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
  return user!;
}

test("isolated demo publishes a mentioned update and keeps thread actions after reload", async ({
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
    const mentioned = credentials.users.find((user) =>
      user.roles?.includes("employee"),
    );
    expect(mentioned, "mentioned employee").toBeTruthy();

    await signInRole(ownerContext, ownerPage, credentials, "holding_owner");
    const composer = ownerPage.locator("#workspace-content details.beta-composer");
    await composer.locator("summary").click();
    const body = `Operations handoff ${Date.now()} @Support`;
    await composer.getByLabel("Post message", { exact: true }).fill(body);
    const mentionMenu = ownerPage.getByRole("listbox", {
      name: "People, AI agents, and records",
      exact: true,
    });
    await expect(mentionMenu).toBeVisible();
    const supportOption = mentionMenu
      .getByRole("option")
      .filter({ hasText: "Support agent" })
      .first();
    await expect(supportOption).toBeVisible();
    await supportOption.click();
    await composer.getByLabel("Post audience").selectOption({ index: 1 });
    await composer.getByRole("button", { name: "Share update", exact: true }).click();
    await expect(ownerPage.locator("#workspace-content").getByText(body.replace(" @Support", " @Support agent"), { exact: true })).toBeVisible();
    const post = ownerPage.getByRole("article").filter({ hasText: "Operations handoff" });
    await expect(post).toBeVisible();
    await expect(post.getByText(/@Support agent/)).toBeVisible();

    await signInRole(employeeContext, employeePage, credentials, "employee");
    await expect(
      employeePage.getByRole("article").filter({ hasText: "Operations handoff" }).first(),
    ).toBeVisible();
    const employeePost = employeePage
      .getByRole("article")
      .filter({ hasText: "Operations handoff" })
      .first();
    await employeePost.getByRole("button", { name: /Comment/ }).click();
    const reply = `Acknowledged by employee ${Date.now()}`;
    await employeePost.getByRole("combobox", { name: "Write a comment", exact: true }).fill(reply);
    const commentForm = employeePost.getByRole("form", {
      name: "Write a comment",
      exact: true,
    });
    const commentResponse = employeePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/posts/") &&
        response.url().endsWith("/posts/" + response.url().split("/posts/").pop()),
    );
    await commentForm.getByRole("button", { name: "Comment", exact: true }).click();
    expect((await commentResponse).status()).toBe(200);
    await expect(employeePost.getByText(reply, { exact: true })).toBeVisible();

    await employeePost.getByRole("button", { name: /Thanks/ }).click();
    await expect(employeePost.getByRole("button", { name: /Thanks/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await employeePost.getByLabel("More post actions", { exact: true }).click();
    const follow = employeePost.getByRole("button", { name: "Follow conversation", exact: true });
    await follow.click();
    await expect(employeePost.getByRole("button", { name: "Unfollow conversation", exact: true })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await employeePage.reload({ waitUntil: "domcontentloaded" });
    const reloaded = employeePage
      .getByRole("article")
      .filter({ hasText: "Operations handoff" })
      .first();
    await reloaded.getByRole("button", { name: /Comment/ }).click();
    await expect(reloaded.getByText(reply, { exact: true })).toBeVisible();
    await expect(reloaded.getByRole("button", { name: /Thanks/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await reloaded.getByLabel("More post actions", { exact: true }).click();
    await expect(
      reloaded.getByRole("button", { name: "Unfollow conversation", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
  } finally {
    await ownerContext.close();
    await employeeContext.close();
  }
});
