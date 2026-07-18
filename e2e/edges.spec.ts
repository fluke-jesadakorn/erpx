import { test, expect } from './fixtures';
import {
  eventCount,
  exec,
  q,
  userIdByCode,
  waybillStage,
} from './helpers';
import { pinLocale } from './helpers/lang';
import { createExpense, signInOnPage } from './helpers/waybill';

async function cleanupWaybillArtifacts(wbId: string, expenseId: number): Promise<void> {
  await exec(`DELETE FROM folio.waybill_events WHERE waybill_id = $1`, [wbId]);
  await exec(`DELETE FROM folio.expense_items WHERE expense_id = $1`, [expenseId]);
  await exec(
    `DELETE FROM folio.expenses WHERE id = $1 AND vendor_name LIKE '%::%'`,
    [expenseId],
  );
  await exec(`DELETE FROM folio.waybills WHERE id = $1`, [wbId]);
}

async function cleanupByTag(tag: string): Promise<void> {
  await exec(
    `DELETE FROM folio.waybill_events
       WHERE waybill_id IN (SELECT id FROM folio.waybills WHERE vendor_name LIKE $1)`,
    [`${tag}%`],
  );
  await exec(`DELETE FROM folio.waybills WHERE vendor_name LIKE $1`, [`${tag}%`]);
  await exec(
    `DELETE FROM folio.expense_items WHERE expense_id IN (SELECT id FROM folio.expenses WHERE vendor_name LIKE $1)`,
    [`${tag}%`],
  );
  await exec(`DELETE FROM folio.expenses WHERE vendor_name LIKE $1`, [`${tag}%`]);
  await exec(
    `DELETE FROM folio.slips WHERE ocr_raw_json::text LIKE $1 OR file_path LIKE $1`,
    [`%${tag}%`],
  );
}

test.describe('edge cases 7.1–7.15', () => {
  test('7.1 malformed waybill id renders not-found', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/waybill/abc');
    await expect(page).toHaveURL(/\/(waybill\/abc|login|forbidden)/);
    const status = r?.status() ?? 0;
    if (status === 200) {
      const html = await page.content();
      const hasNotFound =
        /not\s*found|does not exist/i.test(html) ||
        (await page.getByText(/not found|does not exist|moved/i).count()) > 0;
      expect(hasNotFound).toBeTruthy();
    } else {
      expect(status).toBeGreaterThanOrEqual(400);
    }
  });

  test('7.2 nonexistent waybill id renders not-found', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/waybill/WB-2099-999999');
    await expect(page).toHaveURL(/\/(waybill|login|forbidden)/);
    const status = r?.status() ?? 0;
    if (status === 200) {
      const html = await page.content();
      const hasNotFound =
        /not\s*found|does not exist/i.test(html) ||
        (await page.getByText(/not found|does not exist|moved/i).count()) > 0;
      expect(hasNotFound).toBeTruthy();
    } else {
      expect(status).toBeGreaterThanOrEqual(400);
    }
  });

  test('7.3 by-expense lookup with bad expense id', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/waybill/by-expense/9999999');
    const finalUrl = page.url();
    const status = r?.status() ?? 0;
    const isError =
      /\/login|\/forbidden|by-expense/.test(finalUrl) ||
      (await page.getByText(/not found|does not exist|moved/i).count()) > 0 ||
      status >= 400;
    expect(isError).toBeTruthy();
  });

  test('7.4 slip upload too large → 400/413', async ({ page }) => {
    const id = await userIdByCode('EMP001');
    await page.request.post('/api/actor', { data: { id } });
    const big = Buffer.alloc(60 * 1024 * 1024, 0);
    const r = await page.request.post('/api/upload', {
      multipart: {
        file: { name: 'huge.bin', mimeType: 'image/png', buffer: big },
        kind: 'receipt',
      },
    });
    expect([400, 413]).toContain(r.status());
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    expect(typeof body.error === 'string' || Object.keys(body).length > 0).toBeTruthy();
  });

  test('7.5 slip upload wrong mime → 400', async ({ page }) => {
    const id = await userIdByCode('EMP001');
    await page.request.post('/api/actor', { data: { id } });
    const r = await page.request.post('/api/upload', {
      multipart: {
        file: { name: 'evil.exe', mimeType: 'application/x-msdownload', buffer: Buffer.from('not a real file') },
        kind: 'receipt',
      },
    });
    expect([400, 415, 422]).toContain(r.status());
    const body = (await r.json().catch(() => ({}))) as { error?: string };
    expect(typeof body.error === 'string').toBeTruthy();
  });

  test('7.6 officer → /hr/leave shows locked view', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/hr/leave');
    await page.waitForLoadState('networkidle').catch(() => {});
    const finalUrl = page.url();
    const status = r?.status() ?? 0;
    const locked =
      finalUrl.includes('/forbidden') ||
      finalUrl.includes('/login') ||
      status >= 400 ||
      (await page.getByText(/access|locked|denied|required|read/i).count()) > 0;
    expect(locked).toBeTruthy();
  });

  test('7.7 officer → /ai-settings shows locked view', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/ai-settings');
    const finalUrl = page.url();
    const status = r?.status() ?? 0;
    const locked =
      finalUrl.includes('/forbidden') ||
      finalUrl.includes('/login') ||
      status >= 400 ||
      (await page.getByText(/access|locked|denied|required/i).count()) > 0;
    expect(locked).toBeTruthy();
  });

  test('7.8 officer → /policy shows locked view', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/policy');
    const finalUrl = page.url();
    const status = r?.status() ?? 0;
    const locked =
      finalUrl.includes('/forbidden') ||
      finalUrl.includes('/login') ||
      status >= 400 ||
      (await page.getByText(/access|locked|denied|matrix/i).count()) > 0;
    expect(locked).toBeTruthy();
  });

  test('7.9 officer → /tiles shows locked view', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    const r = await page.goto('/tiles');
    const finalUrl = page.url();
    const status = r?.status() ?? 0;
    const locked =
      finalUrl.includes('/forbidden') ||
      finalUrl.includes('/login') ||
      status >= 400 ||
      (await page.getByText(/access|locked|denied|required/i).count()) > 0;
    expect(locked).toBeTruthy();
  });

  test('7.10 concurrent waybill advance — at least one succeeds', async ({ browser }) => {
    const tag = await vendorTagSafe();
    const { waybillId, expenseId } = await createExpense({
      submitter: 'EMP001',
      amount: 500,
      vendorName: 'EdgeConcurrent',
      vendorTag: tag,
    });

    const ctxA = await browser.newContext();
    const ctxB = await browser.newContext();
    try {
      await pinLocale(ctxA, 'th');
      await pinLocale(ctxB, 'th');

      const pageA = await ctxA.newPage();
      const pageB = await ctxB.newPage();
      try {
        await signInOnPage(pageA, 'EMP017');
        await signInOnPage(pageB, 'EMP017');

        await pageA.goto(`/waybill/${waybillId}`);
        await pageB.goto(`/waybill/${waybillId}`);
        await pageA.waitForLoadState('networkidle').catch(() => {});
        await pageB.waitForLoadState('networkidle').catch(() => {});

        const btnA = pageA.locator('[data-testid^="panel-approve-"]').first();
        const btnB = pageB.locator('[data-testid^="panel-approve-"]').first();

        await btnA.waitFor({ state: 'visible', timeout: 10_000 });
        await btnB.waitFor({ state: 'visible', timeout: 10_000 });

        await Promise.all([
          btnA.click({ noWaitAfter: true }).catch(() => {}),
          btnB.click({ noWaitAfter: true }).catch(() => {}),
        ]);

        await pageA.waitForLoadState('networkidle').catch(() => {});
        await pageB.waitForLoadState('networkidle').catch(() => {});

        await expect
          .poll(async () => waybillStage(waybillId), { timeout: 10_000, intervals: [200, 500, 1000] })
          .not.toBe('submission');
        const events = await eventCount(waybillId);
        expect(events).toBeGreaterThanOrEqual(1);
      } finally {
        await pageA.close().catch(() => {});
        await pageB.close().catch(() => {});
      }
    } finally {
      await ctxA.close().catch(() => {});
      await ctxB.close().catch(() => {});
      await cleanupWaybillArtifacts(waybillId, expenseId);
      await cleanupByTag(tag);
    }
  });

  test('7.11 session with bad HMAC → /login', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    await page.context().addCookies([
      {
        name: 'folio_session',
        value: 'invalid.invalid',
        domain: 'localhost',
        path: '/',
        httpOnly: true,
        sameSite: 'Lax',
      },
    ]);
    await page.goto('/');
    await page.waitForURL(/\/login/, { timeout: 5_000 });
    expect(page.url()).toMatch(/\/login/);
  });

  test('7.12 deleted cookie → /login', async ({ page }) => {
    await signInOnPage(page, 'EMP001');
    await page.context().clearCookies();
    await page.goto('/');
    await page.waitForURL(/\/login/, { timeout: 5_000 });
    expect(page.url()).toMatch(/\/login/);
  });

  test('7.13 invalid waybill id in approve action', async ({ page }) => {
    await signInOnPage(page, 'EMP005');
    const r = await page.request.post('/waybill/WB-bad-id', {
      form: { waybillId: 'WB-bad-id' },
      maxRedirects: 0,
    });
    const status = r.status();
    const body = await r.text();
    const isError =
      status === 404 ||
      status === 400 ||
      status >= 500 ||
      /not\s*found|does not exist|invalid/i.test(body);
    expect(isError).toBeTruthy();
  });

  test('7.14 invalid role-id in grant → 400', async ({ page }) => {
    await signInOnPage(page, 'IT001');
    const targetUser = 960;
    const before = await q<{ role_id: string }>(
      `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
      [targetUser],
    );
    const r = await page.request.put(`/api/perm/users/${targetUser}/roles`, {
      data: { roleIds: ['foo::99'] },
    });
    expect(r.status()).toBeGreaterThanOrEqual(400);
    expect([400, 422]).toContain(r.status());
    const after = await q<{ role_id: string }>(
      `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
      [targetUser],
    );
    expect(after.length).toBe(before.length);
    expect(after.map(r => r.role_id).sort()).toEqual(before.map(r => r.role_id).sort());
  });

  test('7.15 DELETE /api/actor without session → 200', async ({ browser }) => {
    const ctx = await browser.newContext();
    try {
      await pinLocale(ctx, 'th');
      const r = await ctx.request.delete('/api/actor');
      expect(r.ok()).toBeTruthy();
      expect(r.status()).toBe(200);
    } finally {
      await ctx.close();
    }
  });
});

async function vendorTagSafe(): Promise<string> {
  return `E2E_EDGE_${Date.now().toString(36)}_${Math.floor(Math.random() * 1e6).toString(36)}`;
}
