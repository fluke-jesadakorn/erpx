import { test, expect } from './fixtures';
import type { Page, APIRequestContext } from '@playwright/test';
import {
  cleanupTestTag,
  eventCount,
  eventKinds,
  q1,
  userIdByCode,
  vendorTag,
  waybillAmount,
  waybillStage,
} from './helpers/db';
import { createExpense } from './helpers/waybill';

async function advanceViaUi(
  page: Page,
  waybillId: string,
  selector: string,
  expectedStage: string,
  opts: { timeoutMs?: number } = {},
): Promise<void> {
  await page.goto(`/waybill/${waybillId}`);
  await page.locator(selector).first().waitFor({ state: 'visible', timeout: 15_000 });
  await page.locator(selector).first().click();
  await expect
    .poll(async () => (await waybillStage(waybillId)) ?? null, { timeout: opts.timeoutMs ?? 15_000 })
    .toBe(expectedStage);
}

async function settle(page: Page, waybillId: string, slipId: number): Promise<void> {
  await page.goto(`/waybill/${waybillId}?slipId=${slipId}`);
  await page.locator('[data-testid="settle-submit"]').first().waitFor({ state: 'visible', timeout: 30_000 });
  await expect(page.locator('[data-testid="settle-submit"]').first()).toBeEnabled({ timeout: 60_000 });
  await page.locator('select[name="paymentMethod"]').first().selectOption('transfer');
  await page.locator('[data-testid="settle-submit"]').first().click();
  await expect
    .poll(async () => (await waybillStage(waybillId)) ?? null, { timeout: 30_000 })
    .toBe('disbursed');
}

async function uploadPayslip(req: APIRequestContext, vendorTag: string, uploaderId: number): Promise<number> {
  void req;
  const slip = await q1<{ id: number }>(
    `INSERT INTO slips (
       file_path, mime_type, file_size, ocr_raw_json, ocr_confidence,
       uploaded_by, status, kind, expense_id
     )
     VALUES (
       $1, 'image/png', 69, '{}'::jsonb, 0,
       $2, 'pending', 'receipt', NULL
     )
     RETURNING id`,
    [
      `e2e/${vendorTag}.png`,
      uploaderId,
    ],
  );
  if (!slip?.id) throw new Error('failed to insert test slip');
  return slip.id;
}

test.describe('waybill pipeline', () => {
  test('3.1 <200k happy path: submission → disbursed with GL confirm', async ({ withPersona }) => {
    const tag = await vendorTag();
    const { waybillId, expenseId } = await createExpense({
      submitter: 'EMP001',
      amount: 1000,
      vendorName: 'Test Vendor',
      vendorTag: tag,
    });

    try {
      expect(await waybillAmount(waybillId)).toBe(1000);
      expect(await waybillStage(waybillId)).toBe('submission');

      await withPersona('supervisorEmp017', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-submission"]', 'dept_verification');
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_verification"]', 'dept_authorization');
      });
      await withPersona('managerEmp002', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_authorization"]', 'accounting_verification');
      });
      await withPersona('accountOfficerEmp003', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_verification"]', 'accounting_supervision');
      });
      await withPersona('accountSupervisorEmp018', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_supervision"]', 'accounting_authorization');
      });
      await withPersona('accountingManagerEmp004', async (p) => {
        await advanceViaUi(p, waybillId, `[data-testid="big-final-approve-${waybillId}"]`, 'awaiting_disbursement');
      });

      await withPersona('financeEmp019', async (p) => {
        const uploaderId = await userIdByCode('EMP019');
        const slipId = await uploadPayslip(p.request, tag, uploaderId);
        await settle(p, waybillId, slipId);
        await p.goto(`/waybill/${waybillId}`);
        await p.locator(`[data-testid="gl-confirm-${waybillId}"]`).waitFor({ state: 'visible', timeout: 15_000 });
        await p.locator(`[data-testid="gl-confirm-${waybillId}"]`).click();
        await expect
          .poll(async () => {
            const r = await q1<{ at: string | null }>(
              `SELECT gl_confirmed_at AS at FROM expenses WHERE id = $1`,
              [expenseId],
            );
            return r?.at ?? null;
          }, { timeout: 15_000 })
          .not.toBeNull();
      });

      expect(await waybillStage(waybillId)).toBe('disbursed');
      const count = await eventCount(waybillId);
      expect(count).toBeGreaterThanOrEqual(8);
      const kinds = await eventKinds(waybillId);
      for (const k of ['submitted', 'advanced', 'settled', 'posted-to-gl', 'gl-confirmed'] as const) {
        expect(kinds, `eventKinds must include ${k}`).toContain(k);
      }
    } finally {
      await cleanupTestTag(tag);
    }
  });

  test('3.2 ≥200k includes CEO stage chain', async ({ withPersona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: 'EMP001',
      amount: 250000,
      vendorName: 'Big Ticket Vendor',
      vendorTag: tag,
    });

    try {
      expect(await waybillAmount(waybillId)).toBe(250000);

      await withPersona('supervisorEmp017', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-submission"]', 'dept_verification');
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_verification"]', 'dept_authorization');
      });
      await withPersona('managerEmp002', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_authorization"]', 'accounting_verification');
      });
      await withPersona('accountOfficerEmp003', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_verification"]', 'accounting_supervision');
      });
      await withPersona('accountSupervisorEmp018', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_supervision"]', 'accounting_authorization');
      });
      await withPersona('accountingManagerEmp004', async (p) => {
        await advanceViaUi(p, waybillId, `[data-testid="big-final-approve-${waybillId}"]`, 'disbursement_authorization');
      });
      await withPersona('financeEmp019', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-disbursement_authorization"]', 'cfo_authorization');
      });
      await withPersona('cfoEmp005', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-cfo_authorization"]', 'ceo_authorization');
      });
      await withPersona('ceoEmp006', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-ceo_authorization"]', 'awaiting_disbursement');
      });

      await withPersona('financeEmp019', async (p) => {
        const uploaderId = await userIdByCode('EMP019');
        const slipId = await uploadPayslip(p.request, tag, uploaderId);
        await settle(p, waybillId, slipId);
        await p.goto(`/waybill/${waybillId}`);
        await p.locator(`[data-testid="gl-confirm-${waybillId}"]`).waitFor({ state: 'visible', timeout: 15_000 });
        await p.locator(`[data-testid="gl-confirm-${waybillId}"]`).click();
        await expect
          .poll(async () => (await waybillStage(waybillId)) ?? null, { timeout: 15_000 })
          .toBe('disbursed');
      });

      const kinds = await eventKinds(waybillId);
      const ceoTouched = kinds.includes('ceo_authorization');
      const advancedEvents = await q1<{ from_to: string[] }>(
        `SELECT array_agg(stage_from || '->' || stage_to) AS from_to
           FROM waybill_events
          WHERE waybill_id = $1 AND kind = 'advanced'`,
        [waybillId],
      );
      const chain = advancedEvents?.from_to ?? [];
      expect(ceoTouched || chain.some((s) => s.endsWith('->ceo_authorization'))).toBeTruthy();
      expect(chain).toContain('accounting_authorization->disbursement_authorization');
      expect(chain).toContain('cfo_authorization->ceo_authorization');
      expect(chain).toContain('ceo_authorization->awaiting_disbursement');
    } finally {
      await cleanupTestTag(tag);
    }
  });

  test('3.3 audit log chip shows HMAC verified after pipeline', async ({ page, withPersona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: 'EMP001',
      amount: 1000,
      vendorName: 'Audit Vendor',
      vendorTag: tag,
    });

    try {
      await withPersona('supervisorEmp017', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-submission"]', 'dept_verification');
      });
      await page.goto(`/waybill/${waybillId}`);
      const summary = page.locator('details summary').filter({ hasText: /log|append|HMAC/i }).first();
      await summary.scrollIntoViewIfNeeded();
      const details = summary.locator('xpath=..').locator('xpath=..');
      await details.evaluate((el: HTMLElement) => {
        if (el instanceof HTMLDetailsElement) el.open = true;
      });
      await expect(page.locator('text=/HMAC/i').first()).toBeVisible({ timeout: 10_000 });
    } finally {
      await cleanupTestTag(tag);
    }
  });

  test('3.4 settle form: submit disabled without slip + payment method', async ({ withPersona }) => {
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: 'EMP001',
      amount: 1000,
      vendorName: 'No Settle Vendor',
      vendorTag: tag,
    });

    try {
      await withPersona('supervisorEmp017', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-submission"]', 'dept_verification');
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_verification"]', 'dept_authorization');
      });
      await withPersona('managerEmp002', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-dept_authorization"]', 'accounting_verification');
      });
      await withPersona('accountOfficerEmp003', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_verification"]', 'accounting_supervision');
      });
      await withPersona('accountSupervisorEmp018', async (p) => {
        await advanceViaUi(p, waybillId, '[data-testid="big-approve-accounting_supervision"]', 'accounting_authorization');
      });
      await withPersona('accountingManagerEmp004', async (p) => {
        await advanceViaUi(p, waybillId, `[data-testid="big-final-approve-${waybillId}"]`, 'awaiting_disbursement');
      });

      await withPersona('financeEmp019', async (p) => {
        await p.goto(`/waybill/${waybillId}`);
        await p.locator('[data-testid="settle-submit"]').first().waitFor({ state: 'visible', timeout: 15_000 });
        await expect(p.locator('[data-testid="settle-submit"]').first()).toBeDisabled();
      });
    } finally {
      await cleanupTestTag(tag);
    }
  });
});
