import { test, expect } from './fixtures';
import { exec, q1 } from './helpers/db';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';

const SAMPLE_PDF = path.resolve(__dirname, '../sample/law/1_Non_Disclosure_Agreement.pdf');

const uploaded: string[] = [];

async function uploadPdf(req: APIRequestContext, filePath: string): Promise<string> {
  const buf = await readFile(filePath);
  const r = await req.post('/api/law/upload', {
    multipart: {
      file: { name: path.basename(filePath), mimeType: 'application/pdf', buffer: buf },
    },
  });
  if (!r.ok()) throw new Error(`upload failed: ${r.status()} ${await r.text()}`);
  const body = (await r.json()) as { contractId?: string };
  if (!body.contractId) throw new Error('upload missing contractId');
  uploaded.push(body.contractId);
  return body.contractId;
}

async function deleteContract(req: APIRequestContext, id: string): Promise<void> {
  const r = await req.delete(`/api/law/contracts?id=${id}`);
  if (!r.ok()) throw new Error(`delete failed: ${r.status()} ${await r.text()}`);
}

function pageReq(page: Page): APIRequestContext {
  return page.request;
}

test.afterEach(async () => {
  for (const id of uploaded.splice(0)) {
    await exec(`DELETE FROM law.contract_pages WHERE contract_id = $1::uuid`, [id]).catch(() => {});
    await exec(`DELETE FROM law.contract_chunks WHERE contract_id = $1::uuid`, [id]).catch(() => {});
    await exec(`DELETE FROM law.job_queue WHERE contract_id = $1::uuid`, [id]).catch(() => {});
    await exec(`DELETE FROM law.contracts WHERE id = $1::uuid`, [id]).catch(() => {});
  }
});

test.describe.configure({ mode: 'serial' });

test.describe('Law', () => {
  test('6b.1 LW001 sees the law contract list page', async ({ page, persona }) => {
    await persona('lawParalegalLW001');
    const res = await page.goto('/law');
    expect(res?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/law$/);
    await expect(page.getByRole('heading', { name: /Law documents/ })).toBeVisible();
    const body = await page.locator('body').innerText();
    expect(body).toMatch(/Total|Ready|Failed|Pending|Contracts/);
  });

  test('6b.2 IT001 sees the law upload page (file picker + submit)', async ({ page, persona }) => {
    await persona('itAdminIT001');
    const res = await page.goto('/law/upload');
    expect(res?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/law\/upload$/);
    const fileInput = page.locator('input[type="file"]');
    await expect(fileInput).toBeAttached();
    await expect(fileInput).toHaveAttribute('accept', /pdf/i);
    const submit = page.locator('button', { hasText: /Upload contract/ });
    await expect(submit).toBeVisible();
    await expect(submit).toBeDisabled();
  });

  test('6b.3 IT001 uploads a contract via /api/law/upload and gets a contractId', async ({ page, persona }) => {
    await persona('itAdminIT001');
    const contractId = await uploadPdf(pageReq(page), SAMPLE_PDF);
    expect(contractId).toMatch(/^[0-9a-f-]{36}$/);
    const row = await q1<{ id: string; file_name: string; status: string }>(
      `SELECT id, file_name, status FROM law.contracts WHERE id = $1::uuid`,
      [contractId],
    );
    expect(row).toBeTruthy();
    expect(row?.file_name).toBe('1_Non_Disclosure_Agreement.pdf');
  });

  test('6b.4 IT001 sees the law admin page', async ({ page, persona }) => {
    await persona('itAdminIT001');
    const res = await page.goto('/law/admin');
    expect(res?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/law\/admin$/);
    await expect(page.getByText(/Recent activity/)).toBeVisible();
  });

  test('6b.5 /api/law/rag answers a query', async ({ page, persona }) => {
    await persona('itAdminIT001');
    const r = await pageReq(page).post('/api/law/rag', {
      data: { query: 'termination clause', topK: 3 },
      timeout: 120_000,
    });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; answer?: string; sources?: unknown[] };
    expect(body.ok).toBeTruthy();
    expect(typeof body.answer).toBe('string');
    expect(Array.isArray(body.sources)).toBeTruthy();
  });

  test('6b.6 contract detail sub-tabs render', async ({ page, persona }) => {
    await persona('itAdminIT001');
    const contractId = await uploadPdf(pageReq(page), SAMPLE_PDF);

    await page.goto(`/law/${contractId}`);
    await expect(page).toHaveURL(new RegExp(`/law/${contractId}`));

    const tabs = ['parties', 'chunks', 'metadata', 'audit', 'chat'];
    for (const tab of tabs) {
      await page.goto(`/law/${contractId}/${tab}`);
      await expect(page).toHaveURL(new RegExp(`/law/${contractId}/${tab}$`));
      const link = page.locator(`a[href="/law/${contractId}/${tab}"]`).first();
      await expect(link).toBeVisible();
    }
  });

  test('6b.7 EMP001 is redirected from /law/upload to /forbidden', async ({ page, persona }) => {
    await persona('officerEmp001');
    await page.goto('/law/upload');
    await expect(page).toHaveURL(/\/forbidden/);
  });
});

void deleteContract;
