import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import { readFileSync, existsSync } from "node:fs";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
const fixturePath = process.env.ERPX_E2E_FIXTURE ?? ".scratch/messaging/fixture.json";
type Actor = { email: string; userId: string; assignmentId: string };
const f: {
  company: string;
  password: string;
  actors: Record<string, Actor>;
} | null = existsSync(fixturePath)
  ? JSON.parse(readFileSync(fixturePath, "utf8"))
  : null;
test.skip(
  !f,
  "Run bun --env-file=.env.local scripts/seed/seed-messaging-e2e.ts first.",
);
test.setTimeout(180000);
test.use({ actionTimeout: 20000 });
const root = "/api/v1/holdings/acme/messaging";
async function login(ctx: BrowserContext, who: string) {
  const response = await ctx.request.post("/api/auth/sign-in/email", {
    maxRedirects: 0,
    data: { email: f!.actors[who].email, password: f!.password },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  expect(response.headers()["content-type"]).toContain("application/json");
  await selectFixtureAssignment(ctx, f!.company, f!.actors[who].assignmentId);
}
async function post(page: Page, path: string, data: unknown) {
  const rendered = page.locator("[data-work-revision]");
  await expect(rendered).toHaveCount(1);
  await expect(rendered).toHaveAttribute("data-work-stale", "false");
  const company = await rendered.getAttribute("data-work-company");
  const assignment = await rendered.getAttribute("data-work-assignment");
  const revision = await rendered.getAttribute("data-work-revision");
  expect(company).toBe(f!.company);
  expect(assignment).toBe(f!.actors.alice.assignmentId);
  expect(revision).toMatch(/^\d+$/);
  const response = await page.request.post(`${root}/${path}`, {
    data,
    headers: {
      "Idempotency-Key": crypto.randomUUID(),
      Origin: new URL(page.url()).origin,
      "x-erpx-context-company": company!,
      "x-erpx-context-assignment": assignment!,
      "x-erpx-context-revision": revision!,
    },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).data;
}
test("real accounts chat, reconnect, attach files, use teams and enforce private membership", async ({
  browser,
  page,
  context,
}, testInfo) => {
  await login(context, "alice");
  const bob = await browser.newContext({
    baseURL: process.env.ERPX_E2E_BASE_URL,
    viewport: testInfo.project.use.viewport,
  });
  try {
    await login(bob, "bob");
    const b = await bob.newPage();
    await page.goto("/holdings/acme/inbox");
    await page.getByRole("button", { name: "New chat", exact: true }).click();
    await page.getByLabel("Messaging bob", { exact: false }).check();
    await page.getByRole("button", { name: "Start chat", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "Messaging bob", exact: true }),
    ).toBeVisible();
    const greeting = `Hello from Alice ${crypto.randomUUID()}`;
    await page.getByLabel("Message", { exact: true }).fill(greeting);
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      page.getByRole("log").getByText(greeting, { exact: true }),
    ).toBeVisible();
    await b.goto("/holdings/acme/inbox");
    await b.getByRole("button", { name: /Messaging alice/ }).click();
    await expect(
      b.getByRole("log").getByText(greeting, { exact: true }),
    ).toBeVisible();
    const reply = `Bob reply ${crypto.randomUUID()}`;
    await b.getByLabel("Message", { exact: true }).fill(reply);
    await b.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      page.getByRole("log").getByText(reply, { exact: true }),
    ).toBeVisible({
      timeout: 12000,
    });
    await context.setOffline(true);
    const recovery = `Offline retry ${crypto.randomUUID()}`;
    await page.getByLabel("Message", { exact: true }).fill(recovery);
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      page.getByRole("alert").filter({ hasText: "Your message is still here" }),
    ).toBeVisible();
    await context.setOffline(false);
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      b.getByRole("log").getByText(recovery, { exact: true }),
    ).toBeVisible({
      timeout: 12000,
    });
    await page.getByLabel("Attach files").setInputFiles({
      name: "acceptance.csv",
      mimeType: "text/csv",
      buffer: Buffer.from("name,value\nmessage,verified"),
    });
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      b.getByRole("link", { name: /acceptance.csv/ }).last(),
    ).toBeVisible({
      timeout: 12000,
    });
    const attachment = await b
      .getByRole("link", { name: /acceptance.csv/ })
      .last()
      .getAttribute("href");
    expect((await bob.request.get(attachment!)).ok()).toBeTruthy();
    await page.reload();
    await page.getByRole("button", { name: /Messaging bob/ }).click();
    await expect(
      page.getByRole("log").getByText(greeting, { exact: true }),
    ).toBeVisible();
    await captureCaseScreenshot(page, `${testInfo.project.name}-chat.png`, { fullPage: true });
    await page.getByRole("button", { name: "Teams", exact: true }).click();
    await page.getByRole("button", { name: "New team", exact: true }).click();
    const teamName = `Acceptance ${testInfo.project.name} ${Date.now()}`;
    await page.getByLabel("Name", { exact: true }).fill(teamName);
    await page.getByLabel("Messaging bob", { exact: false }).check();
    await page.getByLabel("Messaging carol", { exact: false }).check();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "General", exact: true }),
    ).toBeVisible();
    // Mobile returns to the team list before opening channel settings.
    const back = page.getByRole("button", { name: "Back to conversations" });
    if (await back.isVisible()) await back.click();
    await page
      .getByRole("button", { name: `Add channel to ${teamName}`, exact: true })
      .click();
    await page.getByLabel("Name", { exact: true }).fill("Private planning");
    await page
      .getByRole("combobox", { name: "Access", exact: true })
      .selectOption("private");
    await page.getByLabel("Messaging bob", { exact: false }).check();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: "Private planning", exact: true }),
    ).toBeVisible();
    await page
      .getByLabel("Message", { exact: true })
      .fill("Private channel evidence");
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      page
        .getByRole("log")
        .getByText("Private channel evidence", { exact: true }),
    ).toBeVisible();
    const overview = (await (await context.request.get(root)).json()).data;
    const team = overview.teams.find(
        (t: { name: string }) => t.name === teamName,
      ),
      channel = overview.rooms.find(
        (r: { team_id: string; private: boolean }) =>
          r.team_id === team.id && r.private,
      );
    const carol = await browser.newContext({
      baseURL: process.env.ERPX_E2E_BASE_URL,
    });
    try {
      await login(carol, "carol");
      expect(
        (
          await carol.request.get(`${root}/rooms/${channel.id}/messages`)
        ).status(),
      ).toBe(404);
    } finally {
      await carol.close();
    }
    expect(
      (await bob.request.get(`${root}/rooms/${channel.id}/messages`)).ok(),
    ).toBeTruthy();
    await captureCaseScreenshot(page, `${testInfo.project.name}-channel.png`, { fullPage: true });
    await post(page, `teams/${team.id}`, {
      name: teamName,
      members: [f!.actors.carol.userId],
      expectedVersion: 1,
    });
    expect(
      (await bob.request.get(`${root}/rooms/${channel.id}/messages`)).status(),
    ).toBe(404);
    await page.getByRole("button", { name: "Notifications", exact: true }).click();
    await expect.poll(() => new URL(page.url()).searchParams.get("tab")).toBe("updates");
    const readFilter = page.getByRole("combobox", { name: "Read filter", exact: true });
    await expect(readFilter).toHaveValue("unread");
    await readFilter.selectOption("all");
    await expect(readFilter).toHaveValue("all");
    await expect(page.getByRole("combobox", { name: "Category", exact: true })).toHaveValue("");
  } finally {
    await bob.close();
  }
});
