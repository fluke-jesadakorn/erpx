import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import type { OrganizationOverview } from "../../src/lib/organization";
import { buildOrganizationGraph, visibleChartIds } from "../../src/lib/organization-chart";

const base = "/holdings/acme";
let signedIn: Awaited<ReturnType<BrowserContext["storageState"]>>;
let profileCount: number, initialCount: number, ceoReports: number;
test.beforeAll(async ({ browser, baseURL }) => {
  const context = await browser.newContext({ baseURL });
  const response = await context.request.post("/api/auth/sign-in/email", { data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" } });
  expect(response.ok()).toBe(true);
  await context.addCookies([
    { name: "erpx_company_h001", value: "acme-co", url: baseURL! },
    { name: "erpx_assignment_h001", value: "asg_mali_owner", url: baseURL! },
  ]);
  signedIn = await context.storageState();
  const organization = await context.request.get("/api/v1/holdings/acme/people/organization");
  expect(organization.ok()).toBe(true);
  const { data } = await organization.json() as { data: OrganizationOverview };
  const graph = buildOrganizationGraph(data);
  profileCount = data.people.length;
  initialCount = visibleChartIds(graph, new Set(graph.roots)).length;
  ceoReports = data.people.filter(p => p.manager_person_id === "hr-acme-co-001").length;
  await context.close();
});
test.beforeEach(async ({ context }) => { await context.addCookies(signedIn.cookies); });

async function fits(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
}
async function findPerson(page: Page, name: string) {
  await page.getByRole("textbox", { name: "Find staff in organization" }).fill(name);
  await page.getByRole("region", { name: "Staff search results" }).getByRole("button", { name: new RegExp(name) }).click();
}

for (const width of [360, 390, 768, 1024, 1440]) {
  test("reporting chart is readable and interactive at " + width + "px", async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== "desktop", "This test explicitly covers each responsive width.");
    const errors: string[] = [], writes: string[] = [];
    page.on("pageerror", e => errors.push(e.message));
    page.on("request", request => { if (/\/api\/v1\//.test(request.url()) && !["GET", "HEAD", "OPTIONS"].includes(request.method())) writes.push(request.method() + " " + request.url()); });
    await page.setViewportSize({ width, height: 900 });
    await page.goto(base + "/people/organization");
    const chart = page.getByRole("region", { name: "Organization reporting chart", exact: true });
    const mobile = width < 1024;
    await expect(chart.getByTestId(mobile ? "organization-team-view" : "organization-overview")).toBeVisible();
    await fits(page);
    if (mobile) {
      const selected = chart.getByRole("region", { name: "Selected team", exact: true });
      await expect(selected.getByText(`${ceoReports} reports`, { exact: true })).toBeVisible();
    }
    // Search must reveal a previously collapsed descendant and preserve the chosen person.
    await findPerson(page, "Sofia Ramirez");
    if (!mobile) await chart.getByRole("button", { name: "Team view", exact: true }).click();
    const reports = chart.getByRole("region", { name: "Direct reports", exact: true });
    await expect(reports.getByRole("article")).toHaveCount(6);
    await expect(chart.getByRole("region", { name: "Reporting manager" }).getByRole("article", { name: "Mali Srisai", exact: true })).toBeVisible();
    const selected = chart.getByRole("region", { name: "Selected team", exact: true });
    const detailsButton = selected.getByRole("button", { name: "View details for Sofia Ramirez" });
    await detailsButton.focus();
    await page.keyboard.press("Enter");
    const details = page.getByRole("dialog", { name: "Sofia Ramirez", exact: true });
    await expect(details).toBeVisible();
    await expect(details.getByRole("link", { name: "Open profile to edit" })).toHaveAttribute("href", base + "/people/hr-acme-co-008");
    const bounds = await details.boundingBox();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width + 1);
    if (mobile) expect(bounds!.y + bounds!.height).toBeCloseTo(900, 0);
    else expect(bounds!.width).toBeLessThanOrEqual(440);
    await page.keyboard.press("Escape");
    await expect(details).not.toBeVisible();
    await expect(detailsButton).toBeFocused();
    await chart.getByRole("button", { name: "Overview", exact: true }).click();
    const scope = mobile ? page.getByRole("dialog", { name: "Organization overview", exact: true }) : chart;
    await expect(scope.getByTestId("organization-overview")).toBeVisible();
    await scope.getByRole("button", { name: "Expand all", exact: true }).click();
    await expect(scope.getByText(`${profileCount} of ${profileCount} profiles shown`, { exact: false })).toBeVisible();
    // Large charts can round both zoom levels to the same percentage label.
    const viewport = scope.locator(".react-flow__viewport");
    const before = await viewport.getAttribute("style");
    await scope.getByRole("button", { name: "Zoom in", exact: true }).click();
    await expect(viewport).not.toHaveAttribute("style", before!);
    await scope.getByRole("button", { name: "Fit chart to screen" }).click();
    await scope.getByRole("button", { name: "Collapse", exact: true }).click();
    await expect(scope.getByText(`${initialCount} of ${profileCount} profiles shown`, { exact: false })).toBeVisible();
    await findPerson(page, "Daniel Kim");
    await expect(scope.getByLabel("Chart zoom", { exact: true })).toHaveText("100%");
    const revealed = scope.getByRole("article", { name: "Daniel Kim", exact: true });
    await expect(revealed).toBeVisible();
    await expect.poll(async () => {
      const card = await revealed.boundingBox();
      const canvas = await scope.locator(".react-flow").boundingBox();
      return Boolean(card && canvas && card.x >= canvas.x - 1 && card.y >= canvas.y - 1 && card.x + card.width <= canvas.x + canvas.width + 1 && card.y + card.height <= canvas.y + canvas.height + 1);
    }).toBe(true);
    await scope.getByRole("button", { name: mobile ? "Back to team view" : "Team view", exact: true }).click();
    await expect(chart.getByRole("region", { name: "Selected team", exact: true }).getByRole("article", { name: "Daniel Kim", exact: true })).toBeVisible();
    await expect(chart.getByText("No direct reports in this view.")).toBeVisible();
    await page.setViewportSize({ width: mobile ? 1100 : 390, height: 800 });
    await expect(chart.getByRole("region", { name: "Selected team", exact: true }).getByRole("article", { name: "Daniel Kim", exact: true })).toBeVisible();
    await fits(page);
    await page.getByRole("button", { name: "More navigation", exact: true }).filter({ visible: true }).click();
    const navigationDialog = page.getByRole("dialog", { name: "More", exact: true });
    await expect(navigationDialog).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(navigationDialog).not.toBeVisible();
    expect(writes).toEqual([]);
    expect(errors).toEqual([]);
  });
}

test("touch viewport, search and profile navigation preserve the team workflow", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "mobile", "Uses the touch-enabled mobile project.");
  await page.goto(base + "/people/organization");
  await findPerson(page, "Sofia Ramirez");
  await page.getByRole("region", { name: "Selected team" }).getByRole("button", { name: "View details for Sofia Ramirez" }).tap();
  await page.getByRole("dialog", { name: "Sofia Ramirez", exact: true }).getByRole("link", { name: "Open profile to edit" }).tap();
  await expect(page).toHaveURL(/\/people\/hr-acme-co-008$/);
  await page.goBack();
  await expect(page.getByRole("region", { name: "Organization reporting chart", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  const overview = page.getByRole("dialog", { name: "Organization overview", exact: true });
  const pane = overview.locator(".react-flow__pane");
  await expect(pane).toBeVisible();
  const area = (await pane.boundingBox())!;
  const viewport = overview.locator(".react-flow__viewport");
  const originalTransform = await viewport.getAttribute("style");
  const cdp = await page.context().newCDPSession(page);
  // Start on empty canvas; staff buttons deliberately do not initiate panning.
  const x = area.x + area.width / 2, y = area.y + 60;
  expect(await page.evaluate(({ x, y }) => !document.elementFromPoint(x, y)?.closest(".react-flow__node"), { x, y })).toBe(true);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y, id: 1 }] });
  for (let step = 1; step <= 4; step++) await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x: x + step * 12, y: y + step * 8, id: 1 }] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await expect(viewport).not.toHaveAttribute("style", originalTransform!);
  const zoom = overview.getByLabel("Chart zoom", { exact: true });
  const beforePinch = await zoom.innerText();
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: x - 25, y, id: 1 }, { x: x + 25, y, id: 2 }] });
  for (let step = 1; step <= 4; step++) await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x: x - 25 - step * 12, y, id: 1 }, { x: x + 25 + step * 12, y, id: 2 }] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await expect(zoom).not.toHaveText(beforePinch);
  await cdp.detach();
  await findPerson(page, "Sofia Ramirez");
  await page.setViewportSize({ width: 844, height: 390 });
  const selectedCard = overview.getByRole("article", { name: "Sofia Ramirez", exact: true });
  await expect.poll(async () => {
    const card = await selectedCard.boundingBox(), canvas = await pane.boundingBox();
    return Boolean(card && canvas && card.x >= canvas.x - 1 && card.y >= canvas.y - 1 && card.x + card.width <= canvas.x + canvas.width + 1 && card.y + card.height <= canvas.y + canvas.height + 1);
  }).toBe(true);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(zoom).toHaveText("100%");
  await overview.getByRole("button", { name: "Back to team view", exact: true }).tap();
  await expect(overview).not.toBeVisible();
  await fits(page);
});
