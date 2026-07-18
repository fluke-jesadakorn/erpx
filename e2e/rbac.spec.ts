import { test, expect, PERSONAS } from './fixtures';
import {
  cleanupTestTag,
  eventCount,
  exec,
  q1,
  userIdByCodes,
  vendorTag,
  waybillStage,
} from './helpers/db';
import { createExpense, advanceWaybill } from './helpers/waybill';

const CODES = [
  'EMP001',
  'EMP017',
  'EMP021',
  'IT001',
  'EMP006',
];

const TAG = () => vendorTag();

async function cleanupAll(tag: string): Promise<void> {
  await cleanupTestTag(tag);
  await exec(`DELETE FROM waybills WHERE vendor_tag = $1`, [tag]).catch(() => {});
  await exec(
    `DELETE FROM perm.user_permissions
      WHERE permission_id = 'admin:system:bypass::allow'
        AND granted_by = 'e2e:rbac'`,
  );
}

test.describe.configure({ mode: 'serial' });

test.describe('RBAC permission gates (5.1–5.8)', () => {
  let ids: Record<string, number> = {};
  const originalTiles = new Map<string, string>();

  test.beforeAll(async () => {
    ids = await userIdByCodes(...CODES);
  });

  test.afterAll(async () => {
    for (const [tileId, perm] of originalTiles.entries()) {
      await exec(
        `UPDATE perm.tiles SET view_perm_id = $1, updated_at = now() WHERE id = $2`,
        [perm, tileId],
      );
    }
    await exec(
      `DELETE FROM perm.user_permissions
        WHERE permission_id = 'admin:system:bypass::allow'
          AND granted_by = 'e2e:rbac'`,
    );
    await exec(
      `DELETE FROM perm.audit WHERE actor = 'user:964' AND kind = 'user.perm.permanent.set'`,
    );
  });

  test('5.1 — officer EMP001 cannot see /audit tile (no tile:audit:view perm)', async ({ page }) => {
    await page.request.post('/api/actor', { data: { id: ids.EMP001 } });
    const r = await page.goto('/audit');
    expect(r?.ok() || r?.status() === 200).toBeTruthy();

    const url = page.url();
    const html = await page.content();
    const showsLocked =
      /Insufficient Access Permissions|สิทธิ์ไม่เพียงพอ/.test(html) ||
      /Restricted to specific roles/.test(html);
    const hasNoPermMarker = /tile:audit:view required/.test(html);
    expect(showsLocked && hasNoPermMarker).toBeTruthy();

    const permsRow = await q1<{ perms: string[] }>(
      `SELECT COALESCE(array_agg(DISTINCT p_id), ARRAY[]::text[]) AS perms
         FROM (
           SELECT rp.permission_id AS p_id
             FROM perm.user_roles ur
             JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
            WHERE ur.user_id = $1
           UNION
           SELECT permission_id AS p_id
             FROM perm.user_permissions
            WHERE user_id = $1 AND revoked_at IS NULL
              AND (ends_at IS NULL OR ends_at > now())
         ) t`,
      [ids.EMP001],
    );
    expect(permsRow?.perms ?? []).not.toContain('tile:audit:view::allow');
    void url;
  });

  test('5.2 — officer cannot see reject button on a dept_verification waybill', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACRejectGate',
      vendorTag: tag,
    });
    await advanceWaybill(waybillId, 'dept_verification');

    await page.request.post('/api/actor', { data: { id: ids.EMP001 } });
    await page.goto(`/waybill/${waybillId}?action=reject&stage=dept_verification`);

    expect(await page.locator('textarea[name="reason"]').count()).toBe(0);
    expect(await page.locator('[data-testid^="big-reject-"]').count()).toBe(0);
    expect(await page.locator('[data-testid^="big-approve-"]').count()).toBe(0);

    await cleanupAll(tag);
  });

  test('5.3 — supervisor EMP017 cannot final-approve at accounting_authorization', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACFinalApproveGate',
      vendorTag: tag,
    });
    for (const s of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision', 'accounting_authorization']) {
      await advanceWaybill(waybillId, s);
    }
    expect(await waybillStage(waybillId)).toBe('accounting_authorization');

    await page.request.post('/api/actor', { data: { id: ids.EMP017 } });
    await page.goto(`/waybill/${waybillId}`);

    expect(await page.locator(`[data-testid^="big-final-approve-${waybillId}"]`).count()).toBe(0);
    expect(await page.locator(`[data-testid^="big-final-reject-${waybillId}"]`).count()).toBe(0);

    await cleanupAll(tag);
  });

  test('5.4 — IT001 with admin bypass can act on any stage (submission → approves)', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACITBypass',
      vendorTag: tag,
    });

    await page.request.post('/api/actor', { data: { id: ids.IT001 } });
    const grantRes = await page.request.get(`/api/perm/users/${ids.IT001}/grants`);
    const grantBody = grantRes.ok() ? ((await grantRes.json()) as { active_perm_ids: string[] }) : { active_perm_ids: [] };
    const desired = Array.from(
      new Set([...(grantBody.active_perm_ids ?? []), 'admin:system:bypass::allow']),
    );
    const grant = await page.request.put(`/api/perm/users/${ids.IT001}/grants`, {
      data: {
        mode: 'permanent',
        desired_perm_ids: desired,
        reason: 'e2e:rbac test 5.4',
      },
    });
    expect(grant.ok()).toBeTruthy();

    await page.goto(`/waybill/${waybillId}`);
    const approveBtn = page.locator('[data-testid="big-approve-submission"]');
    await expect(approveBtn).toBeVisible();

    const before = await eventCount(waybillId);
    await approveBtn.click();
    await expect.poll(() => eventCount(waybillId), { timeout: 15_000 }).toBe(before + 1);
    expect(await waybillStage(waybillId)).toBe('dept_verification');

    await cleanupAll(tag);
  });

  test('5.5 — cross-department rejection: EMP021 (marketing) cannot approve dev-submitter dept_authorization', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACCrossDept',
      vendorTag: tag,
    });
    await advanceWaybill(waybillId, 'dept_authorization');
    expect(await waybillStage(waybillId)).toBe('dept_authorization');

    await page.request.post('/api/actor', { data: { id: ids.EMP021 } });
    await page.goto(`/waybill/${waybillId}`);

    expect(await page.locator('[data-testid^="big-approve-dept_authorization"]').count()).toBe(0);
    expect(await page.locator('[data-testid^="big-reject-dept_authorization"]').count()).toBe(0);

    const actorDept = await q1<{ pid: string | null }>(
      `SELECT permission_id AS pid FROM perm.user_permissions
        WHERE user_id = $1 AND permission_id LIKE 'user:dept:%'
          AND revoked_at IS NULL AND (ends_at IS NULL OR ends_at > now())
        ORDER BY permission_id LIMIT 1`,
      [ids.EMP021],
    );
    expect(actorDept?.pid).toBe('user:dept:marketing::allow');

    await cleanupAll(tag);
  });

  test('5.6 — tile gate update via API (PATCH /api/perm/tiles/[id]/gate)', async ({ page }) => {
    const tileId = 'chat';
    const newPerm = 'finance:report:executive::allow';

    const before = await q1<{ view_perm_id: string }>(
      `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
      [tileId],
    );
    expect(before?.view_perm_id).toBeTruthy();
    originalTiles.set(tileId, before!.view_perm_id);

    await page.request.post('/api/actor', { data: { id: ids.IT001 } });
    const r = await page.request.patch(`/api/perm/tiles/${tileId}/gate`, {
      data: { view_perm_id: newPerm },
    });
    expect(r.ok()).toBeTruthy();

    const after = await q1<{ view_perm_id: string }>(
      `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
      [tileId],
    );
    expect(after?.view_perm_id).toBe(newPerm);

    const bad = await page.request.patch(`/api/perm/tiles/${tileId}/gate`, {
      data: { view_perm_id: 'not-a-valid-perm' },
    });
    expect(bad.status()).toBe(400);
  });

  test('5.7 — /api/perm/* list endpoints require auth', async ({ page }) => {
    await page.context().clearCookies();
    const endpoints = ['users', 'roles', 'audit', 'permissions'];
    for (const ep of endpoints) {
      const r = await page.request.get(`/api/perm/${ep}`);
      expect([401, 403]).toContain(r.status());
    }

    await page.request.post('/api/actor', { data: { id: ids.IT001 } });
    for (const ep of endpoints) {
      const r = await page.request.get(`/api/perm/${ep}`);
      if (!r.ok()) {
        const body = await r.text();
        throw new Error(`/api/perm/${ep} returned ${r.status()}: ${body.slice(0, 200)}`);
      }
    }
  });

  test('5.8 — append event on approve (eventCount goes up by exactly 1)', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACAppendEvent',
      vendorTag: tag,
    });

    await page.request.post('/api/actor', { data: { id: ids.EMP001 } });

    const before = await eventCount(waybillId);
    expect(before).toBe(1);

    await page.goto(`/waybill/${waybillId}`);
    const btn = page.locator('[data-testid="big-approve-submission"]');
    await expect(btn).toBeVisible();

    await btn.click();
    await expect.poll(() => eventCount(waybillId), { timeout: 15_000 }).toBe(before + 1);
    expect(await waybillStage(waybillId)).toBe('dept_verification');

    await cleanupAll(tag);
  });

  test('5.bonus — second approve appends exactly one more event', async ({ page }) => {
    const tag = await TAG();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'RBACAppendMulti',
      vendorTag: tag,
    });
    await advanceWaybill(waybillId, 'dept_verification');

    await page.request.post('/api/actor', { data: { id: ids.EMP017 } });
    const before = await eventCount(waybillId);

    await page.goto(`/waybill/${waybillId}`);
    const btn = page.locator('[data-testid="big-approve-dept_verification"]');
    await expect(btn).toBeVisible();

    await btn.click();
    await expect.poll(() => eventCount(waybillId), { timeout: 15_000 }).toBe(before + 1);
    expect(await waybillStage(waybillId)).toBe('dept_authorization');

    await cleanupAll(tag);
  });
});
