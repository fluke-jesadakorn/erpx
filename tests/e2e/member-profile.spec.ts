import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { expect, test } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";

const tinyPng = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=", "base64");

test.beforeEach(async ({ context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname), "Uses the local fictional roster.");
  const login = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(login.status(), await login.text()).toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
});

test("another member's profile aligns the feed width with the profile card", async ({ page, context, browser, baseURL }, testInfo) => {
  const author = await browser.newContext(testInfo.project.use);
  try {
    const login = await author.request.post("/api/auth/sign-in/email", {
      data: { email: "noah@erpx.dev", password: "Noah-ERPX-2026!" },
    });
    expect(login.status(), await login.text()).toBe(200);
    // Noah's branch-only role derives Company from New York lineage.
    const origin = new URL(baseURL!).origin;
    const assignmentApi = "/api/v1/holdings/acme/branches/new-york/assignments/active";
    const currentRole = await author.request.get(assignmentApi);
    expect(currentRole.ok(), await currentRole.text()).toBe(true);
    const current = (await currentRole.json()).data;
    expect(current).toMatchObject({ companyId: "acme-co", userId: "usr_noah" });
    expect(current.options.some((option: { id: string }) => option.id === "seed_usr_noah")).toBe(true);
    const selectedRole = await author.request.post(assignmentApi, {
      headers: { Origin: origin },
      data: { assignmentId: "seed_usr_noah", revision: current.revision },
    });
    expect(selectedRole.ok(), await selectedRole.text()).toBe(true);
    const persistedRole = await author.request.get(assignmentApi);
    expect(persistedRole.ok(), await persistedRole.text()).toBe(true);
    expect((await persistedRole.json()).data).toMatchObject({
      valid: true, companyId: "acme-co", userId: "usr_noah", assignmentId: "seed_usr_noah",
    });
    const authorPage = await author.newPage();
    await authorPage.goto("/holdings/acme/branches/new-york/home");
    const workContext = authorPage.locator("[data-work-assignment]");
    await expect(workContext).toHaveCount(1);
    await expect(workContext).toHaveAttribute("data-work-stale", "false");
    await expect(workContext).toHaveAttribute("data-work-user", "usr_noah");
    await expect(workContext).toHaveAttribute("data-work-company", "acme-co");
    await expect(workContext).toHaveAttribute("data-work-assignment", "seed_usr_noah");
    const stamp = await workContext.evaluate((element) => {
      const node = element as HTMLElement;
      return { revision: node.dataset.workRevision!, company: node.dataset.workCompany!, assignment: node.dataset.workAssignment! };
    });
    expect(stamp.revision).toMatch(/^\d+$/);
    const commandHeaders = {
      Origin: origin,
      "x-erpx-context-revision": stamp.revision,
      "x-erpx-context-company": stamp.company,
      "x-erpx-context-assignment": stamp.assignment,
    };
    const api = "/api/v1/holdings/acme/branches/new-york/beta/drafts";
    const draft = await author.request.post(api, {
      headers: { ...commandHeaders, "Idempotency-Key": randomUUID() },
      data: { body: "Profile layout fixture: Noah's published update.", audience: { kind: "branch", id: "new-york" }, expectedVersion: 1 },
    });
    expect(draft.ok(), await draft.text()).toBe(true);
    const saved = (await draft.json()).data as { id: string; version: number };
    const publish = await author.request.post(`${api}/${saved.id}/publish`, {
      headers: { ...commandHeaders, "Idempotency-Key": randomUUID() },
      data: { expectedVersion: saved.version },
    });
    expect(publish.ok(), await publish.text()).toBe(true);
  } finally {
    await author.close();
  }
  if (testInfo.project.name === "desktop") await page.setViewportSize({ width: 1800, height: 1200 });
  await context.addCookies([{ name: "erpx_appearance", value: "dark", url: baseURL! }]);
  await page.goto("/holdings/acme/profiles/usr_noah");
  await expect(page.getByRole("heading", { name: "Noah Patel" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Time & display" })).toHaveCount(0);
  await expect(page.locator("time").first()).not.toHaveText("…");
  const layout = await page.evaluate(() => {
    const hero = document.querySelector("h1")?.closest("section")?.getBoundingClientRect();
    const feed = document.querySelector('nav[aria-label="Profile sections"]')?.getBoundingClientRect();
    const post = document.querySelector('nav[aria-label="Profile sections"]')?.nextElementSibling?.querySelector("article")?.getBoundingClientRect();
    if (!hero || !feed || !post) return null;
    return { widthGap: Math.abs(hero.width - feed.width), leftGap: Math.abs(hero.left - feed.left), postWidthGap: Math.abs(hero.width - post.width), postLeftGap: Math.abs(hero.left - post.left), overflow: document.documentElement.scrollWidth > innerWidth };
  });
  expect(layout).not.toBeNull();
  expect(layout!.widthGap).toBeLessThan(2);
  expect(layout!.leftGap).toBeLessThan(2);
  expect(layout!.postWidthGap).toBeLessThan(2);
  expect(layout!.postLeftGap).toBeLessThan(2);
  expect(layout!.overflow).toBe(false);
  await captureCaseScreenshot(page, `profile-viewer-${testInfo.project.name}-dark.png`);
});

test("profile opens from the account menu and respects the selected clock", async ({ page, context, baseURL }, testInfo) => {
  test.setTimeout(120_000);
  await context.addCookies([{ name: "erpx_appearance", value: "dark", url: baseURL! }]);
  await page.goto("/holdings/acme/home");
  await page.getByRole("button", { name: /Account menu for/ }).click();
  await page.getByRole("link", { name: "My profile" }).click();
  await expect(page.getByRole("heading", { name: /Mali/ })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Time & display" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Posts" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Posts", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Change cover" }).focus();
  await expect(page.getByRole("button", { name: "Change cover" })).toBeFocused();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await captureCaseScreenshot(page, `profile-${testInfo.project.name}-dark.png`);

  const originalResponse = await context.request.get("/api/v1/account/time-preference");
  expect(originalResponse.ok()).toBe(true);
  const original = (await originalResponse.json()).data as {
    mode: "system" | "manual"; timeZone: string | null; hourCycle: "h12" | "h23"; version: number;
  };
  let changed = false;
  try {
    const update = await context.request.patch("/api/v1/account/time-preference", {
      data: { mode: "manual", timeZone: "America/New_York", hourCycle: "h12", version: original.version },
    });
    expect(update.status(), await update.text()).toBe(200);
    changed = true;
    await page.reload();
    const settings = page.getByRole("region", { name: "Time and display settings" });
    await expect(settings.locator("select").nth(0)).toHaveValue("manual");
    await expect(settings.locator("input")).toHaveValue("America/New_York");
    await expect(settings.locator("select").nth(1)).toHaveValue("h12");
    await expect(settings.locator(".profile-time-preview strong")).toContainText(/UTC-0[45]:00/);
  } finally {
    if (changed) {
      const current = (await (await context.request.get("/api/v1/account/time-preference")).json()).data;
      const restore = await context.request.patch("/api/v1/account/time-preference", {
        data: { mode: original.mode, timeZone: original.timeZone, hourCycle: original.hourCycle, version: current.version },
      });
      expect(restore.ok(), await restore.text()).toBe(true);
    }
  }
  await context.addCookies([{ name: "erpx_appearance", value: "light", url: baseURL! }]);
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await captureCaseScreenshot(page, `profile-${testInfo.project.name}-light.png`);
});

test("generic time entry rejects a DST gap and requires an offset on a repeated hour", async ({ page, context }) => {
  test.setTimeout(120_000);
  const original = (await (await context.request.get("/api/v1/account/time-preference")).json()).data;
  let changed = false;
  try {
    const update = await context.request.patch("/api/v1/account/time-preference", {
      data: { mode: "manual", timeZone: "America/New_York", hourCycle: "h23", version: original.version },
    });
    expect(update.ok(), await update.text()).toBe(true);
    changed = true;
    await page.goto("/holdings/acme/staff-tasks/new");
    const due = page.getByLabel("Due date and time (optional)", { exact: true });
    await expect(due).toBeEnabled();
    await due.fill("2027-03-14T02:30");
    await expect(page.getByText("This local time does not exist in America/New_York.")).toBeVisible();
    await due.fill("2027-11-07T01:30");
    const offset = page.getByRole("combobox", { name: "Due date and time (optional) UTC offset" });
    await expect(offset).toBeVisible();
    await expect(offset.locator("option")).toHaveCount(3);
    await offset.selectOption({ label: "UTC-05:00" });
    await expect(page.locator('input[type="hidden"][name="dueAt"]')).toHaveValue("2027-11-07T06:30:00.000Z");
  } finally {
    if (changed) {
      const current = (await (await context.request.get("/api/v1/account/time-preference")).json()).data;
      const restore = await context.request.patch("/api/v1/account/time-preference", {
        data: { mode: original.mode, timeZone: original.timeZone, hourCycle: original.hourCycle, version: current.version },
      });
      expect(restore.ok(), await restore.text()).toBe(true);
    }
  }
});

test("profile media is scanned, served privately, and removable", async ({ context, page }) => {
  test.setTimeout(120_000);

  const api = "/api/v1/holdings/acme/profiles";
  const beforeResponse = await context.request.get(`${api}/usr_mali`);
  expect(beforeResponse.ok(), await beforeResponse.text()).toBe(true);
  const before = (await beforeResponse.json()).data;
  expect(before.avatarUrl, "Each isolated profile media case starts without an avatar.").toBeFalsy();
  await page.goto(`${api.replace("/api/v1", "")}/usr_mali`);
  await page.getByLabel("Cover image file").setInputFiles({
    name: `cover-${randomUUID()}.png`, mimeType: "image/png", buffer: tinyPng,
  });
  await expect(page.getByText("Previewing cover. Review the image above before saving.")).toBeVisible();
  await captureCaseScreenshot(page, "profile-cover-preview.png");
  await page.getByRole("button", { name: "Cancel image" }).click();
  await expect(page.getByRole("button", { name: "Save cover" })).toHaveCount(0);
  await page.getByLabel("Avatar image file").setInputFiles({
    name: `avatar-${randomUUID()}.png`, mimeType: "image/png", buffer: tinyPng,
  });
  await expect(page.getByText("Previewing avatar. Review the image above before saving.")).toBeVisible();
  await page.getByRole("button", { name: "Save avatar" }).click();
  await expect(page.getByRole("status")).toContainText("Avatar updated.");
  try {
    const image = await context.request.get(`${api}/usr_mali/media/avatar`);
    expect(image.status()).toBe(200);
    expect(image.headers()["content-type"]).toBe("image/webp");
    expect(image.headers()["cache-control"]).toContain("private");
  } finally {
    if (!(await page.getByRole("button", { name: "Remove avatar" }).isVisible())) {
      await page.getByRole("button", { name: "Edit profile" }).click();
    }
    await page.getByRole("button", { name: "Remove avatar" }).click();
    await expect(page.getByRole("status")).toContainText("Avatar removed.");
  }
  const after = await context.request.get(`${api}/usr_mali/media/avatar`);
  expect(after.status()).toBe(404);
});
