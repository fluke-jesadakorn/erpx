import { test, expect, type BrowserContext } from "@playwright/test";
import { randomUUID } from "node:crypto";
test.use({ actionTimeout: 20000 });
test.setTimeout(120000);
const base = "/holdings/acme/branches/bangkok";
async function login(context: BrowserContext, name: string) {
  const r = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: `${name}@erpx.dev`,
      password: `${name[0].toUpperCase() + name.slice(1)}-ERPX-2026!`,
    },
  });
  expect(r.status(), await r.text()).toBe(200);
}
test("composer removes the duplicate avatar and accepts pasted images", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    const response = await context.request.post("/api/auth/sign-in/email", {
      data: {
        email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
        password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
      },
    });
    expect(response.status(), await response.text()).toBe(200);
    const page = await context.newPage();
    await page.goto("/holdings/acme/home");
    await expect(
      page.getByRole("heading", { name: "Connect your staff profile" }),
    ).toBeVisible();
    await expect(
      page.getByText("Connect a staff profile to show your personal schedule."),
    ).toBeVisible();
    await expect(
      page.getByText(/Some information is unavailable/),
    ).toHaveCount(0);
    const composer = page.locator("details.beta-composer");
    await composer.getByText("Share an update", { exact: true }).click();
    await expect(composer).toHaveAttribute("open", "");
    await expect(composer.locator(".beta-person")).toHaveCount(0);
    const editor = composer.getByLabel("Post message", { exact: true });
    await expect(editor).toBeVisible();
    await editor.evaluate((element) => {
      const bytes = Uint8Array.from(
        atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aG20AAAAASUVORK5CYII="),
        (character) => character.charCodeAt(0),
      );
      const clipboard = new DataTransfer();
      clipboard.items.add(new File([bytes], "pasted.png", { type: "image/png" }));
      element.dispatchEvent(
        new ClipboardEvent("paste", { bubbles: true, clipboardData: clipboard }),
      );
    });
    await expect(composer.getByRole("img", { name: "pasted.png" })).toBeVisible();
    await expect(composer.getByRole("button", { name: "Remove pasted.png" })).toBeVisible();
    await expect(page.locator(".beta-status")).toContainText("Image attached.");
  } finally {
    await context.close();
  }
});
test("social post → manual task → evidence → independent acceptance, with AI fallback", async ({
  browser,
}, info) => {
  test.setTimeout(120000);
  const creator = await browser.newContext(info.project.use),
    assignee = await browser.newContext(info.project.use),
    reviewer = await browser.newContext(info.project.use),
    foreign = await browser.newContext(info.project.use);
  try {
    await Promise.all([
      login(creator, "mali"),
      login(assignee, "nisa"),
      login(reviewer, "ava"),
      login(foreign, "noah"),
    ]);
    const page = await creator.newPage(),
      title = `Social repair ${randomUUID().slice(0, 8)}`;
    await page.goto(`${base}/home`);
    await page.getByText("Share an update", { exact: true }).click();
    await page
      .getByLabel("Post message", { exact: true })
      .fill(`${title} · เครื่องชงกาแฟเสีย ช่วยตรวจสอบด้วย`);
    await page.getByLabel("Add photos", { exact: true }).setInputFiles({
      name: "equipment.png",
      mimeType: "image/png",
      buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aG20AAAAASUVORK5CYII=",
        "base64",
      ),
    });
    await page
      .getByRole("button", { name: "Share update", exact: true })
      .click();
    const article = page.getByRole("article").filter({ hasText: title });
    await expect(article).toBeVisible({ timeout: 30000 });
    await expect(
      article.getByRole("img", { name: "equipment.png" }),
    ).toBeVisible();
    const permalink = await article.locator("header a").getAttribute("href");
    expect(permalink).toContain("/posts/");
    const postId = permalink!.split("/").pop()!;
    const file = await article.locator(".beta-images a").getAttribute("href");
    expect((await foreign.request.get(file!)).status()).toBeGreaterThanOrEqual(
      403,
    );
    await article.getByRole("button", { name: /Comment/ }).click();
    await article
      .getByLabel("Write a comment", { exact: true })
      .fill("Please inspect the power switch.");
    await article.getByRole("button", { name: "Reply", exact: true }).click();
    await expect(
      article.getByText("Please inspect the power switch.", { exact: true }),
    ).toBeVisible();
    await article
      .getByRole("button", { name: "Create task", exact: true })
      .click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await page.route(`**/posts/${postId}/ai-drafts`, (route) =>
      route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          error: { message: "AI temporarily unavailable. Continue manually." },
        }),
      }),
    );
    await dialog.getByLabel("Task title", { exact: true }).fill(title);
    await dialog
      .getByRole("button", { name: "Let AI prepare this task", exact: true })
      .click();
    await expect(dialog.getByRole("status")).toContainText(
      "AI temporarily unavailable",
    );
    await expect(dialog.getByLabel("Task title", { exact: true })).toHaveValue(
      title,
    );
    await dialog
      .getByLabel("Responsible teammate", { exact: true })
      .selectOption("usr_nisa");
    await dialog
      .getByLabel("Independent completion reviewer", { exact: true })
      .selectOption("usr_ava");
    await dialog
      .getByLabel("Due date · your local time", { exact: true })
      .fill("2026-10-01T12:00");
    await dialog
      .getByRole("button", {
        name: "Confirm and create draft task",
        exact: true,
      })
      .click();
    await expect(
      page.getByRole("heading", { name: title, exact: true }),
    ).toBeVisible();
    const taskUrl = page.url(),
      taskId = taskUrl.split("/").pop()!;
    await expect(
      page.getByRole("link", {
        name: "Open source conversation ↗",
        exact: true,
      }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Submit task", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Submit task", exact: true }),
    ).toHaveCount(0);
    const work = await assignee.newPage();
    await work.goto(taskUrl);
    await work.getByRole("button", { name: "Start work", exact: true }).click();
    await expect(
      work.getByRole("button", { name: "Submit completion", exact: true }),
    ).toBeVisible();
    await work
      .getByLabel("Attach supporting file", { exact: true })
      .setInputFiles({
        name: "inspection.csv",
        mimeType: "text/csv",
        buffer: Buffer.from("check,result\npower,restored\n"),
      });
    await expect(
      work.getByRole("link", { name: /inspection.csv/ }),
    ).toBeVisible();
    await work
      .getByLabel("Completion or decision note", { exact: true })
      .fill("Power switch replaced and tested.");
    await work
      .getByRole("button", { name: "Submit completion", exact: true })
      .click();
    await expect(
      work.getByRole("button", { name: "Submit completion", exact: true }),
    ).toHaveCount(0);
    const review = await reviewer.newPage();
    await review.goto(taskUrl);
    await review
      .getByLabel("Completion or decision note", { exact: true })
      .fill("Inspected on site. Accepted.");
    await review
      .getByRole("button", { name: "Approve current step", exact: true })
      .click();
    await expect(
      review.getByRole("button", { name: "Approve current step", exact: true }),
    ).toHaveCount(0);
    const result = await creator.request.get(
      `/api/v1${base}/staff-tasks/${taskId}`,
    );
    expect((await result.json()).data.task.state).toBe("completed");
    await page.goto(permalink!);
    await expect(
      page.getByRole("link", { name: /Linked work · completed/ }),
    ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth + 1,
      ),
    ).toBe(true);
    await page.goto(`${base}/feed?post=${postId}`);
    await expect(page).toHaveURL(new RegExp(`/posts/${postId}$`));
  } finally {
    await Promise.all([
      creator.close(),
      assignee.close(),
      reviewer.close(),
      foreign.close(),
    ]);
  }
});
