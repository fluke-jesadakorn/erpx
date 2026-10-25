import {
  test,
  expect,
  type BrowserContext,
  type Locator,
  type Page,
} from "@playwright/test";
import { randomUUID } from "node:crypto";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
test.use({ actionTimeout: 20000 });
test.setTimeout(120000);
const base = "/holdings/acme/branches/bangkok";
const tinyPng = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
  "base64",
);
async function login(context: BrowserContext, name: "mali" | "nisa" | "ava" | "noah") {
  const r = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: `${name}@erpx.dev`,
      password: `${name[0].toUpperCase() + name.slice(1)}-ERPX-2026!`,
    },
  });
  expect(r.status(), await r.text()).toBe(200);
  if (name === "ava" || name === "noah") {
    // Local roles derive Company from the branch; no holding Company grant is needed.
    const branch = name === "ava" ? "bangkok" : "new-york";
    const assignmentId = `seed_usr_${name}`;
    const endpoint = `/api/v1/holdings/acme/branches/${branch}/assignments/active`;
    const current = await context.request.get(endpoint);
    expect(current.ok(), `Read ${branch} role: ${current.status()}`).toBe(true);
    const state = (await current.json()).data;
    expect(state).toMatchObject({ companyId: "acme-co", userId: `usr_${name}` });
    expect(Number.isInteger(state.revision) && state.revision >= 0).toBe(true);
    expect(state.options.filter((option: { id: string }) => option.id === assignmentId)).toEqual([
      expect.objectContaining({ id: assignmentId, companyId: "acme-co", role: name === "ava" ? "hr_admin" : "employee", scopeKind: "branch", scopeNodeId: branch }),
    ]);
    const selected = await context.request.post(endpoint, {
      headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102" },
      data: { assignmentId, revision: state.revision },
    });
    expect(selected.ok(), `Select ${branch} role: ${selected.status()}`).toBe(true);
    const selection = (await selected.json()).data;
    expect(selection).toMatchObject({ userId: `usr_${name}`, companyId: "acme-co", assignmentId, appliesToWorkspace: true });
    const persisted = await context.request.get(endpoint);
    expect(persisted.ok(), `Verify ${branch} role: ${persisted.status()}`).toBe(true);
    expect((await persisted.json()).data).toMatchObject({ valid: true, userId: `usr_${name}`, companyId: "acme-co", assignmentId, revision: selection.revision });
  } else {
    await selectFixtureAssignment(context, "acme-co", name === "nisa" ? "workflow_controller" : "seed_usr_mali");
  }
}
async function pasteImages(editor: Locator, names: string[], caret: number) {
  await editor.focus();
  await editor.press("ControlOrMeta+Home");
  for (let offset = 0; offset < caret; offset += 1) await editor.press("ArrowRight");
  await editor.evaluate(
    (element, input) => {
      const bytes = Uint8Array.from(
        atob(
          "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
        ),
        (character) => character.charCodeAt(0),
      );
      const clipboard = new DataTransfer();
      for (const name of input.names)
        clipboard.items.add(new File([bytes], name, { type: "image/png" }));
      element.dispatchEvent(
        new ClipboardEvent("paste", {
          bubbles: true,
          clipboardData: clipboard,
        }),
      );
    },
    { names },
  );
}
async function openComposer(page: Page) {
  await page.locator("details.beta-composer > summary").click();
  const composer = page.getByRole("dialog", { name: "Share an update", exact: true });
  await expect(composer).toBeVisible();
  await expect(composer.getByLabel("Post message", { exact: true })).toHaveAttribute("contenteditable", "true");
  return composer;
}

test("pastes two images at the caret, restores their draft order, and publishes them", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    await login(context, "mali");
    const page = await context.newPage();
    const marker = `Photo order ${randomUUID().slice(0, 8)}`;
    const firstName = `first-${marker.slice(-8)}.png`;
    const secondName = `second-${marker.slice(-8)}.png`;
    await page.goto(`${base}/home`);
    const composer = await openComposer(page);
    const editor = composer.getByLabel("Post message", { exact: true }).first();
    await expect(editor).toBeFocused();
    const before = `${marker} before`;
    const after = "after the photos";
    await editor.fill(`${before}${after}`);
    await pasteImages(editor, [firstName, secondName], before.length);
    await expect(composer.getByRole("img", { name: firstName })).toBeVisible();
    await expect(composer.getByRole("img", { name: secondName })).toBeVisible();
    await expect(
      editor.locator("p").first(),
    ).toHaveText(before);
    await expect(
      editor.locator("p").last(),
    ).toHaveText(after);
    await composer
      .getByRole("button", { name: "Save draft", exact: true })
      .click();
    await expect(page.locator(".beta-status")).toContainText("Draft saved", {
      timeout: 30000,
    });

    await page.reload();
    const restored = await openComposer(page);
    await restored
      .locator("details > summary")
      .filter({ hasText: "Your private drafts" })
      .click();
    await restored.locator(".beta-draft").filter({ hasText: marker }).click();
    await expect(restored.getByRole("img", { name: firstName })).toBeVisible();
    await expect(restored.getByRole("img", { name: secondName })).toBeVisible();
    await expect(
      restored.getByLabel("Post message", { exact: true }).locator("p").first(),
    ).toHaveText(before);
    await expect(
      restored.getByLabel("Post message", { exact: true }).locator("p").last(),
    ).toHaveText(after);
    await restored.getByLabel("Post audience").selectOption({ index: 1 });
    await restored
      .getByRole("button", { name: "Share update", exact: true })
      .click();
    const article = page.getByRole("article").filter({ hasText: marker });
    await expect(article).toBeVisible({ timeout: 30000 });
    await expect(article.getByRole("img", { name: firstName })).toBeVisible();
    await expect(article.getByRole("img", { name: secondName })).toBeVisible();
    const permalink = await article.locator("header a").getAttribute("href");
    const postId = permalink?.split("/").pop();
    expect(postId).toBeTruthy();
    const response = await context.request.get(
      `/api/v1${base}/posts/${postId}`,
    );
    expect(response.status(), await response.text()).toBe(200);
    const post = (await response.json()).data;
    const textBlocks = post.content.filter((block: { type: string }) => block.type === "text") as { text: string }[];
    expect(textBlocks.map(block => block.text).filter(text => text.trim())).toEqual([before, after]);
    expect(post.body).toBe(textBlocks.map(block => block.text).join("\n"));
    expect(post.content.filter((block: { type: string; text?: string }) => block.type === "image" || block.text?.trim()).map((block: { type: string }) => block.type)).toEqual([
      "text",
      "image",
      "image",
      "text",
    ]);
    expect(
      post.content
        .filter((block: { type: string }) => block.type === "image")
        .map((block: { name: string }) => block.name),
    ).toEqual([firstName, secondName]);
  } finally {
    await context.close();
  }
});

test("publishes an image-only employee post", async ({ browser }, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    const employeeBase = "/holdings/acme/branches/new-york";
    await login(context, "noah");
    const page = await context.newPage();
    const name = `photo-only-${randomUUID().slice(0, 8)}.png`;
    await page.goto(`${employeeBase}/home`);
    const composer = await openComposer(page);
    await composer.getByLabel("Add images to post", { exact: true }).setInputFiles({
      name,
      mimeType: "image/png",
      buffer: tinyPng,
    });
    await expect(composer.getByRole("img", { name })).toBeVisible();
    await expect(
      composer.getByRole("button", { name: "Share update", exact: true }),
    ).toBeEnabled();
    await composer.getByLabel("Post audience").selectOption({ index: 1 });
    await composer
      .getByRole("button", { name: "Share update", exact: true })
      .click();
    const article = page
      .getByRole("article")
      .filter({ has: page.getByRole("img", { name }) });
    await expect(article).toBeVisible({ timeout: 30000 });
    const permalink = await article.locator("header a").getAttribute("href");
    const postId = permalink?.split("/").pop();
    expect(postId).toBeTruthy();
    const response = await context.request.get(
      `/api/v1${employeeBase}/posts/${postId}`,
    );
    expect(response.status(), await response.text()).toBe(200);
    const post = (await response.json()).data;
    expect(post.body).toBe("");
    expect(
      post.content.filter((block: { type: string }) => block.type === "image"),
    ).toEqual([expect.objectContaining({ type: "image", name })]);
  } finally {
    await context.close();
  }
});

test("lets an author delete a published post from the feed", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  const reader = await browser.newContext(info.project.use);
  let postId: string | undefined;
  try {
    await Promise.all([login(context, "mali"), login(reader, "nisa")]);
    const page = await context.newPage();
    const marker = `Delete post ${randomUUID().slice(0, 8)}`;
    const imageName = `delete-${marker.slice(-8)}.png`;
    await page.goto(`${base}/home`);
    const composer = await openComposer(page);
    await composer.getByLabel("Post message", { exact: true }).fill(marker);
    await composer.getByLabel("Add images to post", { exact: true }).setInputFiles({
      name: imageName,
      mimeType: "image/png",
      buffer: tinyPng,
    });
    await composer.getByLabel("Post audience").selectOption({ index: 1 });
    await composer
      .getByRole("button", { name: "Share update", exact: true })
      .click();

    const article = page.getByRole("article").filter({ hasText: marker });
    await expect(article).toBeVisible({ timeout: 30000 });
    const permalink = await article.locator("header a").getAttribute("href");
    postId = permalink?.split("/").pop();
    expect(postId).toBeTruthy();
    const imageLink = article.locator(".beta-post-inline-image a");
    await expect(imageLink).toBeVisible();
    const fileUrl = await imageLink.getAttribute("href");
    expect(fileUrl).toBeTruthy();

    const readerPage = await reader.newPage();
    await readerPage.goto(`${base}/home`);
    const readerArticle = readerPage
      .getByRole("article")
      .filter({ hasText: marker });
    await expect(readerArticle).toBeVisible();
    await readerArticle.locator("details.post-more > summary").click();
    await expect(
      readerArticle.getByRole("button", { name: "Delete post", exact: true }),
    ).toHaveCount(0);

    await article.locator("details.post-more > summary").click();
    await expect(
      article
        .getByRole("button", { name: "Edit history", exact: true })
        .locator("svg"),
    ).toHaveCount(1);
    const deleteButton = article.getByRole("button", {
      name: "Delete post",
      exact: true,
    });
    await expect(deleteButton.locator("svg")).toHaveCount(1);
    await deleteButton.click();
    const dialog = page.getByRole("dialog", { name: "Delete post?" });
    await expect(dialog).toBeVisible();
    const keepButton = dialog.getByRole("button", {
      name: "Keep post",
      exact: true,
    });
    await expect(keepButton).toBeFocused();
    await keepButton.click();
    await expect(dialog).toBeHidden();
    await expect(article).toBeVisible();
    await expect(article.locator("details.post-more > summary")).toBeFocused();

    await article.locator("details.post-more > summary").click();
    await article
      .getByRole("button", { name: "Delete post", exact: true })
      .click();
    await dialog
      .getByRole("button", { name: "Delete post", exact: true })
      .click();
    await expect(article).toHaveCount(0, { timeout: 30000 });
    await expect(page.locator("section.beta-feed")).toBeFocused();
    await expect(
      page.getByRole("status").filter({ hasText: "Post deleted." }),
    ).toBeVisible();
    const response = await context.request.get(
      `/api/v1${base}/posts/${postId}`,
    );
    expect(response.status(), await response.text()).toBe(404);
    const fileResponse = await context.request.get(fileUrl!);
    expect(fileResponse.status(), await fileResponse.text()).toBe(404);
  } finally {
    if (postId) {
      try {
        const current = await context.request.get(
          `/api/v1${base}/posts/${postId}`,
        );
        if (current.ok()) {
          const post = (await current.json()).data;
          await context.request.post(`/api/v1${base}/posts/${postId}`, {
            headers: { "Idempotency-Key": randomUUID() },
            data: { action: "retract", expectedVersion: post.version },
          });
        }
      } catch {}
    }
    await Promise.all([context.close(), reader.close()]);
  }
});

test("keeps a failed image upload in the composer and retries it", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    await login(context, "mali");
    const page = await context.newPage();
    const marker = `Retry photo ${randomUUID().slice(0, 8)}`;
    const name = `retry-${marker.slice(-8)}.png`;
    await page.goto(`${base}/home`);
    const composer = await openComposer(page);
    await composer
      .getByLabel("Post message", { exact: true })
      .first()
      .fill(marker);
    await composer
      .getByLabel("Add images to post", { exact: true })
      .setInputFiles({
        name: "wrong.txt",
        mimeType: "text/plain",
        buffer: Buffer.from("not an image"),
      });
    await expect(composer.getByRole("status").filter({ hasText: "JPEG, PNG, and WebP" })).toContainText(
      "JPEG, PNG, and WebP",
    );
    await expect(composer.getByRole("img", { name: "wrong.txt" })).toHaveCount(
      0,
    );
    await composer
      .getByLabel("Add images to post", { exact: true })
      .setInputFiles({
        name: "too-large.png",
        mimeType: "image/png",
        buffer: Buffer.alloc(10 * 1024 * 1024 + 1),
      });
    await expect(composer.getByRole("status").filter({ hasText: "10 MB" })).toContainText("10 MB");
    await expect(
      composer.getByRole("img", { name: "too-large.png" }),
    ).toHaveCount(0);
    await composer
      .getByLabel("Add images to post", { exact: true })
      .setInputFiles({ name, mimeType: "image/png", buffer: tinyPng });
    let failed = false;
    await page.route("**/uploads/*/content", async (route) => {
      if (route.request().method() === "PUT" && !failed) {
        failed = true;
        await route.fulfill({
          status: 503,
          contentType: "application/json",
          body: JSON.stringify({
            error: { message: "Temporary upload failure" },
          }),
        });
      } else await route.continue();
    });
    await composer.getByLabel("Post audience").selectOption({ index: 1 });
    await composer
      .getByRole("button", { name: "Share update", exact: true })
      .click();
    await expect(page.locator(".beta-status")).toContainText(
      "Temporary upload failure",
      { timeout: 30000 },
    );
    await expect(
      composer.getByLabel("Post message", { exact: true }).first(),
    ).toContainText(marker);
    await expect(composer.getByRole("img", { name })).toBeVisible();
    await composer
      .getByRole("button", { name: "Share update", exact: true })
      .click();
    await expect(
      page
        .getByRole("article")
        .filter({ hasText: marker })
        .getByRole("img", { name }),
    ).toBeVisible({ timeout: 30000 });
  } finally {
    await context.close();
  }
});
test("composer removes the duplicate avatar and accepts pasted images", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    const response = await context.request.post("/api/auth/sign-in/email", {
      data: {
        email:
          process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
        password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
      },
    });
    expect(response.status(), await response.text()).toBe(200);
    const signedIn = await response.json() as { user: { id: string } };
    await selectFixtureAssignment(context, "acme-co", `dev_${signedIn.user.id}_acme-co_holding_owner`);
    const page = await context.newPage();
    await page.goto("/holdings/acme/home");
    await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible();
    await expect(page.getByText(/Some information is unavailable/)).toHaveCount(
      0,
    );
    const composer = await openComposer(page);
    await expect(composer.locator(".beta-person")).toHaveCount(0);
    const editor = composer.getByLabel("Post message", { exact: true });
    await expect(editor).toBeVisible();
    await pasteImages(editor, ["pasted.png"], 0);
    await expect(
      composer.getByRole("img", { name: "pasted.png" }),
    ).toBeVisible();
    await expect(
      composer.getByRole("button", { name: "Remove pasted.png" }),
    ).toBeVisible();
    await expect(editor.locator(".beta-editor-image")).toHaveCount(1);
  } finally {
    await context.close();
  }
});
test("social post → manual task → evidence → independent acceptance, with AI fallback", async ({
  browser,
}, info) => {
  test.setTimeout(240000);
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
    const composer = await openComposer(page);
    await composer
      .getByLabel("Post message", { exact: true })
      .fill(`${title} · เครื่องชงกาแฟเสีย ช่วยตรวจสอบด้วย`);
    await composer.getByLabel("Add images to post", { exact: true }).setInputFiles({
      name: "equipment.png",
      mimeType: "image/png",
      buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
        "base64",
      ),
    });
    await composer.getByLabel("Post audience").selectOption({ index: 1 });
    await composer
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
    const file = await article
      .locator(".beta-post-inline-image a")
      .getAttribute("href");
    expect((await foreign.request.get(file!)).status()).toBeGreaterThanOrEqual(
      403,
    );
    await article.getByRole("button", { name: /Comment/ }).click();
    await article
      .getByRole("combobox", { name: "Write a comment", exact: true })
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
    await dialog.getByLabel(/^Task title(?:\s*\*)?$/).fill(title);
    await dialog
      .getByRole("button", { name: "Let AI prepare this task", exact: true })
      .click();
    await expect(dialog.getByRole("status")).toContainText(
      "AI temporarily unavailable",
    );
    await expect(dialog.getByLabel(/^Task title(?:\s*\*)?$/)).toHaveValue(
      title,
    );
    await dialog
      .getByLabel("Responsible teammate", { exact: true })
      .selectOption("usr_nisa");
    await dialog
      .getByLabel("Independent completion reviewer", { exact: true })
      .selectOption("usr_ava");
    await dialog
      .getByLabel("Due date", { exact: true })
      .fill("2026-10-01T12:00");
    await dialog
      .getByRole("button", {
        name: "Create draft task",
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
    const evidence = work.getByRole("heading", { name: "Task evidence", exact: true }).locator("xpath=ancestor::section[1]");
    await evidence
      .locator('input[type="file"]:not([capture])')
      .setInputFiles({
        name: "inspection.png",
        mimeType: "image/png",
        buffer: tinyPng,
      });
    await expect(evidence.getByText("Ready to attach", { exact: true })).toBeVisible({ timeout: 120000 });
    const download = evidence.getByRole("link", { name: "Download inspection.png", exact: true });
    await expect(download).toBeVisible();
    const evidenceUrl = await download.getAttribute("href");
    expect(evidenceUrl).toBeTruthy();
    const privateEvidence = await assignee.request.get(evidenceUrl!);
    expect(privateEvidence.status()).toBe(200);
    expect(privateEvidence.headers()["cache-control"]).toBe("private, no-store");
    expect([403, 404]).toContain((await foreign.request.get(evidenceUrl!)).status());
    await work
      .getByLabel("Completion or decision note", { exact: true })
      .fill("Power switch replaced and tested.");
    const beforeCompletion = await assignee.request.get(`/api/v1${base}/staff-tasks/${taskId}`);
    expect(beforeCompletion.ok(), await beforeCompletion.text()).toBe(true);
    const executingVersion = (await beforeCompletion.json()).data.task.version as number;
    const completion = work.waitForResponse(response => new URL(response.url()).pathname === `/api/v1${base}/staff-tasks/${taskId}/transition` && response.request().method() === "POST");
    await work
      .getByRole("button", { name: "Submit completion", exact: true })
      .click();
    const submittedCompletion = await completion;
    expect(submittedCompletion.ok(), await submittedCompletion.text()).toBe(true);
    expect(submittedCompletion.request().postDataJSON()).toMatchObject({ action: "complete", expectedVersion: executingVersion });
    expect(submittedCompletion.request().postDataJSON().attachmentSetIds).toHaveLength(1);
    await expect(
      work.getByRole("button", { name: "Submit completion", exact: true }),
    ).toHaveCount(0);
    const review = await reviewer.newPage();
    await review.goto(taskUrl);
    await expect(review.getByRole("link", { name: "Download inspection.png", exact: true })).toBeVisible();
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
