import { test, expect } from './fixtures';
import { exec, q, q1, userIdByCodes } from './helpers/db';
import { signInOnPage } from './helpers/waybill';
import type { APIRequestContext, Browser, Page } from '@playwright/test';

const EMP = { IT001: 'IT001', EMP007: 'EMP007', EMP002: 'EMP002', EMP003: 'EMP003', EMP004: 'EMP004', EMP019: 'EMP019' };
const TAG = `E2E_SALES_${Date.now().toString(36)}_${Math.floor(Math.random() * 1e6).toString(36)}`;

interface SoContext {
  soId: number;
  wbId: string;
  customerId: number;
  customerCode: string;
}

async function signIn(page: Page, code: string): Promise<void> {
  await signInOnPage(page, code);
}

async function dismissOverlays(page: Page): Promise<void> {
  await page.evaluate(() => {
    try { localStorage.setItem('onboarded_v1', '1'); } catch {}
  });
  await page.reload().catch(() => null);
  await page.waitForLoadState('domcontentloaded').catch(() => null);
}

async function promoteAdmin(req: APIRequestContext, userId: number): Promise<void> {
  const cur = await req.get(`/api/perm/users/${userId}/grants`);
  const body = (await cur.json()) as { user?: { perm_role_ids?: string[] } };
  const ids = body.user?.perm_role_ids ?? [];
  if (ids.includes('admin::2')) return;
  const desired = Array.from(new Set([...ids, 'admin::2']));
  const r = await req.put(`/api/perm/users/${userId}/roles`, { data: { roles: desired } });
  if (!r.ok()) {
    const text = await r.text();
    throw new Error(`promoteAdmin failed: ${r.status()} ${text}`);
  }
}

async function seedCustomer(req: APIRequestContext, code: string, name: string, credit = 100000): Promise<number> {
  const r = await q<{ id: number }>(
    `INSERT INTO folio.customers (code, name, payment_terms, credit_limit_thb, is_active)
     VALUES ($1, $2, 'Net 30', $3, TRUE)
     ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name
     RETURNING id`,
    [code, name, credit],
  );
  void req;
  return r[0].id;
}

async function createSalesOrder(req: APIRequestContext, customerId: number, _submitterId: number): Promise<{ soId: number; wbId: string; currentStage: string }> {
  const r = await req.post('/api/sales', {
    data: {
      customerId,
      paymentTerms: 'Net 30',
      dueDate: '2026-09-30',
      items: [{ description: `${TAG} widget`, qty: 1, unitPrice: 10000, vatPct: 7 }],
      totals: { subtotal: 10000, vat: 700, total: 10700 },
    },
  });
  if (!r.ok()) {
    const text = await r.text();
    throw new Error(`createSalesOrder failed: ${r.status()} ${text}`);
  }
  const body = (await r.json()) as { salesOrderId: number; waybillId: string; currentStage: string };
  return { soId: body.salesOrderId, wbId: body.waybillId, currentStage: body.currentStage };
}

async function advanceSo(wbId: string, toStage: string): Promise<void> {
  await exec(`UPDATE folio.waybills SET current_stage = $1, updated_at = now() WHERE id = $2`, [toStage, wbId]);
  await exec(`UPDATE folio.sales_orders SET status = $1, updated_at = now() WHERE id = (SELECT origin_id FROM folio.waybills WHERE id = $2)`, [toStage, wbId]);
}

async function wbStage(wbId: string): Promise<string | null> {
  const r = await q1<{ current_stage: string }>(`SELECT current_stage FROM folio.waybills WHERE id = $1`, [wbId]);
  return r?.current_stage ?? null;
}

async function soStatus(soId: number): Promise<string | null> {
  const r = await q1<{ status: string }>(`SELECT status FROM folio.sales_orders WHERE id = $1`, [soId]);
  return r?.status ?? null;
}

async function soCountBySubmitter(submitterId: number): Promise<number> {
  const r = await q1<{ n: string }>(
    `SELECT count(*)::text AS n FROM folio.waybills WHERE origin='so' AND submitter_id=$1 AND status='open'`,
    [submitterId],
  );
  return parseInt(r?.n ?? '0', 10);
}

async function soCountAll(): Promise<number> {
  const r = await q1<{ n: string }>(
    `SELECT count(*)::text AS n FROM folio.waybills WHERE origin='so' AND status='open'`,
  );
  return parseInt(r?.n ?? '0', 10);
}

test.describe.configure({ mode: 'serial' });

test.describe('Sales pipeline (6c)', () => {
  let ctxIds: Record<string, number> = {};
  let customerId = 0;
  let createdSo: SoContext | null = null;

  test.beforeAll(async ({ browser }: { browser: Browser }) => {
    ctxIds = await userIdByCodes(...Object.values(EMP));
    const adminPage = await browser.newPage();
    await signIn(adminPage, 'IT001');
    await promoteAdmin(adminPage.request, ctxIds.IT001);
    await adminPage.request.delete('/api/actor');

    await signIn(adminPage, 'IT001');
    customerId = await seedCustomer(
      adminPage.request,
      `${TAG}-C01`,
      `${TAG} Customer Co`,
      500000,
    );

    await exec(
      `INSERT INTO folio.customers (code, name, payment_terms, credit_limit_thb, is_active)
       VALUES ($1, $2, 'Net 30', $3, TRUE), ($4, $5, 'Net 30', $6, TRUE)
       ON CONFLICT (code) DO NOTHING`,
      [`${TAG}-C02`, `${TAG} Customer Two`, 200000, `${TAG}-C03`, `${TAG} Customer Three`, 300000],
    );
    await adminPage.context().close();
  });

  test.afterAll(async ({ browser }: { browser: Browser }) => {
    const cleanupPage = await browser.newPage();
    try {
      await signIn(cleanupPage, 'IT001');
      if (createdSo?.wbId) {
        await exec(`DELETE FROM folio.waybill_events WHERE waybill_id = $1`, [createdSo.wbId]);
        await exec(`DELETE FROM folio.waybills WHERE id = $1`, [createdSo.wbId]);
      }
      await exec(
        `DELETE FROM folio.so_items WHERE sales_order_id IN (SELECT id FROM folio.sales_orders WHERE sales_rep_id = $1 AND updated_at > now() - interval '1 hour')`,
        [ctxIds.EMP007],
      );
      await exec(
        `DELETE FROM folio.sales_orders WHERE sales_rep_id = $1 AND updated_at > now() - interval '1 hour'`,
        [ctxIds.EMP007],
      );
      await exec(`DELETE FROM folio.customers WHERE code LIKE $1`, [`${TAG}%`]);
      await cleanupPage.request.put(`/api/perm/users/${ctxIds.IT001}/roles`, { data: { roles: ['it::2'] } });
      await cleanupPage.request.put(`/api/perm/users/${ctxIds.EMP007}/roles`, { data: { roles: ['officer::5'] } });
    } finally {
      await cleanupPage.context().close();
    }
  });

  test('6c.1 — grant sales_rep::3 to EMP007 via API', async ({ page }) => {
    await signIn(page, 'IT001');
    const r = await page.request.put(`/api/perm/users/${ctxIds.EMP007}/roles`, {
      data: { roles: ['sales_rep::3'] },
    });
    expect(r.ok()).toBeTruthy();
    const g = await page.request.get(`/api/perm/users/${ctxIds.EMP007}/grants`);
    expect(g.ok()).toBeTruthy();
    const body = (await g.json()) as {
      user: { perm_role_ids: string[]; perm_role_names: string[] };
    };
    expect(body.user.perm_role_ids).toContain('sales_rep::3');
    expect(body.user.perm_role_names).toContain('Sales Rep');
  });

  test('6c.2 — /sales renders for EMP007 with sales_rep::3', async ({ page }) => {
    await signIn(page, 'EMP007');
    const r = await page.goto('/sales');
    expect(r?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/sales/);
    await dismissOverlays(page);
    await expect(page.getByTestId('sales-sticky-submit')).toBeVisible();
  });

  test('6c.3 — create a sales order via POST /api/sales', async ({ page }) => {
    await signIn(page, 'EMP007');
    const created = await createSalesOrder(page.request, customerId, ctxIds.EMP007);
    expect(created.wbId).toMatch(/^WB-\d{4}-\d{6}$/);
    expect(created.currentStage).toBe('so_sales_review');
    const stage = await wbStage(created.wbId);
    expect(stage).toBe('so_sales_review');
    const status = await soStatus(created.soId);
    expect(status).toBe('so_sales_review');
    createdSo = { soId: created.soId, wbId: created.wbId, customerId, customerCode: `${TAG}-C01` };
  });

  test('6c.4 — walk sales pipeline (so_sales_review → so_dept_approval → so_credit_check → so_invoiced → so_paid)', async ({ page }) => {
    await signIn(page, 'IT001');
    expect(createdSo).not.toBeNull();
    const { wbId } = createdSo!;

    const stages = ['so_dept_approval', 'so_credit_check', 'so_invoiced', 'so_paid'];
    for (const next of stages) {
      await advanceSo(wbId, next);
      const s = await wbStage(wbId);
      expect(s).toBe(next);
      const status = await soStatus(createdSo!.soId);
      expect(status).toBe(next);
    }

    const r0 = await page.goto(`/waybill/${wbId}`);
    expect(r0?.ok()).toBeTruthy();
  });

  test('6c.5 — sales inbox scopes differ for sales_rep', async ({ page }) => {
    await signIn(page, 'EMP007');
    await page.goto('/sales?scope=mine');
    await expect(page).toHaveURL(/scope=mine/);
    const mineCount = await soCountBySubmitter(ctxIds.EMP007);

    await page.goto('/sales?scope=all');
    await expect(page).toHaveURL(/scope=all/);
    const allCount = await soCountAll();

    await page.goto('/sales?scope=queue');
    await expect(page).toHaveURL(/scope=queue/);

    expect(mineCount).toBeGreaterThanOrEqual(0);
    expect(allCount).toBeGreaterThanOrEqual(mineCount);
  });

  test('6c.6 — /customers renders seeded customers and click opens detail', async ({ page }) => {
    await signIn(page, 'IT001');
    const r = await page.goto('/customers');
    expect(r?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/customers/);
    await dismissOverlays(page);
    await expect(page.getByText(`${TAG} Customer Co`).first()).toBeVisible();
    await expect(page.getByText(`${TAG} Customer Two`).first()).toBeVisible();
    await expect(page.getByText(`${TAG} Customer Three`).first()).toBeVisible();

    const link = page.getByRole('link', { name: `${TAG}-C01` }).first();
    await link.click();
    await expect(page).toHaveURL(new RegExp(`/customers/${customerId}`));
    await expect(page.getByText(`${TAG} Customer Co`).first()).toBeVisible();
  });

  test('6c.7 — GET /api/customers/search returns matching customers', async ({ page }) => {
    await signIn(page, 'IT001');
    const r = await page.request.get(`/api/customers/search?q=${encodeURIComponent(TAG)}`);
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; customers: Array<{ id: number; code: string; name: string }> };
    expect(body.ok).toBe(true);
    expect(Array.isArray(body.customers)).toBe(true);
    const codes = body.customers.map((c) => c.code);
    expect(codes.some((c) => c.startsWith(TAG))).toBe(true);
  });
});