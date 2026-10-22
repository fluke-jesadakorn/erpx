import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";

import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import type { WorkPageSummary } from "@erpx/server-work_projection/public/work-summary";

const base = "/holdings/acme";
const api = `/api/v1${base}`;

// These cases use the documented development roster and retain cancelled
// acceptance records. Never implicitly use those accounts on another host.
test.beforeEach(async ({ context, baseURL }) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname),
    "Work journeys require the local development roster.",
  );
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: process.env.ERPX_E2E_EMAIL ?? "mali@erpx.dev", password: process.env.ERPX_E2E_PASSWORD ?? "Mali-ERPX-2026!" },
  });
  expect(response.status(), "Seeded Mali account must be available").toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
});

async function refreshAfterTabReturn(page: Page) {
  const response = page.waitForResponse(r => new URL(r.url()).pathname === `${api}/work/summary` && r.request().method() === "GET");
  await expect(page.getByRole("button", { name: "New", exact: true })).toBeEnabled();
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  return await response;
}

test("WORK-001 filters survive queue changes and reload, then clear", async ({ page }, info) => {
  await page.goto(`${base}/work?type=service_request`);
  const workFilters = page.locator("#workspace-content form").filter({ has: page.getByLabel("Search work", { exact: true }) });
  await expect(workFilters).toHaveCount(1);
  await workFilters.getByLabel("Responsibility", { exact: true }).selectOption("requested");
  await workFilters.getByLabel("Work type", { exact: true }).selectOption("all");
  await expect(workFilters.getByLabel("Work type", { exact: true })).toHaveValue("all");
  await workFilters.getByLabel("Work type", { exact: true }).selectOption("service_request");
  const search = `No such request ${randomUUID()}`;
  await workFilters.getByLabel("Search work", { exact: true }).fill(search);
  await workFilters.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByRole("region", { name: "Work results" })).toContainText("No matching work");
  if (info.project.name === "mobile") await page.getByRole("navigation", { name: "Mobile navigation", exact: true }).getByRole("link", { name: /My work/ }).click();
  const views = info.project.name === "mobile" ? page.getByRole("dialog", { name: "My work views", exact: true }) : page.locator('[aria-label="My work views"]');
  await views.getByRole("link", { name: "Tracking", exact: true }).click();
  const trackingFilters = page.getByRole("navigation", { name: "Tracking filters", exact: true });
  await trackingFilters.getByRole("link", { name: "Waiting", exact: true }).click();
  await expect(page).toHaveURL(url => url.searchParams.get("view") === "tracking" && url.searchParams.get("tracking") === "waiting");
  await expect(workFilters.getByLabel("Search work", { exact: true })).toHaveValue(search);
  await expect(workFilters.getByLabel("Work type", { exact: true })).toHaveValue("service_request");
  // A new named view resets responsibility. Refine within that view, then
  // verify the full filter set survives reload and Clear preserves Waiting.
  await expect(workFilters.getByLabel("Responsibility", { exact: true })).toHaveValue("all");
  await workFilters.getByLabel("Responsibility", { exact: true }).selectOption("requested");
  await workFilters.getByRole("button", { name: "Search", exact: true }).click();
  await page.reload();
  await expect(workFilters).toHaveCount(1);
  await expect(workFilters.getByLabel("Search work", { exact: true })).toHaveValue(search);
  await expect(workFilters.getByLabel("Work type", { exact: true })).toHaveValue("service_request");
  await expect(workFilters.getByLabel("Responsibility", { exact: true })).toHaveValue("requested");
  await expect(trackingFilters.getByRole("link", { name: "Waiting", exact: true })).toHaveAttribute("aria-current", "page");
  await workFilters.getByRole("link", { name: "Clear filters", exact: true }).click();
  await expect(page).toHaveURL(`${base}/work?view=tracking&tracking=waiting`);
  await expect(workFilters.getByLabel("Search work", { exact: true })).toHaveValue("");
  await expect(workFilters.getByLabel("Work type", { exact: true })).toHaveValue("all");
  await expect(workFilters.getByLabel("Responsibility", { exact: true })).toHaveValue("all");
});

for (const entry of [
  { id: "WORK-002", path: "/requests", type: "service_request" },
  { id: "WORK-003", path: "/staff-tasks?queue=completed&q=acceptance", type: "staff_task" },
]) {
  test(`${entry.id} legacy ${entry.path} opens the corresponding Work list`, async ({ page }) => {
    await page.goto(`${base}${entry.path}`);
    await expect(page).toHaveURL(new RegExp(`/work\\?.*type=${entry.type}`));
    await expect(page.getByRole("heading", { name: "My work", exact: true })).toBeVisible();
    await expect(page.getByLabel("Work type", { exact: true })).toHaveValue(entry.type);
    if (entry.type === "staff_task") {
      await expect(page).toHaveURL(/queue=completed/);
      await expect(page.getByLabel("Search work", { exact: true })).toHaveValue("acceptance");
    }
  });
}

test("WORK-004 New work closes with Escape and restores keyboard focus", async ({ page }) => {
  await page.goto(`${base}/work`);
  const trigger = page.getByRole("button", { name: "New", exact: true });
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: "New work" });
  await expect(dialog.getByRole("link", { name: "Service request", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(trigger).toBeFocused();
});

test("WORK-005 failed summary refresh keeps filters and recovers through Retry work", async ({ page, context }) => {
  const query = "queue=waiting&type=service_request&q=recovery";
  const response = await context.request.get(`${api}/work/summary?${query}&paged=1`);
  expect(response.ok(), await response.text()).toBe(true);
  const snapshot = (await response.json() as { data: WorkPageSummary }).data;
  let failure = false;
  await page.route(`**${api}/work/summary**`, route => failure
    ? route.fulfill({ status: 503, json: { error: { message: "E2E temporary summary failure" } } })
    : route.fulfill({ json: { data: { ...snapshot, unavailableSources: ["Service requests"] } } }));
  await page.goto(`${base}/work?${query}`);
  await expect(page.getByRole("heading", { name: "My work", exact: true })).toBeVisible();
  expect((await refreshAfterTabReturn(page)).ok()).toBe(true);
  await expect(page.locator("main").getByRole("button", { name: "Retry", exact: true })).toBeVisible();
  failure = true;
  expect((await refreshAfterTabReturn(page)).status()).toBe(503);
  await expect(page.getByRole("status").filter({ hasText: "E2E temporary summary failure" })).toBeVisible();
  await expect(page.getByLabel("Search work", { exact: true })).toHaveValue("recovery");
  await expect(page.getByRole("region", { name: "Work results" })).toBeVisible();
  await page.unroute(`**${api}/work/summary**`);
  const recovered = page.waitForResponse(r => new URL(r.url()).pathname === `${api}/work/summary` && r.status() === 200);
  await page.locator("main").getByRole("button", { name: "Retry", exact: true }).click(); await recovered;
  await expect(page.locator("main").getByRole("button", { name: "Retry", exact: true })).toHaveCount(0);
  await expect(page.getByRole("status").filter({ hasText: "E2E temporary summary failure" })).toHaveCount(0);
  await expect(page.getByLabel("Search work", { exact: true })).toHaveValue("recovery");
  await expect(page.getByLabel("Work type", { exact: true })).toHaveValue("service_request");
});

test("REQ-001 request draft survives reload and failed save, retries once, then cancels", async ({ page, context }) => {
  test.setTimeout(120_000);
  const title = `E2E request ${randomUUID()}`;
  let id: string | undefined;
  const keys: string[] = [];
  try {
    await page.goto(`${base}/work?queue=all&type=service_request&q=${encodeURIComponent(title)}`);
    await page.getByRole("button", { name: "New", exact: true }).click();
    await page.getByRole("dialog", { name: "New work" }).getByRole("link", { name: "Service request", exact: true }).click();
    await page.getByRole("textbox", { name: "Title", exact: true }).fill(title);
    await page.getByRole("textbox", { name: "Describe the issue", exact: true }).fill("Development acceptance: recover a draft without submitting to a team.");
    await page.reload();
    await page.getByRole("button", { name: "New", exact: true }).click();
    await page.getByRole("dialog", { name: "New work" }).getByRole("link", { name: "Service request", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "Service request", exact: true })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Title", exact: true })).toHaveValue(title);
    await expect(page.getByRole("textbox", { name: "Describe the issue", exact: true })).toHaveValue("Development acceptance: recover a draft without submitting to a team.");

    await page.route(`**${api}/requests`, async (route) => {
      if (route.request().method() !== "POST") return route.continue();
      keys.push(route.request().headers()["idempotency-key"]);
      if (keys.length === 1) return route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({ error: { message: "E2E temporary save failure" } }),
      });
      await route.continue();
    });
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    await expect(page.getByRole("alert").filter({ hasText: "E2E temporary save failure" })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Title", exact: true })).toHaveValue(title);
    const savedResponse = page.waitForResponse((r) => r.url().endsWith(`${api}/requests`) && r.request().method() === "POST");
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    const saved = await savedResponse;
    expect(saved.ok()).toBe(true);
    id = (await saved.json()).data.id;
    expect(id).toBeTruthy();
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBeTruthy();
    expect(keys[1]).toBe(keys[0]);
    await expect(page.getByRole("heading", { name: title, exact: true })).toBeVisible();
    const persisted = await context.request.get(`${api}/requests/${id}`);
    expect(persisted.status()).toBe(200);
    expect((await persisted.json()).data.request).toMatchObject({ title, state: "draft" });
    await page.getByRole("link", { name: "← Back to Work", exact: true }).click();
    await expect(page.getByLabel("Search work", { exact: true })).toHaveValue(title);
    const row = page.getByRole("region", { name: "Work results" }).getByRole("link", { name: new RegExp(title) });
    await expect(row).toHaveCount(1);
    await row.click();
    await page.getByRole("button", { name: "Cancel draft", exact: true }).click();
    await expect(page.getByText("Cancelled", { exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("button", { name: "Save draft", exact: true })).toHaveCount(0);
    const cancelled = await context.request.get(`${api}/requests/${id}`);
    expect(cancelled.status()).toBe(200);
    expect((await cancelled.json()).data.request.state).toBe("cancelled");
  } finally {
    // Preserve history, but don't leave an actionable request if an assertion fails.
    if (id) {
      const response = await context.request.get(`${api}/requests/${id}`);
      expect(response.status(), "Read acceptance record for cleanup").toBe(200);
      const row = (await response.json()).data.request;
      if (row.state === "draft") {
        const selected = await context.request.get(`${api}/assignments/active`);
        expect(selected.ok(), await selected.text()).toBe(true);
        const active = (await selected.json()).data;
        const cleanup = await context.request.post(`${api}/requests/${id}/actions`, {
          headers: { Origin: new URL(page.url()).origin, "Idempotency-Key": randomUUID(), "x-erpx-context-company": active.companyId, "x-erpx-context-assignment": active.assignmentId, "x-erpx-context-revision": String(active.revision) },
          data: { action: "cancel", expectedVersion: row.version },
        });
        expect(cleanup.ok(), "Cancel remaining acceptance draft").toBe(true);
      }
    }
  }
});
