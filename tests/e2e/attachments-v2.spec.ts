import { expect, test, type Page } from "@playwright/test";

const base = "/holdings/acme";
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
  "base64",
);

test.setTimeout(180_000);
test.use({ actionTimeout: 20_000 });

async function visit(page: Page, path: string) {
  await page.goto(`${base}${path}`);
  if (page.url().includes("/work-role")) {
    await page
      .getByRole("combobox", { name: "Role for actions & approvals" })
      .fill("people manager");
    await page.getByRole("listbox").getByRole("option").first().click();
    await page.goto(`${base}${path}`);
  }
}

test.beforeEach(async ({ context }) => {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
});

test("canonical Leave evidence supports resilient private files on desktop and mobile", async ({
  page,
  context,
}) => {
  const health = await (await context.request.get("/api/health")).json();
  test.skip(
    health.attachments?.enabled !== true,
    "Canonical attachment gate is intentionally disabled for this environment.",
  );

  await visit(page, "/leave/new");
  await page.getByRole("combobox", { name: "Leave type", exact: true }).selectOption({ index: 1 });
  await page.getByLabel("First day", { exact: true }).fill("2026-11-09");
  await page.getByLabel("Last day", { exact: true }).fill("2026-11-09");
  await page.getByLabel("Handover note").fill("Canonical attachment browser acceptance draft.");
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await expect(page).toHaveURL(/\/tasks\/people_payroll\/leave-/);

  const panel = page
    .getByRole("heading", { name: "Medical certificates", exact: true })
    .locator("xpath=ancestor::section[1]");
  await expect(panel).toBeVisible();
  await expect(panel).toHaveAttribute("aria-labelledby", /.+/);
  const picker = panel.locator('input[type="file"]:not([capture])');
  const camera = panel.locator('input[type="file"][capture="environment"]');
  await expect(picker).toHaveAttribute("multiple", "");
  await expect(camera).toHaveAttribute("accept", "image/*");

  let interruptNextUpload = true;
  await page.route("**/uploads/*/content", async (route) => {
    if (route.request().method() === "PUT" && interruptNextUpload) {
      interruptNextUpload = false;
      await route.abort("failed");
      return;
    }
    await route.continue();
  });
  await picker.setInputFiles([
    { name: "medical-certificate.png", mimeType: "image/png", buffer: png },
    { name: "supporting-photo.png", mimeType: "image/png", buffer: png },
  ]);

  await expect(panel.getByText("medical-certificate.png", { exact: true })).toBeVisible();
  await expect(panel.getByText("supporting-photo.png", { exact: true })).toBeVisible();
  const retry = panel.getByRole("button", { name: "Retry upload for medical-certificate.png" });
  await expect(retry).toBeVisible();
  await retry.click();
  await page.unroute("**/uploads/*/content");

  await expect(panel.getByText("Ready to attach", { exact: true })).toHaveCount(2, {
    timeout: 120_000,
  });
  await expect(panel.getByRole("list", { name: "Medical certificates file list" })).toBeVisible();

  const previewUrl = await panel
    .getByRole("link", { name: "Preview medical-certificate.png" })
    .getAttribute("href");
  expect(previewUrl).toBeTruthy();
  const preview = await context.request.get(previewUrl!);
  expect(preview.status()).toBe(200);
  expect(preview.headers()["content-type"]).toBe("image/webp");
  expect(preview.headers()["cache-control"]).toBe("private, no-store");
  expect(preview.headers()["x-content-type-options"]).toBe("nosniff");

  const originalUrl = await panel
    .getByRole("link", { name: "Download medical-certificate.png" })
    .getAttribute("href");
  expect(originalUrl).toBeTruthy();
  const original = await context.request.get(originalUrl!);
  expect(original.status()).toBe(200);
  expect(original.headers()["content-disposition"]).toContain("attachment");
  expect(original.headers()["cache-control"]).toBe("private, no-store");

  await panel.getByRole("button", { name: "Withdraw supporting-photo.png" }).click();
  await panel.getByLabel("Reason for withdrawing supporting-photo.png").fill(
    "Browser acceptance correction",
  );
  await panel.getByRole("button", { name: "Withdraw file" }).click();
  await expect(panel.getByText("Withdrawn", { exact: true })).toBeVisible();

  await page.reload({ waitUntil: "domcontentloaded" });
  const reloaded = page
    .getByRole("heading", { name: "Medical certificates", exact: true })
    .locator("xpath=ancestor::section[1]");
  await expect(reloaded.getByText("medical-certificate.png", { exact: true })).toBeVisible();
  await expect(reloaded.getByText("Ready to attach", { exact: true })).toBeVisible();
});
