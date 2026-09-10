import { test, expect, type Page } from "@playwright/test";
import { getPlatformProxy } from "wrangler";
import { randomUUID } from "node:crypto";
test.use({ actionTimeout: 10000 });
async function signIn(page: Page, email: string, password: string) {
  await page.goto("/login");
  await page.getByLabel("Work email").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
}
test("local draft review, failed request retry, evidence and independent approval", async ({
  browser,
}, testInfo) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(testInfo.project.use.baseURL as string).hostname,
    ),
    "Fixture creation is local only.",
  );
  test.setTimeout(90000);
  const employee = await browser.newContext(testInfo.project.use);
  const manager = await browser.newContext(testInfo.project.use);
  const humanResources = await browser.newContext(testInfo.project.use);
  const fixtureIds: string[] = [];
  let id = "";
  try {
    const page = await employee.newPage();
    await signIn(page, "mali@erpx.dev", "Mali-ERPX-2026!");
    for (let index = 0; index < 6; index++) {
      const response = await employee.request.post(
        "/api/v1/holdings/acme/branches/bangkok/leave-requests",
        {
          headers: {
            "Idempotency-Key": randomUUID(),
            Origin: new URL(testInfo.project.use.baseURL as string).origin,
          },
          data: {
            startDate: "2026-09-14",
            days: 2,
            handoverNote: "Isolated UI verification request",
          },
        },
      );
      expect(response.ok()).toBeTruthy();
      fixtureIds.push((await response.json()).data.id);
    }
    id = fixtureIds[0];
    const number = `LR-${id.slice("leave-".length)}`;
    await page.goto(
      "/holdings/acme/branches/bangkok/work?queue=todo&type=leave",
    );
    const count = page
      .getByRole("navigation", { name: "Work queues" })
      .getByRole("link", { name: /To do/ })
      .locator("span");
    expect(Number(await count.innerText())).toBeGreaterThanOrEqual(
      fixtureIds.length,
    );
    await page.goto(
      `/holdings/acme/branches/bangkok/work?queue=todo&q=${number}`,
    );
    await page.getByRole("link", { name: new RegExp(number) }).click();
    await expect(page).toHaveURL(new RegExp(`/tasks/people_payroll/${id}`));
    await page.goBack();
    await expect(page.getByLabel("Search work", { exact: true })).toHaveValue(
      number,
    );
    await page.goto(
      `/holdings/acme/branches/bangkok/tasks/people_payroll/${id}`,
    );
    const handover = "The receiving team will cover my morning deliveries.";
    await page
      .getByRole("textbox", { name: "Handover note", exact: true })
      .fill(handover);
    await page
      .getByLabel("Supporting evidence", { exact: true })
      .setInputFiles({
        name: "handover.csv",
        mimeType: "text/csv",
        buffer: Buffer.from("task,owner\nreceiving,team\n"),
      });
    await expect(
      page.getByText("handover.csv · Uploaded and verified"),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Review request", exact: true })
      .click();
    await expect(
      page.getByRole("dialog", { name: "Submit leave request" }),
    ).toBeVisible();
    await expect(
      page.getByRole("dialog").getByText(handover, { exact: true }),
    ).toBeVisible();
    const attempts: string[] = [];
    let fail = true;
    await page.route(
      `**/api/v1/holdings/acme/branches/bangkok/leave-requests/${id}`,
      async (route) => {
        if (route.request().method() !== "PATCH") return route.continue();
        attempts.push(route.request().headers()["idempotency-key"]);
        if (fail) {
          fail = false;
          await route.abort("failed");
        } else await route.continue();
      },
    );
    await page
      .getByRole("button", { name: "Submit leave request", exact: true })
      .click();
    await expect(page.getByText(/connection was interrupted/)).toBeVisible();
    await page
      .getByRole("button", { name: "Submit leave request", exact: true })
      .click();
    await expect(
      page.getByText(
        "Request submitted. It’s ready for an independent review.",
      ),
    ).toBeVisible();
    expect(attempts).toHaveLength(2);
    expect(attempts[0]).toBe(attempts[1]);
    await expect(
      page.getByRole("link", { name: /handover.csv/ }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Review approval" }),
    ).not.toBeVisible();
    const review = await manager.newPage();
    await signIn(review, "nisa@erpx.dev", "Nisa-ERPX-2026!");
    await review.goto(`/holdings/acme/tasks/people_payroll/${id}`);
    await expect(review.getByText(handover, { exact: true })).toBeVisible();
    await expect(
      review.getByRole("link", { name: /handover.csv/ }),
    ).toBeVisible();
    await review
      .getByLabel("Decision note", { exact: true })
      .first()
      .fill("Handover reviewed.");
    await review
      .getByRole("button", { name: "Review approval", exact: true })
      .click();
    await review
      .getByRole("button", { name: "Approve leave", exact: true })
      .click();
    await expect(
      review.getByText("Approval recorded. The current stage is shown above."),
    ).toBeVisible();
    await page.reload();
    await expect(
      page.getByText("Current approver: Ava Williams"),
    ).toBeVisible();
    const hr = await humanResources.newPage();
    await hr.goto("/login");
    await hr.getByLabel("Work email").fill("ava@erpx.dev");
    await hr.getByLabel("Password", { exact: true }).fill("Ava-ERPX-2026!");
    await hr.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(hr).toHaveURL(/\/holdings\/acme/);
    await hr.goto(`/holdings/acme/branches/bangkok/tasks/people_payroll/${id}`);
    await hr
      .getByRole("button", { name: "Review approval", exact: true })
      .click();
    await hr
      .getByRole("button", { name: "Approve leave", exact: true })
      .click();
    await expect(hr.getByText("This request is approved.")).toBeVisible();
    await page.reload();
    await expect(page.getByText("This request is approved.")).toBeVisible();
    await page.getByText("Approval route and history", { exact: true }).click();
    await expect(
      page.getByText("Handover reviewed.", { exact: true }).first(),
    ).toBeVisible();
  } finally {
    await employee.close().catch(() => {});
    await manager.close().catch(() => {});
    await humanResources.close().catch(() => {});
    const platform = await getPlatformProxy<CloudflareEnv>({
      configPath: "wrangler.jsonc",
      persist: true,
      remoteBindings: false,
    });
    const db = platform.env.H001_DB;
    const keys = (
      await db
        .prepare(
          "SELECT staging_key,final_key FROM upload_intents WHERE owner_type=? AND owner_id=?",
        )
        .bind("leave_request", id)
        .all<{ staging_key: string; final_key: string | null }>()
    ).results;
    for (const row of keys) {
      await platform.env.H001_FILES.delete(row.staging_key);
      if (row.final_key) await platform.env.H001_FILES.delete(row.final_key);
    }
    // Delete only this test's unique records and evidence; preserve all existing fixtures.
    const cleanup = [];
    for (const fixtureId of fixtureIds) {
      cleanup.push(
        db
          .prepare(
            "DELETE FROM approval_decisions WHERE instance_id IN (SELECT id FROM approval_instances WHERE record_id=?)",
          )
          .bind(fixtureId),
      );
      cleanup.push(
        db
          .prepare(
            "DELETE FROM approval_steps WHERE instance_id IN (SELECT id FROM approval_instances WHERE record_id=?)",
          )
          .bind(fixtureId),
      );
      cleanup.push(
        db
          .prepare("DELETE FROM approval_instances WHERE record_id=?")
          .bind(fixtureId),
      );
      for (const [table, column] of [
        ["staff_notifications", "record_id"],
        ["attachments", "owner_id"],
        ["upload_intents", "owner_id"],
        ["decision_evidence", "record_id"],
        ["audit_events", "record_id"],
        ["outbox_events", "aggregate_id"],
        ["command_receipts", "record_id"],
        ["leave_requests", "id"],
      ])
        cleanup.push(
          db.prepare(`DELETE FROM ${table} WHERE ${column}=?`).bind(fixtureId),
        );
    }
    try {
      for (let attempt = 0; attempt < 5; attempt++) {
        try {
          await db.batch(cleanup);
          break;
        } catch (error) {
          if (
            attempt === 4 ||
            !/SQLITE_BUSY|database is locked/.test(String(error))
          )
            throw error;
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
      }
    } finally {
      await platform.dispose();
    }
  }
});
