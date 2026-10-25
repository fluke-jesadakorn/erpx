import { captureInvestorCheckpoint } from "../helpers/investor-capture";
import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page, type Route } from "@playwright/test";
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
    await ownerPage.locator("#workspace-content").getByText("Share an update", { exact: true }).click();
    const composer = ownerPage.getByRole("dialog", { name: "Share an update", exact: true });
    await expect(composer).toBeVisible();
    const body = "Operations handoff: confirm the customer installation window @Support";
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
    let releaseFeed!: () => void;
    const feedGate = new Promise<void>(resolve => { releaseFeed = resolve; });
    let feedHeld = false;
    const feedPath = /\/api\/v1\/holdings\/simulation\/beta\/home(?:\?|$)/;
    const holdFeed = async (route: Route) => {
      if (route.request().method() === "GET") {
        feedHeld = true;
        await feedGate;
      }
      await route.continue();
    };
    await ownerPage.route(feedPath, holdFeed);
    try {
      const publication = ownerPage.waitForResponse(response =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname.startsWith("/api/v1/holdings/simulation/beta/drafts/") &&
        new URL(response.url()).pathname.endsWith("/publish"));
      await composer.getByRole("button", { name: "Share update", exact: true }).click();
      expect((await publication).status()).toBe(200);
      await expect.poll(() => feedHeld).toBe(true);
      await expect(composer).toBeVisible();
      for (const name of ["Post type", "Post audience", "Link existing work"]) {
        await expect(composer.getByRole("combobox", { name, exact: true })).toBeDisabled();
      }
      await expect(composer.getByLabel("Post message", { exact: true })).toHaveAttribute("contenteditable", "false");
      await expect(composer.getByLabel("Add images to post", { exact: true })).toBeDisabled();
      for (const name of ["Advanced", "Save draft", "Share update"]) {
        await expect(composer.getByRole("button", { name, exact: true })).toBeDisabled();
      }
    } finally {
      releaseFeed();
      await ownerPage.unrouteAll({ behavior: "wait" });
    }
    await expect(composer).not.toBeVisible();
    await expect(ownerPage.locator("#workspace-content").getByText(body.replace(" @Support", " @Support agent"), { exact: true })).toBeVisible();
    const post = ownerPage.getByRole("article").filter({ hasText: "Operations handoff" });
    await expect(post).toBeVisible();
    await expect(post.getByText(/@Support agent/)).toBeVisible();
    await captureInvestorCheckpoint(ownerPage, "social-update", post);

    await ownerPage.locator("#workspace-content").getByText("Share an update", { exact: true }).click();
    await expect(composer).toBeVisible();
    await expect(composer.getByLabel("Post message", { exact: true })).toHaveText("");
    await expect(composer.getByLabel("Post audience")).toHaveValue("");
    await expect(composer.getByLabel("Post type")).toHaveValue("update");
    await expect(composer.getByLabel("Link existing work")).toHaveValue("");
    await composer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(ownerPage.getByRole("dialog", { name: "Unsaved changes", exact: true })).not.toBeVisible();
    await expect(composer).not.toBeVisible();
    await expect(post).toBeVisible();

    await signInRole(employeeContext, employeePage, credentials, "employee");
    await expect(
      employeePage.getByRole("article").filter({ hasText: "Operations handoff" }).first(),
    ).toBeVisible();
    const employeePost = employeePage
      .getByRole("article")
      .filter({ hasText: "Operations handoff" })
      .first();
    await employeePost.getByRole("button", { name: /Comment/ }).click();
    const reply = "Acknowledged. Support will confirm the installation window with the customer.";
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
    await captureInvestorCheckpoint(employeePage, "social-thread", reloaded);
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
