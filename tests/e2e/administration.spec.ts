import { test, expect } from "@playwright/test";
import { randomUUID } from "node:crypto";

test.use({ actionTimeout: 15000 });

test("approval rerouting has one authorized editor and handles a changed version", async ({
  page,
}, info) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(info.project.use.baseURL as string).hostname,
    ),
    "Creates isolated local work.",
  );
  test.setTimeout(90000);
  const base = "/holdings/acme";
  await page.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  async function command(path: string, data: unknown) {
    const r = await page.request.post(`/api/v1${base}/${path}`, {
      data,
      headers: { "Idempotency-Key": randomUUID() },
    });
    expect(r.ok(), await r.text()).toBeTruthy();
    return (await r.json()).data;
  }
  const task = await command("staff-tasks", {
    title: `Routing test ${randomUUID().slice(0, 8)}`,
    description: "Verify the centralized approval editor.",
    assigneeUserId: "usr_nisa",
    dueAt: "2030-01-01T12:00:00Z",
    projectId: null,
    startApprovers: ["usr_ava"],
    completionApprovers: ["usr_ava"],
  });
  try {
    await command(`staff-tasks/${task.id}/transition`, {
      action: "submit",
      expectedVersion: 1,
    });
    await page.goto(`${base}/staff-tasks/${task.id}`);
    await expect(
      page.getByText("Administratively reroute current step", { exact: true }),
    ).toHaveCount(0);
    await page.getByRole("link", { name: "Resolve approval issue" }).click();
    await expect(page).toHaveURL(
      new RegExp(`section=approvals&kind=task&record=${task.id}`),
    );
    await page
      .getByText("Administratively reroute current step", { exact: true })
      .click();
    await page
      .getByRole("combobox", { name: "Replacement approver", exact: true })
      .selectOption("usr_noah");
    await page
      .getByLabel("Rerouting reason")
      .fill("Cover the unavailable reviewer.");
    const current = (
      await (
        await page.request.get(`/api/v1${base}/staff-tasks/${task.id}`)
      ).json()
    ).data;
    await command(`approvals/${current.approval.instance.id}/reroute`, {
      expectedVersion: current.task.version,
      instanceVersion: current.approval.instance.version,
      approverUserId: "usr_ava",
      reason: "Concurrent configuration review.",
    });
    await page
      .getByRole("button", { name: "Record rerouting", exact: true })
      .click();
    await expect(page.locator("main").getByRole("alert")).toBeVisible();
    await expect(page.getByLabel("Rerouting reason")).toHaveValue(
      "Cover the unavailable reviewer.",
    );
    await page.reload();
    await page
      .getByText("Administratively reroute current step", { exact: true })
      .click();
    await page
      .getByRole("combobox", { name: "Replacement approver", exact: true })
      .selectOption("usr_noah");
    await page
      .getByLabel("Rerouting reason")
      .fill("Cover the unavailable reviewer.");
    await page
      .getByRole("button", { name: "Record rerouting", exact: true })
      .click();
    await expect(
      page.getByText("Waiting for Noah Patel", { exact: true }),
    ).toBeVisible();
    await page.goto(`${base}/staff-tasks/${task.id}`);
    await expect(
      page.getByText("Current approver: Noah Patel", { exact: true }),
    ).toBeVisible();
    await page.getByText("Approval route and history", { exact: true }).click();
    await expect(
      page.getByText("Cover the unavailable reviewer.", { exact: true }),
    ).toBeVisible();
  } finally {
    const current = (
      await (
        await page.request.get(`/api/v1${base}/staff-tasks/${task.id}`)
      ).json()
    ).data.task;
    if (current.state !== "cancelled")
      await command(`staff-tasks/${task.id}/transition`, {
        action: "cancel",
        expectedVersion: current.version,
        note: "UI verification completed.",
      });
  }
});

test("a profile owns level, activation and distinct access commands", async ({
  page,
}, info) => {
  test.skip(
    !["localhost", "127.0.0.1"].includes(
      new URL(info.project.use.baseURL as string).hostname,
    ),
    "Uses an isolated local employee.",
  );
  test.setTimeout(180000);
  const base = "/holdings/acme",
    suffix = randomUUID().slice(0, 8);
  await page.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  await page.goto(`${base}/home`);
  await page.getByRole("button", { name: "Switch workspace" }).click();
  await expect(
    page
      .getByRole("dialog", { name: "Switch workspace", exact: true })
      .getByRole("button", { name: /Acme Corporation/ }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("dialog", { name: "Switch workspace", exact: true }),
  ).not.toBeVisible();
  async function command(path: string, data: unknown) {
    const response = await page.request.post(`/api/v1${base}/${path}`, {
      data,
      headers: { "Idempotency-Key": randomUUID() },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return (await response.json()).data;
  }
  const levels = [];
  for (const rank of [1, 2])
    levels.push(
      await command("staff/levels", {
        code: `UX_${suffix}_${rank}`,
        name: `UX level ${suffix} ${rank}`,
        rank,
        expectedVersion: 0,
      }),
    );
  const options = (
    await (await page.request.get(`/api/v1${base}/staff/settings`)).json()
  ).data;
  const branchId = options.people[0].branch_id;
  const person = await command("people/directory", {
    legalName: `UX employee ${suffix}`,
    email: `ux-${suffix}@example.test`,
    countryCode: "TH",
    branchId,
    jobTitle: "UX test operator",
    workerType: "employee",
    employmentType: "salaried",
    startDate: "2026-09-01",
    currency: "THB",
    levelId: levels[0].id,
  });
  const profile = `${base}/people/${person.id}`;
  try {
    await page.goto(profile);
    await page.getByText("Change staff level", { exact: true }).click();
    await page
      .getByRole("combobox", { name: "New level", exact: true })
      .selectOption(levels[1].id);
    const reason = `Development review ${suffix}`;
    await page.getByLabel("Change reason").fill(reason);
    const keys: string[] = [];
    await page.route("**/staff/engagements/*/level", (route) => {
      keys.push(route.request().headers()["idempotency-key"]);
      return keys.length === 1
        ? route.fulfill({
            status: 503,
            contentType: "application/json",
            body: JSON.stringify({ error: { message: "Retry level change" } }),
          })
        : route.continue();
    });
    await page.getByRole("button", { name: "Record level change" }).click();
    await expect(page.locator("main").getByRole("alert")).toHaveText(
      "Retry level change",
    );
    await expect(page.getByLabel("Change reason")).toHaveValue(reason);
    await page.getByRole("button", { name: "Record level change" }).click();
    await expect(
      page.getByRole("definition").filter({ hasText: `UX level ${suffix} 2` }),
    ).toBeVisible();
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
    await page.getByText("Level history", { exact: true }).click();
    await expect(
      page.locator("details").filter({ hasText: "Level history" }),
    ).toContainText(reason);
    await page
      .getByRole("button", { name: "Activate employee", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Deactivate employee", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("link", { name: "Login & access", exact: true })
      .click();
    await page.getByText("Invite to ERPX", { exact: true }).click();
    await page.getByLabel("System role").selectOption("employee");
    await page.getByLabel("Access expires").fill("2030-01-01");
    await page.getByRole("button", { name: "Create activation link" }).click();
    const link = page.getByLabel("Activation link (expires in 48 hours)");
    await expect(link).toHaveValue(/\/activate\?token=/);
    const token = new URL(await link.inputValue()).searchParams.get("token");
    const activation = await page.request.post("/api/v1/account-invitations", {
      data: { token, password: `UX-activation-${suffix}!` },
    });
    expect(activation.ok(), await activation.text()).toBeTruthy();
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(1);
    await page
      .getByRole("button", { name: "Revoke role", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    async function grant() {
      await page.getByText("Grant role", { exact: true }).first().click();
      await page.getByLabel("System role").selectOption("employee");
      await page.getByLabel("Access expires").fill("2030-01-01");
      await page
        .getByRole("button", { name: "Grant role", exact: true })
        .click();
      await expect(
        page.getByRole("button", { name: "Revoke role", exact: true }),
      ).toHaveCount(1);
    }
    await grant();
    await page.getByText("Company access", { exact: true }).click();
    await page
      .getByRole("button", { name: "Revoke company access", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    await page.reload();
    await grant();
    await page.goto(profile);
    await page
      .getByRole("button", { name: "Deactivate employee", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Activate employee", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Activate employee", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Deactivate employee", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("link", { name: "Login & access", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Revoke role", exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByText("Login account linked.", { exact: false }),
    ).toBeVisible();
  } finally {
    const current = (
      await (
        await page.request.get(`/api/v1${base}/people/people/${person.id}`)
      ).json()
    ).data.person;
    await command(`people/directory/${person.id}/status`, {
      status: "archived",
      expectedVersion: Number(current.version),
    });
  }
});
