import { test, expect } from './fixtures';
import { chromium, type Page } from '@playwright/test';
import { exec, q1, q, userIdByCode } from './helpers/db';
import { pinLocale } from './helpers/lang';

const REASON = 'E2E family trip';
const LEAVE_TYPE = 'annual';
const START = '2026-08-01';
const END = '2026-08-03';
const DAYS = 3;

async function cleanupLeavesByEmployee(employeeId: number, reason: string): Promise<void> {
  await exec(
    `DELETE FROM folio.waybill_events
      WHERE waybill_id IN (
        SELECT hl.waybill_id FROM folio.hr_leave hl
         WHERE hl.employee_id = $1
           AND (hl.reason LIKE $2 OR hl.medical_cert_note LIKE $2)
      )`,
    [employeeId, `${reason}%`],
  );
  await exec(
    `DELETE FROM folio.waybills
      WHERE id IN (
        SELECT hl.waybill_id FROM folio.hr_leave hl
         WHERE hl.employee_id = $1
           AND (hl.reason LIKE $2 OR hl.medical_cert_note LIKE $2)
      )`,
    [employeeId, `${reason}%`],
  );
}

async function cleanAll(): Promise<void> {
  const ids = await q<{ id: number }>(
    `SELECT id FROM folio.users WHERE employee_code = ANY($1::text[])`,
    [['EMP001', 'EMP015', 'EMP016', 'HR002']],
  );
  for (const { id } of ids) {
    await cleanupLeavesByEmployee(id, REASON);
  }
  await exec(
    `UPDATE folio.users SET quota_sick = 30, quota_annual = 10, quota_personal = 6
      WHERE employee_code = ANY($1::text[])`,
    [['EMP001', 'EMP015', 'EMP016', 'HR002']],
  );
}

async function signIn(page: Page, code: string): Promise<void> {
  const id = await userIdByCode(code);
  const r = await page.request.post('/api/actor', { data: { id } });
  if (!r.ok()) throw new Error(`Sign-in failed for ${code}: ${r.status()}`);
}

async function newSignedInPage(code: string): Promise<{ browser: import('@playwright/test').Browser; page: Page }> {
  const browser = await chromium.launch();
  const ctx = await browser.newContext();
  await pinLocale(ctx, 'th');
  const page = await ctx.newPage();
  await signIn(page, code);
  return { browser, page };
}

test.describe.configure({ mode: 'serial' });

test.describe('HR suite', () => {
  test.afterEach(async () => {
    await cleanAll();
  });

  test('6a.1 HR Officer submits own leave request', async () => {
    const emp016Id = await userIdByCode('EMP016');
    const { browser, page } = await newSignedInPage('EMP016');
    try {
      await page.goto('/me/leave');
      await page.locator('select[name="leaveType"]').selectOption(LEAVE_TYPE);
      await page.locator('input[name="startDate"]').fill(START);
      await page.locator('input[name="endDate"]').fill(END);
      await expect(page.locator('input[name="days"]')).toHaveValue(String(DAYS));
      await page.locator('textarea[name="reason"]').fill(REASON);
      await page.locator('form').evaluate((form: HTMLFormElement) => {
        form.requestSubmit();
      });
      await page.waitForURL(/\/me\/leave/, { timeout: 15000 });
      await page.waitForLoadState('networkidle');

      const rows = await q<{ waybill_id: string; status: string; reason: string }>(
        `SELECT w.id AS waybill_id, w.status, hl.reason
           FROM folio.waybills w
           JOIN folio.hr_leave hl ON hl.waybill_id = w.id
          WHERE hl.employee_id = $1
            AND w.origin = 'hr_leave'
            AND hl.reason LIKE $2
          ORDER BY w.created_at DESC LIMIT 1`,
        [emp016Id, `${REASON}%`],
      );
      expect(rows.length).toBe(1);
      expect(rows[0].status).toBe('open');
      expect(rows[0].reason).toContain(REASON);
      await expect(page.locator(`text=${REASON}`).first()).toBeVisible({ timeout: 5000 });
    } finally {
      await browser.close();
    }
  });

  test('6a.2 HR Manager views and approves leave', async () => {
    const emp016Id = await userIdByCode('EMP016');
    const yr = new Date().getFullYear();
    const seq = Date.now() % 1_000_000;
    const waybillId = `WB-${yr}-${String(seq).padStart(6, '0')}`;
    await exec(
      `INSERT INTO folio.waybills
         (id, origin, origin_id, fiscal_year, waybill_kind,
          submitter_id, current_stage, status, created_at, updated_at)
       VALUES ($1, 'hr_leave',
               (SELECT COALESCE(MAX(origin_id), 0) + 1 FROM folio.waybills WHERE origin='hr_leave'),
               $2::smallint, 'hr_leave', $3, 'hr_review', 'open', now(), now())`,
      [waybillId, yr, emp016Id],
    );
    await exec(
      `INSERT INTO folio.hr_leave (waybill_id, employee_id, leave_type, start_date, end_date, days, reason)
       VALUES ($1, $2, $3, $4::date, $5::date, $6, $7)`,
      [waybillId, emp016Id, LEAVE_TYPE, START, END, DAYS, REASON],
    );
    await exec(
      `INSERT INTO folio.waybill_events
         (waybill_id, sequence, kind, stage_from, stage_to, actor_id, payload, occurred_at)
       VALUES ($1, 1, 'submitted', NULL, 'hr_review', $2, '{}'::jsonb, now())`,
      [waybillId, emp016Id],
    );

    const { browser, page } = await newSignedInPage('EMP015');
    try {
      await page.goto('/hr/leave');
      await expect(page.locator(`text=${REASON}`).first()).toBeVisible({ timeout: 15000 });

      await page.goto(`/hr/leave/${waybillId}`);
      await expect(page.locator('[data-testid="status-badge"][data-status="pending"]')).toBeVisible({ timeout: 15000 });
      await page.locator('[data-testid="approve-leave"]').click();
      await page.waitForURL(new RegExp(`/hr/leave/${waybillId}`), { timeout: 15000 });
      await page.waitForLoadState('networkidle');

      const wb = await q1<{ current_stage: string; status: string }>(
        `SELECT current_stage, status FROM folio.waybills WHERE id = $1`,
        [waybillId],
      );
      expect(wb).toBeTruthy();
      expect(wb!.current_stage).not.toBe('hr_review');
      if (wb!.current_stage === 'hr_disbursed') {
        expect(wb!.status).toBe('completed');
      } else {
        expect(['hr_authorization', 'hr_disbursed']).toContain(wb!.current_stage);
      }
    } finally {
      await browser.close();
    }
  });

  test('6a.3 HR Manager views employee directory and detail', async () => {
    const { browser, page } = await newSignedInPage('EMP015');
    try {
      await page.goto('/hr/employees');
      await expect(page.locator('text=John Staff').first()).toBeVisible({ timeout: 15000 });
      await expect(page.locator('text=Patricia Manager').first()).toBeVisible();

      const emp001Id = await userIdByCode('EMP001');
      await page.goto(`/hr/employees/${emp001Id}`);
      await expect(page.locator('text=John Staff').first()).toBeVisible({ timeout: 10000 });
      await expect(page.locator('text=EMP001').first()).toBeVisible();
      await expect(page.locator('text=Senior Developer').first()).toBeVisible();
      await expect(page.locator('text=Engineering').first()).toBeVisible();
    } finally {
      await browser.close();
    }
  });

  test('6a.4 Officer cannot access /hr/leave', async () => {
    const { browser, page } = await newSignedInPage('EMP001');
    try {
      await page.goto('/hr/leave');
      await page.waitForURL(/\/forbidden/, { timeout: 15000 });
    } finally {
      await browser.close();
    }
  });

  test('6a.5 HR dashboard renders for HR manager', async () => {
    const { browser, page } = await newSignedInPage('EMP015');
    try {
      await page.goto('/hr', { waitUntil: 'networkidle' });
      await expect(page.locator('text=HR Leave Management Portal')).toBeVisible({ timeout: 15000 });
      await expect(page.getByText('คำขอลาทั้งหมด').first()).toBeVisible();
      await expect(page.getByText('รออนุมัติ').first()).toBeVisible();
    } finally {
      await browser.close();
    }
  });

  test('6a.6 HR CSV export endpoint', async ({ persona, page }) => {
    await persona('hrManagerEmp015');
    const r = await page.request.get('/api/hr/export');
    expect(r.status()).toBe(200);
    const ct = r.headers()['content-type'] ?? '';
    expect(ct).toMatch(/(text\/csv|application\/octet-stream)/);
    const body = await r.text();
    expect(body).toContain('รหัสพนักงาน');
    expect(body).toContain('สถานะ');
  });

  test('6a.7 HR leave quota update endpoint', async ({ persona, page }) => {
    await persona('hrManagerEmp015');
    const empId = await userIdByCode('EMP001');
    const hrId = await userIdByCode('EMP015');
    const r = await page.request.post('/api/hr/employee/leave-quota', {
      data: {
        employeeId: String(empId),
        hrId: String(hrId),
        totalAnnualLeave: 30,
        reason: 'E2E quota update',
      },
    });
    expect(r.status()).toBe(200);
    const body = (await r.json()) as { success: boolean; changes: { label: string; from: number; to: number }[] };
    expect(body.success).toBe(true);
    const after = await q1<{ quota_annual: number }>(
      `SELECT quota_annual FROM folio.users WHERE id = $1`,
      [empId],
    );
    expect(after?.quota_annual).toBe(30);
  });
});