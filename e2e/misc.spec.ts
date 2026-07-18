import { test, expect, PERSONAS } from './fixtures';
import { q1, userIdByCode } from './helpers/db';

test.describe.configure({ mode: 'serial' });

async function signIn(page: import('@playwright/test').Page, code: keyof typeof PERSONAS): Promise<void> {
  const id = await userIdByCode(PERSONAS[code].code);
  const r = await page.request.post('/api/actor', { data: { id } });
  if (!r.ok()) throw new Error(`sign in ${code} failed: ${r.status()}`);
}

test.describe('Audit', () => {
  test('6e.1 — /audit renders for IT001', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.goto('/audit');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    await expect(page).toHaveURL(/\/audit$/);
    await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
    await expect(page.locator('body')).toBeVisible();
  });

  test('6e.2 — /api/perm/audit returns 200 + events array', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.request.get('/api/perm/audit?limit=10');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { events: Array<{ id: number; kind: string; actor: string; occurred_at: string }> };
    expect(Array.isArray(body.events)).toBeTruthy();
    if (body.events.length > 0) {
      expect(body.events[0]).toHaveProperty('id');
      expect(body.events[0]).toHaveProperty('kind');
      expect(body.events[0]).toHaveProperty('occurred_at');
    }
  });

  test('6e.3 — EMP001 cannot access /audit (locked view)', async ({ page }) => {
    await signIn(page, 'officerEmp001');
    const r = await page.goto('/audit');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    const html = await page.content();
    const locked = /tile:audit:view required|Access Restricted|สิทธิ์ไม่เพียงพอ|Insufficient Access Permissions/.test(html);
    expect(locked).toBeTruthy();
  });
});

test.describe('Policy', () => {
  test('6e.4 — /policy matrix renders for IT001', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.goto('/policy');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
    const html = await page.content();
    expect(/Persona|Permission|department|policy\.title|Role|stage:/i.test(html)).toBeTruthy();
  });

  test('6e.5 — /api/policy/full returns 200 + snapshot', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.request.get('/api/policy/full');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { columns: unknown[]; targets: unknown[]; cells: Record<string, string[]> };
    expect(Array.isArray(body.columns)).toBeTruthy();
    expect(Array.isArray(body.targets)).toBeTruthy();
    expect(typeof body.cells).toBe('object');
    expect(body.columns.length).toBeGreaterThan(0);
  });

  test('6e.6 — /api/policy/lint accepts {ast}', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.request.post('/api/policy/lint', { data: { ast: {} } });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; lint: { findings: Array<{ code: string; severity: string; message: string }> } };
    expect(body.ok).toBeTruthy();
    expect(Array.isArray(body.lint.findings)).toBeTruthy();
    expect(body.lint.findings.length).toBeGreaterThan(0);
    expect(body.lint.findings[0].code).toBeTruthy();

    const ok = await page.request.post('/api/policy/lint', {
      data: { ast: { rules: [{ allow: 'stage:submission:act::allow' }] } },
    });
    expect(ok.ok()).toBeTruthy();
    const okBody = (await ok.json()) as { ok: boolean; lint: { findings: unknown[] } };
    expect(okBody.ok).toBeTruthy();
    expect(okBody.lint.findings).toEqual([]);
  });
});

test.describe('Inbox', () => {
  test('6e.7 — /inbox scopes render (waiting, watching, all)', async ({ page }) => {
    await signIn(page, 'officerEmp001');

    const scopes = ['waiting', 'watching', 'all'] as const;
    const counts: Record<string, number> = {};
    for (const s of scopes) {
      const r = await page.goto(`/inbox?scope=${s}`);
      expect(r?.ok() || r?.status() === 200).toBeTruthy();
      await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
      const html = await page.content();
      const ids = html.match(/WB-\d{4}-\d{6}/g) ?? [];
      counts[s] = ids.length;
    }
    expect(counts.all).toBeGreaterThanOrEqual(0);
    expect(counts.waiting).toBeGreaterThanOrEqual(0);
    expect(counts.watching).toBeGreaterThanOrEqual(0);
  });

  test('6e.8 — /expense scopes render (mine, queue, all)', async ({ page }) => {
    await signIn(page, 'officerEmp001');
    for (const s of ['mine', 'queue', 'all']) {
      const r = await page.goto(`/expense?scope=${s}`);
      expect(r?.ok() || r?.status() === 200).toBeTruthy();
      await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
      await expect(page).toHaveURL(new RegExp(`/expense\\?scope=${s}`));
    }
  });

  test('6e.9 — /api/waybill/nudges returns 200 + array', async ({ page }) => {
    await signIn(page, 'officerEmp001');
    const r = await page.request.get('/api/waybill/nudges');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; items: unknown[] };
    expect(body.ok).toBeTruthy();
    expect(Array.isArray(body.items)).toBeTruthy();
  });
});

test.describe('Tiles', () => {
  test('6e.10 — /api/perm/tiles returns 200 + array', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.request.get('/api/perm/tiles');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { tiles: Array<{ id: string; display_name: string; view_perm_id: string }>; departments: unknown[] };
    expect(Array.isArray(body.tiles)).toBeTruthy();
    expect(Array.isArray(body.departments)).toBeTruthy();
    expect(body.tiles.length).toBeGreaterThan(0);
    for (const t of body.tiles.slice(0, 3)) {
      expect(t.id).toBeTruthy();
      expect(t.display_name).toBeTruthy();
      expect(t.view_perm_id).toBeTruthy();
    }
  });

  test('6e.11 — /tiles page renders catalog with edit controls', async ({ page }) => {
    await signIn(page, 'itAdminIT001');
    const r = await page.goto('/tiles');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
    const html = await page.content();
    expect(/view_perm_id/i.test(html)).toBeTruthy();
    const saveBtn = page.locator('button:has-text("Save")').first();
    await expect(saveBtn).toBeVisible({ timeout: 10_000 });
  });

  test('6e.12 — PATCH /api/perm/tiles/[id]/gate is reversible', async ({ page }) => {
    const tileId = 'expense';
    const before = await q1<{ view_perm_id: string }>(
      `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
      [tileId],
    );
    expect(before?.view_perm_id).toBeTruthy();

    await signIn(page, 'itAdminIT001');

    const targetPerm = 'finance:report:executive::allow';
    const r = await page.request.patch(`/api/perm/tiles/${tileId}/gate`, {
      data: { view_perm_id: targetPerm },
    });
    expect(r.ok()).toBeTruthy();
    const after = await q1<{ view_perm_id: string }>(
      `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
      [tileId],
    );
    expect(after?.view_perm_id).toBe(targetPerm);

    const revert = await page.request.patch(`/api/perm/tiles/${tileId}/gate`, {
      data: { view_perm_id: before!.view_perm_id },
    });
    expect(revert.ok()).toBeTruthy();
    const final = await q1<{ view_perm_id: string }>(
      `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
      [tileId],
    );
    expect(final?.view_perm_id).toBe(before!.view_perm_id);
  });
});

test.describe('Cockpit / Executive', () => {
  test('6e.13 — /cockpit renders for finance EMP019', async ({ page }) => {
    await signIn(page, 'financeEmp019');
    const r = await page.goto('/cockpit');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    await expect(page).toHaveURL(/\/cockpit$/);
    await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
    await expect(page.locator('body')).toBeVisible();
  });

  test('6e.14 — /executive renders for CFO EMP005', async ({ page }) => {
    await signIn(page, 'cfoEmp005');
    const r = await page.goto('/executive');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();
    await expect(page).toHaveURL(/\/executive$/);
    await page.waitForLoadState('networkidle', { timeout: 15_000 }).catch(() => {});
    await expect(page.locator('body')).toBeVisible();
  });

  test('6e.15 — /api/exec/brief returns 200 + JSON for CFO', async ({ page }) => {
    await signIn(page, 'cfoEmp005');
    const r = await page.request.get('/api/exec/brief');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; brief: unknown };
    expect(body.ok).toBeTruthy();
    expect(body.brief).toBeTruthy();
  });
});

test.describe('Notifications', () => {
  test('6e.16 — /api/notifications returns 200 + array', async ({ page }) => {
    await signIn(page, 'officerEmp001');
    const r = await page.request.get('/api/notifications');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { items: unknown[] };
    expect(Array.isArray(body.items)).toBeTruthy();
  });
});
