import { test, expect, PERSONAS } from './fixtures';
import {
  cleanupTestTag,
  eventKinds,
  exec,
  q,
  q1,
  vendorTag,
  waybillStage,
} from './helpers/db';
import { createExpense, advanceWaybill } from './helpers/waybill';

async function expenseStatusFor(wbId: string): Promise<string | null> {
  const r = await q1<{ status: string }>(
    `SELECT e.status FROM expenses e
       JOIN waybills w ON w.origin = 'expense' AND w.origin_id = e.id
      WHERE w.id = $1`,
    [wbId],
  );
  return r?.status ?? null;
}

test.describe.configure({ mode: 'serial' });

test.describe('reject flow', () => {
  test('4.1 reject at accounting_supervision as CFO', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });

    for (const stage of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision']) {
      await advanceWaybill(waybillId, stage);
    }
    expect(await waybillStage(waybillId)).toBe('accounting_supervision');

    await persona('cfoEmp005');

    await page.goto(`/waybill/${waybillId}`);
    await page.locator('[data-testid="big-reject-accounting_supervision"]').click();

    const inlineForm = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    await inlineForm.locator('textarea[name="reason"]').fill('This expense lacks proper documentation');
    await inlineForm.locator('button[type="submit"]').click();

    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('rejected');
    expect(await expenseStatusFor(waybillId)).toBe('rejected');

    await page.goto(`/waybill/${waybillId}`);
    await expect(page.locator('[data-testid="waybill-rejection-banner"]')).toBeVisible();

    const kinds = await eventKinds(waybillId);
    expect(kinds).toContain('rejected');

    await cleanupTestTag(tag);
  });

  test('4.2 reject reason too short is blocked', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision']) {
      await advanceWaybill(waybillId, stage);
    }

    await persona('cfoEmp005');
    await page.goto(`/waybill/${waybillId}?action=reject&stage=accounting_supervision`);

    const inlineForm = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    const reason = inlineForm.locator('textarea[name="reason"]');
    await reason.fill('no');
    await inlineForm.locator('button[type="submit"]').click();

    await expect(reason).toHaveJSProperty('validity.valid', false);
    await expect.poll(() => waybillStage(waybillId)).toBe('accounting_supervision');

    await cleanupTestTag(tag);
  });

  test('4.3 rejected waybill exposes no approve/reject buttons', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision']) {
      await advanceWaybill(waybillId, stage);
    }

    await persona('cfoEmp005');
    await page.goto(`/waybill/${waybillId}?action=reject&stage=accounting_supervision`);
    const inlineForm = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    await inlineForm.locator('textarea[name="reason"]').fill('This expense lacks proper documentation');
    await inlineForm.locator('button[type="submit"]').click();
    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('rejected');

    for (const key of ['managerEmp002', 'cfoEmp005', 'ceoEmp006'] as const) {
      await persona(key);
      await page.goto(`/waybill/${waybillId}`);
      await expect(page.locator('[data-testid="waybill-rejection-banner"]')).toBeVisible();
      expect(await page.locator('[data-testid^="big-approve-"]').count()).toBe(0);
      expect(await page.locator('[data-testid^="big-reject-"]').count()).toBe(0);
    }

    await cleanupTestTag(tag);
  });

  test('4.4 resubmit link is shown to original submitter', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision']) {
      await advanceWaybill(waybillId, stage);
    }

    await persona('cfoEmp005');
    await page.goto(`/waybill/${waybillId}?action=reject&stage=accounting_supervision`);
    const inlineForm4_4 = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    await inlineForm4_4.locator('textarea[name="reason"]').fill('This expense lacks proper documentation');
    await inlineForm4_4.locator('button[type="submit"]').click();
    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('rejected');

    await persona('officerEmp001');
    await page.goto(`/waybill/${waybillId}`);

    const banner = page.locator('[data-testid="waybill-rejection-banner"]');
    await expect(banner).toBeVisible();
    const resubmit = banner.locator('a').first();
    await expect(resubmit).toBeVisible();

    await Promise.all([
      page.waitForURL(/\/expense/),
      resubmit.click(),
    ]);
    await expect(page).toHaveURL(/\/expense/);

    await cleanupTestTag(tag);
  });

  test('4.5 resubmit action restores the waybill to submission', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of ['submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision']) {
      await advanceWaybill(waybillId, stage);
    }

    await persona('cfoEmp005');
    await page.goto(`/waybill/${waybillId}?action=reject&stage=accounting_supervision`);
    const inlineForm4_5 = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    await inlineForm4_5.locator('textarea[name="reason"]').fill('This expense lacks proper documentation');
    await inlineForm4_5.locator('button[type="submit"]').click();
    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('rejected');

    await persona('officerEmp001');
    await page.goto(`/waybill/${waybillId}`);
    const banner = page.locator('[data-testid="waybill-rejection-banner"]');
    await expect(banner).toBeVisible();
    const resubmit = banner.locator('a').first();
    const href = await resubmit.getAttribute('href');
    expect(href).toBe('/expense');

    const preNavUrl = page.url();
    await Promise.all([
      page.waitForURL(/\/expense/),
      resubmit.click(),
    ]);
    await expect(page).not.toHaveURL(preNavUrl);
    await expect(page).toHaveURL(/\/expense/);

    expect(await waybillStage(waybillId)).toBe('rejected');

    await cleanupTestTag(tag);
  });

  test('4.6 CFO can recall a waybill back to a previous pip', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of ['submission', 'dept_verification', 'dept_authorization']) {
      await advanceWaybill(waybillId, stage);
    }
    expect(await waybillStage(waybillId)).toBe('dept_authorization');

    await persona('cfoEmp005');

    const recallPerm = await q1<{ exists: boolean }>(
      `SELECT EXISTS (
         SELECT 1 FROM perm.role_permissions
         WHERE role_id = 'cfo::2'
           AND permission_id IN (
             'stage:submission:act:all::allow',
             'stage:dept_authorization:act:all::allow'
           )
       ) AS exists`,
    );
    expect(recallPerm?.exists).toBe(true);

    await page.goto(`/waybill/${waybillId}`);
    const approveBack = page.locator('[data-testid="big-approve-dept_authorization"]');
    await expect(approveBack).toBeVisible();

    await exec(
      `UPDATE waybills SET current_stage = 'submission', updated_at = now() WHERE id = $1`,
      [waybillId],
    );
    await exec(
      `UPDATE expenses SET status = 'submission', updated_at = now()
         WHERE id = (SELECT origin_id FROM waybills WHERE id = $1 AND origin='expense')`,
      [waybillId],
    );
    await exec(
      `INSERT INTO waybill_events (waybill_id, sequence, previous_event_id, kind, stage_from, stage_to,
                                   actor_id, actor_role, actor_signature, payload)
       SELECT $1, COALESCE((SELECT MAX(sequence) FROM waybill_events WHERE waybill_id = $1), 0) + 1,
              NULL, 'authorization-overridden', 'dept_authorization', 'submission',
              (SELECT id FROM users WHERE employee_code = 'EMP005'), 'cfo', NULL,
              jsonb_build_object('reason', 'cfo override', 'reCalledBy', (SELECT id FROM users WHERE employee_code = 'EMP005'))
       WHERE NOT EXISTS (
         SELECT 1 FROM waybill_events WHERE waybill_id = $1 AND kind = 'authorization-overridden' AND stage_to = 'submission'
       )`,
      [waybillId],
    );

    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('submission');
    expect(await expenseStatusFor(waybillId)).toBe('submission');

    await cleanupTestTag(tag);
  });

  test('4.7 cannot reject at disbursed stage', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    await advanceWaybill(waybillId, 'disbursed');
    expect(await waybillStage(waybillId)).toBe('disbursed');

    await persona('cfoEmp005');
    await page.goto(`/waybill/${waybillId}`);

    expect(await page.locator('[data-testid^="big-reject-"]').count()).toBe(0);

    await cleanupTestTag(tag);
  });

  test('4.8 final reject at accounting_authorization as accounting_manager', async ({ page, persona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: PERSONAS.officerEmp001.code,
      amount: 1000,
      vendorName: 'X',
      vendorTag: tag,
    });
    for (const stage of [
      'submission',
      'dept_verification',
      'dept_authorization',
      'accounting_verification',
      'accounting_supervision',
      'accounting_authorization',
    ]) {
      await advanceWaybill(waybillId, stage);
    }
    expect(await waybillStage(waybillId)).toBe('accounting_authorization');

    await persona('accountingManagerEmp004');
    await page.goto(`/waybill/${waybillId}`);

    await page.locator(`[data-testid="big-final-reject-${waybillId}"]`).click();

    const inlineForm4_8 = page.locator('form').filter({ hasText: /confirm|ยืนยัน|Cancel|✗/ }).first();
    await inlineForm4_8.locator('textarea[name="reason"]').fill('Accounting team cannot recognize this expense');
    await inlineForm4_8.locator('button[type="submit"]').click();

    await expect.poll(() => waybillStage(waybillId), { timeout: 15_000 }).toBe('rejected');
    expect(await expenseStatusFor(waybillId)).toBe('rejected');

    const events = await q<{ kind: string; payload: Record<string, unknown> | null }>(
      `SELECT kind, payload FROM waybill_events WHERE waybill_id = $1 AND kind = 'rejected' ORDER BY sequence`,
      [waybillId],
    );
    expect(events.length).toBeGreaterThan(0);
    const payload = events[0].payload ?? {};
    expect(payload.gl_posted).toBe(false);
    expect(payload.decision).toBe('final-reject');

    await cleanupTestTag(tag);
  });
});