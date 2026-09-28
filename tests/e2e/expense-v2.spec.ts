import {
  test,
  expect as baseExpect,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { readFileSync, existsSync } from "node:fs";
import { randomUUID } from "node:crypto";

// A dedicated local company, created by scripts/seed-expense-e2e.ts.
const fixturePath = ".scratch/expense-v2/fixture.json";
type Fixture = {
  company: string;
  accounts: { expense: string };
  period: string;
  password: string;
  actors: Record<string, { email: string; assignmentId: string; userId: string }>;
};
const fixture: Fixture = existsSync(fixturePath)
  ? JSON.parse(readFileSync(fixturePath, "utf8"))
  : null;
test.skip(
  !fixture,
  "Run bun scripts/seed-expense-e2e.ts against the local database first.",
);
const base = "/holdings/acme";
const expect = baseExpect.configure({ timeout: 15000 });
test.use({ actionTimeout: 15000 });

async function login(context: BrowserContext, who: string) {
  const response = await context.request.post("/api/auth/sign-in/email", {
    timeout: 60000,
    data: { email: fixture.actors[who].email, password: fixture.password },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  await context.addCookies([
    {
      name: "erpx_company_h001",
      value: fixture.company,
      url: process.env.ERPX_E2E_BASE_URL ?? "http://localhost:8787",
    },
    {
      name: "erpx_assignment_h001",
      value: fixture.actors[who].assignmentId,
      url: process.env.ERPX_E2E_BASE_URL ?? "http://localhost:8787",
    },
  ]);
  const activeResponse = await context.request.get(`/api/v1/holdings/acme/assignments/active`);
  expect(activeResponse.ok(), await activeResponse.text()).toBeTruthy();
  const active = (await activeResponse.json()).data as { revision: number };
  const selectedResponse = await context.request.post(`/api/v1/holdings/acme/assignments/active`, {
    headers: { "x-erpx-context-company": fixture.company },
    data: { assignmentId: fixture.actors[who].assignmentId, revision: Number(active.revision ?? 0) },
  });
  expect(selectedResponse.ok(), await selectedResponse.text()).toBeTruthy();
}
async function contextHeaders(context: BrowserContext, who: string) {
  const activeResponse = await context.request.get(
    `/api/v1/holdings/acme/assignments/active`,
  );
  expect(activeResponse.ok(), await activeResponse.text()).toBeTruthy();
  const active = (await activeResponse.json()).data as {
    revision: number;
    companyId: string;
    assignmentId: string;
  };
  return {
    "x-erpx-context-revision": String(active.revision),
    "x-erpx-context-company": active.companyId || fixture.company,
    "x-erpx-context-assignment":
      active.assignmentId || fixture.actors[who].assignmentId,
  };
}
async function sampleImage(page: Page, text: string, format = "image/png") {
  return Buffer.from(
    await page.evaluate(
      ({ text, format }) => {
        const canvas = document.createElement("canvas");
        canvas.width = 800;
        canvas.height = 600;
        const c = canvas.getContext("2d")!;
        c.fillStyle = "white";
        c.fillRect(0, 0, 800, 600);
        c.fillStyle = "black";
        c.font = "24px sans-serif";
        text
          .split("\n")
          .forEach((line, i) => c.fillText(line, 30, 60 + i * 40));
        return canvas.toDataURL(format).split(",")[1];
      },
      { text, format },
    ),
    "base64",
  );
}
function scannedPdf(jpeg: Buffer) {
  const objects = [
    Buffer.from("<< /Type /Catalog /Pages 2 0 R >>"),
    Buffer.from("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
    Buffer.from(
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 800 600] /Resources << /XObject << /Scan 4 0 R >> >> /Contents 5 0 R >>",
    ),
    Buffer.concat([
      Buffer.from(
        `<< /Type /XObject /Subtype /Image /Width 800 /Height 600 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n`,
      ),
      jpeg,
      Buffer.from("\nendstream"),
    ]),
    Buffer.from(
      "<< /Length 31 >>\nstream\nq 800 0 0 600 0 0 cm /Scan Do Q\n\nendstream",
    ),
  ];
  const chunks = [Buffer.from("%PDF-1.4\n")],
    offsets = [0];
  let length = chunks[0].length;
  objects.forEach((object, i) => {
    offsets.push(length);
    const chunk = Buffer.concat([
      Buffer.from(`${i + 1} 0 obj\n`),
      object,
      Buffer.from("\nendobj\n"),
    ]);
    chunks.push(chunk);
    length += chunk.length;
  });
  chunks.push(
    Buffer.from(
      `xref\n0 6\n0000000000 65535 f \n${offsets
        .slice(1)
        .map((n) => `${String(n).padStart(10, "0")} 00000 n \n`)
        .join(
          "",
        )}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${length}\n%%EOF`,
    ),
  );
  return Buffer.concat(chunks);
}
async function snapshots(page: Page, name: string) {
  for (const width of [390, 768, 1440])
    for (const theme of ["light", "dark"] as const) {
      await page.setViewportSize({ width, height: 950 });
      await page.emulateMedia({ colorScheme: theme });
      await page.evaluate(
        (theme) => (document.documentElement.dataset.theme = theme),
        theme,
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth + 1,
        ),
      ).toBe(true);
      if (width === 390) {
        const small = await page
          .locator(
            "main button:visible, main input:not([type=checkbox]):not([type=hidden]):visible, main select:visible",
          )
          .evaluateAll((elements) =>
            elements
              .filter((e) => e.getBoundingClientRect().height < 43.5)
              .map(
                (e) =>
                  e.getAttribute("aria-label") || e.textContent || e.tagName,
              ),
          );
        expect(small).toEqual([]);
      }
      await page.screenshot({
        path: `.scratch/expense-v2/${name}-${width}-${theme}.png`,
        fullPage: true,
      });
    }
}
test("every member can preserve an incomplete claim and reach People setup", async ({
  browser,
}, info) => {
  test.setTimeout(90000);
  const context = await browser.newContext(info.project.use);
  try {
    await login(context, "missing");
    const page = await context.newPage();
    await page.goto(`${base}/work?type=staff_task&queue=all&q=keep`);
    await page.getByRole("button", { name: "New", exact: true }).click();
    await page
      .getByRole("dialog", { name: "New work" })
      .getByRole("link", { name: "Expense claim", exact: true })
      .click();
    await page
      .getByLabel("Merchant", { exact: true })
      .fill("Preserved incomplete claim");
    await page.reload();
    await expect(page.getByLabel("Merchant", { exact: true })).toHaveValue(
      "Preserved incomplete claim",
    );
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    await expect(page).toHaveURL(/\/expenses\/expense-/);
    await expect(
      page.getByText("Before you submit", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Open claimant profile in People" }),
    ).toBeVisible();
    await page
      .getByRole("textbox", { name: "Business purpose", exact: true })
      .fill("My unsaved explanation");
    await page.reload();
    await expect(
      page.getByRole("textbox", { name: "Business purpose", exact: true }),
    ).toHaveValue("My unsaved explanation");
    await snapshots(page, `draft-${info.project.name}`);
    await page
      .getByRole("link", { name: "Open claimant profile in People" })
      .click();
    await expect(page.getByRole("dialog").last()).toBeVisible();
    await page.goBack();
    await expect(
      page.getByRole("textbox", { name: "Business purpose", exact: true }),
    ).toHaveValue("My unsaved explanation");
  } finally {
    await context.close();
  }
});
test("expense goes through four independent people and two slip-backed payments", async ({
  browser,
}, info) => {
  test.setTimeout(240000);
  const contexts: BrowserContext[] = [];
  for (const who of ["claimant", "head", "reviewer", "manager", "payer", "owner"]) {
    const context = await browser.newContext(info.project.use);
    contexts.push(context);
    await login(context, who);
  }
  const [claimant, head, reviewer, manager, payer, owner] = contexts;
  try {
    const page = await claimant.newPage(),
      merchant = `Test receipt ${randomUUID().slice(0, 8)}`;
    await page.goto(`${base}/work?new=expense&type=expense`);
    await page.getByLabel("Expense date", { exact: true }).fill("2026-09-10");
    await page.getByLabel("Merchant", { exact: true }).fill(merchant);
    await page.getByLabel("Receipt number", { exact: true }).fill(merchant);
    await page
      .getByRole("combobox", { name: "Expense category", exact: true })
      .selectOption(fixture.accounts.expense);
    await page.getByLabel("Net amount", { exact: true }).fill("100");
    await page.getByLabel("Recoverable tax", { exact: true }).fill("7");
    await page
      .getByRole("textbox", { name: "Business purpose", exact: true })
      .fill("Sample supplies for the test team");
    await page.getByRole("button", { name: "Save draft", exact: true }).click();
    await expect(page).toHaveURL(/\/expenses\/expense-/);
    const url = page.url(),
      id = url.split("/").pop()!;
    const receipt = scannedPdf(
      await sampleImage(
        page,
        `TEST RECEIPT\n${merchant}\n2026-09-10\nSupplies 100.00\nTax 7.00\nTotal THB 107.00`,
        "image/jpeg",
      ),
    );
    await page.getByLabel("Receipt file", { exact: true }).setInputFiles({
      name: "scanned-receipt.pdf",
      mimeType: "application/pdf",
      buffer: receipt,
    });
    const derivative = page.waitForResponse(
      (r) =>
        r.url().includes("/derivatives?") && r.request().method() === "POST",
      { timeout: 60000 },
    );
    await page
      .getByRole("button", { name: "Attach receipt", exact: true })
      .click();
    expect((await derivative).status()).toBe(200);
    await expect(
      page.getByRole("link", { name: /^scanned-receipt.pdf/ }),
    ).toBeVisible();
    await expect(page.locator(".error[role=alert]")).toHaveCount(0);
    await page
      .getByRole("button", { name: "Submit for department approval" })
      .click();
    await expect(
      page.getByRole("heading", { name: "Review and decide", exact: true }),
    ).toBeVisible();
    const flowEndpoint=`/api/v1${base}/flows/expense/${id}`;
    const flowResponse=await owner.request.get(flowEndpoint);
    expect(flowResponse.ok(),await flowResponse.text()).toBeTruthy();
    const flow=(await flowResponse.json()).data;
    const assigned=await owner.request.post(flowEndpoint,{headers:{...await contextHeaders(owner,"owner"),"Idempotency-Key":randomUUID()},data:{stepKey:"department_approval",userId:fixture.actors.head.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:"Primary reviewer for this isolated browser test",saveDefault:false}});
    expect(assigned.ok(),await assigned.text()).toBeTruthy();
    const approve = await head.newPage();
    await approve.goto(url);
    await approve
      .getByLabel("Decision reason")
      .fill("Purpose, amount and receipt verified");
    await approve
      .getByRole("button", { name: "Approve department review" })
      .click();
    await expect(
      approve.getByRole("heading", { name: "Accounts review", exact: true }),
    ).toBeVisible();
    const accounts = await reviewer.newPage();
    await accounts.goto(url);
    await accounts
      .getByRole("combobox", { name: "Accounting period", exact: true })
      .selectOption(fixture.period);
    await accounts
      .getByRole("textbox", { name: "Accounting note", exact: true })
      .fill("Documents and tax reviewed");
    await accounts.reload();
    await expect(
      accounts.getByRole("textbox", { name: "Accounting note", exact: true }),
    ).toHaveValue("Documents and tax reviewed");
    await accounts
      .getByRole("button", { name: "Save accounting preparation" })
      .click();
    await expect(
      accounts.getByRole("button", { name: "Save accounting preparation" }),
    ).toBeEnabled();
    await accounts
      .getByLabel("Decision reason")
      .fill("Accounting classification verified");
    await accounts
      .getByRole("button", { name: "Confirm Accounts review" })
      .click();
    await expect(
      accounts.getByRole("heading", {
        name: "Accounts manager approval",
        exact: true,
      }),
    ).toBeVisible();
    const post = await manager.newPage();
    await post.goto(url);
    await post
      .getByLabel("Decision reason")
      .first()
      .fill("Approve accounting and payable posting");
    await post
      .getByRole("button", { name: "Approve accounting → GL review" })
      .click();
    await expect(post.getByRole("heading",{name:"Post general ledger",exact:true})).toBeVisible();
    await post.getByRole("button",{name:"Confirm Debit / Credit and post to GL", exact:true}).click();
    await expect(
      post.getByRole("heading", { name: "Finance payment", exact: true }),
    ).toBeVisible();
    const payment = await payer.newPage();
    await payment.goto(url);
    for (const [index, amount] of ["50.00", "57.00"].entries()) {
      await payment
        .getByRole("button", { name: "Record bank payment", exact: true })
        .click();
      await expect(
        payment.getByLabel("Payment date", { exact: true }),
      ).toBeVisible();
      await payment
        .getByLabel("Payment date", { exact: true })
        .fill("2026-09-21");
      await payment
        .getByLabel("Amount paid (THB)", { exact: true })
        .fill(amount);
      await payment
        .getByLabel("Bank reference", { exact: true })
        .fill(`TEST-${id}-${index}`);
      await payment
        .getByLabel("Payment slip file", { exact: true })
        .setInputFiles({
          name: `test-bank-slip-${index}.png`,
          mimeType: "image/png",
          buffer: await sampleImage(
            payment,
            `TEST BANK SLIP — NOT A REAL PAYMENT\nReference TEST-${id}-${index}\n2026-09-10\nTHB ${amount}`,
          ),
        });
      await payment
        .getByRole("button", { name: "Attach slip", exact: true })
        .click();
      await expect(
        payment.getByRole("link", {
          name: `Preview of test-bank-slip-${index}.png`, exact: true,
        }),
      ).toBeVisible();
      await expect(
        payment.getByLabel("Amount paid (THB)", { exact: true }),
      ).toHaveValue(amount);
      await expect(
        payment.getByRole("button", { name: "Confirm bank payment" }),
      ).toBeDisabled();
      await payment.getByRole("checkbox").check();
      if (index === 0) await snapshots(payment, `payment-${info.project.name}`);
      const confirmed = payment.waitForResponse(
        (r) => r.url().endsWith("/confirm") && r.request().method() === "POST",
      );
      await payment
        .getByRole("button", { name: "Confirm bank payment" })
        .click();
      const confirmation=await confirmed; expect(confirmation.ok(),await confirmation.text()).toBeTruthy();
      await expect(
        payment.getByLabel("Payment date", { exact: true }),
      ).toHaveCount(0);
    }
    await expect(
      payment.getByRole("heading", { name: "Completed", exact: true }),
    ).toBeVisible();
    const result = await claimant.request.get(`/api/v1${base}/expenses/${id}`);
    const data = (await result.json()).data;
    expect(data.expense.status).toBe("reimbursed");
    expect(
      data.paymentDrafts.filter(
        (d: { status: string }) => d.status === "confirmed",
      ),
    ).toHaveLength(2);
  } finally {
    await Promise.all(contexts.map((c) => c.close()));
  }
});
test("IT can save a masked custom provider draft while employees are denied", async ({
  browser,
}, info) => {
  test.setTimeout(90000);
  const context = await browser.newContext(info.project.use),
    employee = await browser.newContext(info.project.use);
  try {
    await login(context, "it");
    await login(employee, "missing");
    expect(
      (await employee.request.get(`/api/v1${base}/ai-providers`)).status(),
    ).toBe(403);
    const page = await context.newPage();
    await page.goto(`${base}/settings?section=ai`);
    await expect(
      page.getByRole("heading", { name: "AI providers", exact: true }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Add connection" }).click();
    const name = `Custom draft ${randomUUID().slice(0, 8)}`;
    await page.getByLabel("Connection name").fill(name);
    await page.getByLabel("Base URL").fill("https://api.example.com/v1");
    await expect(
      page.getByRole("combobox", { name: "Authentication", exact: true }),
    ).toHaveValue("bearer");
    await page
      .getByLabel("API key", { exact: true })
      .fill("synthetic-test-key-do-not-send");
    await page.getByLabel("Text model ID").fill("manual-test-model");
    await page.getByRole("button", { name: "Save connection" }).click();
    await expect(
      page.getByRole("heading", { name, exact: true }),
    ).toBeVisible();
    const response = await context.request.get(`/api/v1${base}/ai-providers`);
    expect(response.status()).toBe(200);
    const body = await response.text();
    expect(body).not.toContain("synthetic-test-key");
    expect(body).not.toContain("credential_json");
    const card = page
      .getByRole("article")
      .filter({ has: page.getByRole("heading", { name, exact: true }) });
    await expect(
      card.getByRole("button", { name: "Activate", exact: true }),
    ).toBeDisabled();
    await card.getByRole("button", { name: "Edit connection", exact: true }).click();
    const apiKey = page.getByLabel("API key", { exact: true });
    const authentication = page.getByRole("combobox", {
      name: "Authentication",
      exact: true,
    });
    await expect(apiKey).toHaveValue("");
    await authentication.selectOption("none");
    await expect(apiKey).toBeDisabled();
    await expect(page.getByLabel(/Remove saved key/)).toHaveCount(0);
    await expect(
      page.getByText(
        "This draft will use an unauthenticated endpoint and will not retain the saved key.",
        { exact: true },
      ),
    ).toBeVisible();
    await expect(page.getByText("No credential stored", { exact: true })).toBeVisible();
    await authentication.selectOption("bearer");
    await expect(apiKey).toBeEnabled();
    await expect(page.getByLabel(/Remove saved key/)).not.toBeChecked();
    await expect(
      page.getByText("Leave blank to keep the saved credential.", {
        exact: true,
      }),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await snapshots(page, `providers-${info.project.name}`);
  } finally {
    await context.close();
    await employee.close();
  }
});

test("AI suggestions fill blanks, preserve human edits and require explicit replacement", async ({
  browser,
}, info) => {
  const context = await browser.newContext(info.project.use);
  try {
    await login(context, "claimant");
    const created = await context.request.post(`/api/v1${base}/expenses`, {
      headers: { ...(await contextHeaders(context,"claimant")), "Idempotency-Key": randomUUID() },
      data: { merchant: "Human merchant" },
    });
    expect(created.status()).toBe(201);
    const id = (await created.json()).data.id;
    const page = await context.newPage();
    const aiRequestMethods: string[] = [];
    // The service/worker is tested in integration; this controlled response tests
    // the suggestion-to-form handoff without contacting a real AI provider.
    await page.route(`**/expenses/${id}/ai`, (route) => {
      aiRequestMethods.push(route.request().method());
      return route.fulfill({
        json: {
          data: {
            allowed: true,
            unavailable: null,
            hasEvidence: true,
            incomplete: [],
            stale: false,
            sourceVersion: 1,
            evidenceDigest: "synthetic",
            proposal: {
              id: "synthetic-proposal",
              state: "ready",
              error: null,
              result: {
                summary: "Synthetic receipt fields for UI verification.",
                fields: [
                  {
                    name: "merchant",
                    value: "AI merchant",
                    citations: [
                      { attachmentId: "synthetic-evidence", page: 1 },
                    ],
                  },
                  {
                    name: "receiptReference",
                    value: "AI-REF-42",
                    citations: [
                      { attachmentId: "synthetic-evidence", page: 1 },
                    ],
                  },
                ],
                missing: ["Business purpose"],
                conflicts: [],
              },
            },
          },
        },
      });
    });
    await page.goto(`${base}/expenses/${id}`);
    await expect.poll(() => aiRequestMethods.length).toBeGreaterThan(0);
    expect(aiRequestMethods).toEqual(["GET"]);
    await expect(page.getByLabel("Merchant", { exact: true })).toHaveValue(
      "Human merchant",
    );
    await expect(
      page.getByLabel("Receipt number", { exact: true }),
    ).toHaveValue("AI-REF-42");
    await expect(
      page.getByRole("textbox", { name: "Business purpose", exact: true }),
    ).toHaveValue("");
    await page
      .getByText("Extraction details and sources", { exact: true })
      .click();
    await page
      .getByText("Suggested fields and sources", { exact: true })
      .click();
    await page
      .locator(".row")
      .filter({ has: page.getByText("AI merchant", { exact: true }) })
      .getByRole("button", { name: "Use suggestion" })
      .click();
    await expect(page.getByLabel("Merchant", { exact: true })).toHaveValue(
      "AI merchant",
    );
    await page.reload();
    await expect(page.getByLabel("Merchant", { exact: true })).toHaveValue(
      "AI merchant",
    );
    const server = (
      await (await context.request.get(`/api/v1${base}/expenses/${id}`)).json()
    ).data.expense;
    expect(server.merchant).toBe("Human merchant");
    expect(server.status).toBe("draft");
    await page
      .getByLabel("Merchant", { exact: true })
      .fill("My final human edit");
    await page.unroute(`**/expenses/${id}/ai`);
    await page.route(`**/expenses/${id}/ai`, (route) =>
      route.fulfill({
        json: {
          data: {
            allowed: true,
            unavailable: "Provider changed; continue manually.",
            hasEvidence: true,
            incomplete: [],
            stale: true,
            sourceVersion: 2,
            evidenceDigest: "changed",
            proposal: null,
          },
        },
      }),
    );
    await page.reload();
    await expect(
      page.getByLabel("Receipt number", { exact: true }),
    ).toHaveValue("");
    await expect(page.getByLabel("Merchant", { exact: true })).toHaveValue(
      "My final human edit",
    );
  } finally {
    await context.close();
  }
});
