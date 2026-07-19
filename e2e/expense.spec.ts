import type { APIRequestContext, APIResponse, Page } from '@playwright/test';
import { test, expect, PERSONAS } from './fixtures';
import { cleanupTestTag, q1, userIdByCode, vendorTag } from './helpers/db';
import { uploadSlip, type UploadSlipOptions } from './helpers/upload';
import { signInOnPage, signOut } from './helpers/waybill';

interface UploadBody {
  slipId: number;
  parsed: Record<string, unknown>;
  fileKey: string;
  validation: { ok: boolean };
}

function receipt(tag: string): Buffer {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="600" height="800"><rect width="600" height="800" fill="#fff"/><text x="40" y="60" font-size="32" font-family="sans-serif">RECEIPT</text><text x="40" y="120" font-size="20" font-family="sans-serif">Vendor: ${tag}</text><text x="40" y="160" font-size="20" font-family="sans-serif">Date: 2026-07-18</text><text x="40" y="220" font-size="20" font-family="sans-serif">Subtotal: 100.00</text><text x="40" y="260" font-size="20" font-family="sans-serif">VAT 7%: 7.00</text><text x="40" y="320" font-size="28" font-family="sans-serif">Total: 107.00</text></svg>`);
}

function bank(tag: string): Buffer {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="600" height="400"><rect width="600" height="400" fill="#fff"/><text x="40" y="60" font-size="28" font-family="sans-serif">BANK BOOK</text><text x="40" y="120" font-size="20" font-family="sans-serif">Bank: Bangkok Bank</text><text x="40" y="160" font-size="20" font-family="sans-serif">Branch: Silom</text><text x="40" y="200" font-size="20" font-family="sans-serif">Account: 123-4-56789-0</text><text x="40" y="240" font-size="20" font-family="sans-serif">Name: ${tag}</text></svg>`);
}

async function upload(req: APIRequestContext, opts: UploadSlipOptions = {}): Promise<APIResponse> {
  return await uploadSlip(req, opts) as unknown as APIResponse;
}

async function signIn(page: Page): Promise<void> {
  await signInOnPage(page, PERSONAS.officerEmp001.code);
}

async function mockReceipt(page: Page, tag: string): Promise<void> {
  const uid = await userIdByCode(PERSONAS.officerEmp001.code);
  await page.route('**/api/upload', async (route) => {
    const parsed = {
      vendorName: `${tag}_OCR`,
      vendorAddress: '',
      createdTo: '',
      createdToAddress: '',
      transactionDate: '2026-07-18',
      subtotal: 100,
      vatAmount: 7,
      totalAmount: 107,
      paymentMethod: 'cash',
      currency: 'THB',
      isCorrupted: false,
      correctionNotes: '',
      items: [{ description: 'Original OCR item', qty: 1, unitPrice: 100, amount: 100 }],
      e2eTag: tag,
    };
    const slip = await q1<{ id: number }>(
      `INSERT INTO slips (
         file_path, mime_type, file_size, ocr_raw_json, ocr_confidence,
         uploaded_by, status, kind
       ) VALUES ($1, 'image/svg+xml', $2, $3::jsonb, 1, $4, 'pending', 'receipt')
       RETURNING id`,
      [`e2e/${tag}.svg`, receipt(tag).length, JSON.stringify(parsed), uid],
    );
    if (!slip) throw new Error('Failed to insert E2E slip');
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        slipId: slip.id,
        status: 'pending',
        fileKey: `e2e/${tag}.svg`,
        fileUrl: `/api/slips/file?key=e2e%2F${tag}.svg`,
        mime: 'image/svg+xml',
        size: receipt(tag).length,
        parsed,
        confidence: 1,
        mode: 'receipt:e2e',
        modelName: 'e2e',
        kind: 'receipt',
        validation: { ok: true, errors: [], warnings: [], retried: false, summary: 'ok' },
      }),
    });
  });
}

async function extract(page: Page, tag: string): Promise<number> {
  const done = page.waitForResponse((r) => new URL(r.url()).pathname === '/api/upload' && r.request().method() === 'POST');
  await page.locator('input[type="file"]').first().setInputFiles({
    name: `${tag}.svg`,
    mimeType: 'image/svg+xml',
    buffer: receipt(tag),
  });
  const res = await done;
  expect(res.status()).toBe(200);
  const body = await res.json() as UploadBody;
  const vendor = page.getByTestId('slip-field-vendor');
  await expect(vendor).toBeEnabled();
  await expect(vendor).toHaveValue(`${tag}_OCR`);
  return body.slipId;
}

async function ready(page: Page, tag: string): Promise<void> {
  await page.getByTestId('slip-field-vendor').fill(tag);
  await page.getByTestId('slip-field-date').fill('2026-07-18');
  await page.getByTestId('slip-field-subtotal').fill('100');
  await page.getByTestId('slip-field-vat').fill('7');
  await page.getByTestId('slip-field-total').fill('107');
}

test.describe('POST /api/upload', () => {
  test.describe.configure({ mode: 'serial', timeout: 300_000 });
  test.use({ actionTimeout: 240_000 });

  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('accepts a receipt slip', async ({ page }) => {
    const tag = await vendorTag();
    const res = await upload(page.request, {
      fileName: `${tag}.svg`,
      content: receipt(tag),
    });
    expect(res.status()).toBe(200);
    const body = await res.json() as UploadBody;
    expect(body.slipId).toEqual(expect.any(Number));
    expect(body.parsed).toEqual(expect.any(Object));
    expect(body.fileKey).toEqual(expect.any(String));
    expect(body.validation.ok).toBe(true);
  });

  test('rejects missing file', async ({ page }) => {
    const res = await page.request.post('/api/upload', { multipart: { kind: 'receipt' } });
    expect(res.status()).toBe(400);
    await expect(res.json()).resolves.toEqual(expect.objectContaining({ error: expect.any(String) }));
  });

  test('rejects bad target_type', async ({ page }) => {
    const res = await page.request.post('/api/upload', {
      multipart: {
        file: { name: 'E2E_bad_target.svg', mimeType: 'image/svg+xml', buffer: receipt('E2E_BAD_TARGET') },
        kind: 'receipt',
        target_type: 'foo',
      },
    });
    expect(res.status()).toBe(400);
    await expect(res.json()).resolves.toEqual(expect.objectContaining({ error: expect.stringContaining('target_type') }));
  });

  test('rejects missing target_id for pr', async ({ page }) => {
    const res = await page.request.post('/api/upload', {
      multipart: {
        file: { name: 'E2E_missing_target.svg', mimeType: 'image/svg+xml', buffer: receipt('E2E_MISSING_TARGET') },
        kind: 'receipt',
        target_type: 'pr',
      },
    });
    expect(res.status()).toBe(400);
    await expect(res.json()).resolves.toEqual(expect.objectContaining({ error: expect.stringContaining('target_id') }));
  });

  test('accepts a book_bank slip', async ({ page }) => {
    const tag = await vendorTag();
    const res = await upload(page.request, {
      kind: 'book_bank',
      fileName: `${tag}.svg`,
      content: bank(tag),
    });
    expect(res.status()).toBe(200);
    const body = await res.json() as UploadBody;
    expect(body.parsed).toEqual(expect.objectContaining({
      bankName: expect.any(String),
      bankBranch: expect.any(String),
      accountNumber: expect.any(String),
      accountName: expect.any(String),
    }));
  });

  test('requires finance:expense:create::allow', async ({ page }) => {
    await signOut(page.request);
    const res = await upload(page.request, {
      fileName: 'E2E_signed_out.svg',
      content: receipt('E2E_SIGNED_OUT'),
    });
    expect(res.status()).toBe(401);
  });
});

test.describe('slip UI', () => {
  test.describe.configure({ mode: 'serial', timeout: 120_000 });
  let tag = '';

  test.beforeEach(async ({ page }) => {
    tag = await vendorTag();
    await signIn(page);
    await mockReceipt(page, tag);
    await page.goto('/expense?scope=mine');
    await expect(page.getByTestId('slip-drop-zone').first()).toBeVisible();
  });

  test.afterEach(async () => {
    await cleanupTestTag(tag);
    tag = '';
  });

  test('drop zone + automatic extraction + confirm', async ({ page }) => {
    await extract(page, tag);
    await ready(page, tag);
    await page.locator('summary[title="OCR line items"]').click();
    await page.locator('input[placeholder="Description"]').fill('Edited line item');
    const own = page.getByTestId('slip-confirm');
    if (await own.count()) {
      await expect(own).toBeEnabled();
      await own.click();
    } else {
      const sticky = page.getByTestId('expense-sticky-submit');
      await expect(sticky).toBeEnabled();
      await sticky.click();
    }
    await page.waitForURL(/\/waybill\/WB-\d{4}-\d{6}(?:\?.*)?$/);
    const id = page.url().match(/WB-\d{4}-\d{6}/)?.[0];
    expect(id).toBeTruthy();
    const row = await q1<{ id: string; origin: string; vendor_name: string; item_description: string }>(
      `SELECT w.id, w.origin, e.vendor_name,
              (SELECT description FROM expense_items WHERE expense_id = e.id ORDER BY id LIMIT 1) AS item_description
         FROM waybills w
         JOIN expenses e ON e.id = w.origin_id
        WHERE w.id = $1 AND w.origin = 'expense'`,
      [id],
    );
    expect(row).toEqual(expect.objectContaining({ id, origin: 'expense', vendor_name: tag, item_description: 'Edited line item' }));
    await expect(page.getByTestId('expense-submit-success')).toBeVisible();
  });

  test('total not equal to subtotal plus VAT blocks submit', async ({ page }) => {
    await extract(page, tag);
    await ready(page, tag);
    await page.getByTestId('slip-field-total').fill('999');
    await expect(page.getByTestId('expense-sticky-submit')).toBeDisabled();
  });

  test('transfer payment requires book bank slip', async ({ page }) => {
    await extract(page, tag);
    await ready(page, tag);
    await expect(page.getByTestId('expense-book-bank-section')).toBeHidden();
    await page.getByTestId('slip-payment-credit_card').click();
    await expect(page.getByTestId('expense-book-bank-section')).toBeHidden();
    await page.getByTestId('slip-payment-transfer').click();
    await expect(page.getByTestId('expense-sticky-submit')).toBeDisabled();
    await expect(page.getByTestId('expense-book-bank-section')).toBeVisible();
    await expect(page.getByLabel('Add book bank slip')).toBeVisible();
  });

  test('discard pending slip', async ({ page }) => {
    const id = await extract(page, tag);
    await page.getByTestId('slip-remove').click();
    await expect(page.getByTestId('slip-drop-zone').first()).toBeVisible();
    await expect.poll(async () => await q1(`SELECT id FROM slips WHERE id = $1`, [id])).toBeNull();
  });
});
