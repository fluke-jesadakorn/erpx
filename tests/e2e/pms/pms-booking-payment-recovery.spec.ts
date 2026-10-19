import { expect, test } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createHmac, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { Pool } from 'pg';
import { browserChildEnvironment } from "../../../scripts/testing/runner-environment";
import { createEngineeringBrowserContext } from "../../support/browser/engineering-browser-evidence";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

test.use({ actionTimeout: 20_000 });
// Interrupted POS jobs keep a 120-second durable lease before another worker may reclaim them.
const interruptedWorkerLeaseRecoveryTimeout = 150_000;

test('paid guest booking recovers interrupted receipt and independently approved partial refund without duplicate money', async ({ browser }, info) => {
  test.setTimeout(480_000);
  const run = process.env.ERPX_DEMO_RUN;
  expect(run).toMatch(/^[a-f0-9]{12}$/);
  const directory = resolve('.runtime/browser', run!), origin = 'http://127.0.0.1:3104';
  expect(process.env.ERPX_E2E_BASE_URL).toBe(origin);
  const state = JSON.parse(await readFile(join(directory, 'state.json'), 'utf8'));
  expect(state).toMatchObject({ run, prefix: `erpx_browser_${run}`, profile: 'booking', origin, port: 3104, providerFixture: true, ready: true });
  expect(state.release?.id).toBeTruthy(); expect(state.jobsPid).toBeGreaterThan(0);
  const fixture = state.fixture, providerPath = join(directory, 'provider-fixture.json');
  const manifest = JSON.parse(await readFile(join(directory, 'databases.json'), 'utf8')), url = new URL(manifest.databases.H001_DB);
  expect(url.hostname).toBe('127.0.0.1'); expect(url.port).toBe('5432'); expect(url.pathname).toBe(`/erpx_browser_${run}_h001_db`);
  const db = new Pool({ connectionString: url.href, max: 1 });
  const guest = await createEngineeringBrowserContext(browser, info, { baseURL: origin });
  // Keep the application's checkout redirect, but never visit a real provider.
  await guest.route('https://checkout.stripe.com/**', route => route.fulfill({ status: 200, contentType: 'text/html', body: '<!doctype html><html lang="en"><title>Fictional provider checkout</title><h1>Fictional provider checkout</h1><p>Mock payment transport for the isolated ERPX acceptance cohort.</p></html>' }));
  const reviewer = await createEngineeringBrowserContext(browser, info, { baseURL: origin });
  const page = await guest.newPage(), review = await reviewer.newPage();
  let paused = false;
  const recoveries: Record<string, unknown>[] = [];
  const providerState = async () => JSON.parse(await readFile(providerPath, 'utf8'));
  async function control(mode: 'pause-jobs' | 'resume-jobs') {
    const environment = browserChildEnvironment(process.env, { ERPX_BROWSER_SIMULATION_PORT: '3104', ERPX_BROWSER_SIMULATION_PROFILE: 'booking', ERPX_BROWSER_SIMULATION_ATTACHMENTS: 'local', ERPX_BROWSER_SIMULATION_JOBS: '1', ERPX_PMS_PROVIDER_FIXTURE: '1', ERPX_ENVIRONMENT: 'local', ERPX_PUBLIC_ORIGIN: origin, ERPX_DATA_DIR: directory, POS_STRIPE_TEST_SECRET_KEY: 'sk_test_erpx_fictional_provider', POS_STRIPE_TEST_WEBHOOK_SECRET: 'whsec_erpx_fictional_provider' });
    const bun = resolve('.runtime/bun-1.4.3/bun-darwin-aarch64/bun');
    const child = spawn(bun, ['--env-file=/dev/null', 'scripts/local/run-server-script.ts', 'scripts/local/browser-simulation.ts', mode, run!], { cwd: process.cwd(), env: environment, stdio: ['ignore', 'pipe', 'pipe'] });
    let output = ''; child.stdout.on('data', bytes => { output += bytes; }); child.stderr.on('data', bytes => { output += bytes; });
    const exit = await new Promise<number | null>((ok, no) => { child.once('error', no); child.once('exit', ok); });
    expect(exit, output).toBe(0);
    const current = JSON.parse(await readFile(join(directory, 'state.json'), 'utf8'));
    expect(current.release).toEqual(state.release); expect(current.sourceFingerprint).toBe(state.sourceFingerprint);
    expect(current.jobsPaused).toBe(mode === 'pause-jobs'); paused = mode === 'pause-jobs'; return current;
  }
  async function arm(route: string) {
    expect(paused).toBe(true);
    const provider = await providerState(); expect(provider.held ?? null).toBeNull();
    provider.holdNextResponse = route;
    await writeFile(providerPath, JSON.stringify(provider) + '\n', { mode: 0o600 });
  }
  async function interruptHeldWorker(label: string, beforeStop: () => Promise<void>) {
    const first = await control('resume-jobs');
    await expect.poll(async () => (await providerState()).held?.pid, { timeout: 15_000 }).toBe(first.jobsPid);
    await beforeStop();
    await control('pause-jobs');
    const resumed = await control('resume-jobs'); expect(resumed.jobsPid).not.toBe(first.jobsPid);
    recoveries.push({ label, stoppedPid: first.jobsPid, restartedPid: resumed.jobsPid });
  }
  try {
    await control('pause-jobs');
    await page.goto('/book/booking-test');
    await page.getByRole('combobox', { name: 'Language / ภาษา', exact: true }).selectOption('en');
    const arrival = new Date(Date.now() + 7 * 86_400_000).toISOString().slice(0, 10), departure = new Date(Date.now() + 9 * 86_400_000).toISOString().slice(0, 10);
    await page.getByLabel('Check-in', { exact: true }).fill(arrival); await page.getByLabel('Check-out', { exact: true }).fill(departure);
    await page.getByLabel('Child ages per room').fill('7'); await page.getByLabel('Rooms per selection').fill('2');
    await page.getByRole('button', { name: 'Search availability', exact: true }).click();
    await expect(page.getByText('THB 14,124.00', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Add rooms', exact: true }).click(); await page.getByRole('button', { name: 'Continue to guest details', exact: true }).click();
    await page.getByLabel('Full name').fill('Fictional paid recovery guest'); await page.getByLabel('Email', { exact: true }).fill('paid-recovery@example.test'); await page.getByLabel('Phone', { exact: true }).fill('020000000');
    await page.getByRole('button', { name: 'Review price & policy', exact: true }).click();
    await page.getByLabel('I have reviewed the dates, full price and cancellation terms.').check();
    let bookingResult: { state: string; attemptToken: string } | undefined;
    await guest.route('**/api/public/booking/booking-test/reservations', async route => {
      const response = await route.fetch(); expect(response.status()).toBe(200);
      bookingResult = (await response.json()).data;
      await route.fulfill({ response });
    });
    await page.getByRole('button', { name: 'Continue to secure payment', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Fictional provider checkout', exact: true })).toBeVisible();
    expect(bookingResult).toBeTruthy(); const booking = bookingResult!;
    expect(booking.state).toBe('payment_pending'); const paymentId = booking.attemptToken.split('.')[0];
    const payment = (await db.query('SELECT amount_minor,checkout_id FROM pms_booking_payments WHERE id=$1', [paymentId])).rows[0]; expect(Number(payment.amount_minor)).toBe(1412400);
    const provider = await providerState(), session = provider.resources[payment.checkout_id];
    await arm('payment_intents');
    const eventId = `evt_fictional_${randomUUID()}`, event = { id: eventId, type: 'payment_intent.succeeded', account: 'acct_erpx_fictional', livemode: false, created: Math.floor(Date.now() / 1000), data: { object: provider.resources[session.payment_intent] } };
    const raw = JSON.stringify(event), timestamp = Math.floor(Date.now() / 1000), signature = `t=${timestamp},v1=${createHmac('sha256', 'whsec_erpx_fictional_provider').update(`${timestamp}.${raw}`).digest('hex')}`;
    expect((await guest.request.post('/api/pos/stripe/webhook', { headers: { 'stripe-signature': signature }, data: raw + ' ' })).status()).toBe(400);
    for (let index = 0; index < 2; index++) expect((await guest.request.post('/api/pos/stripe/webhook', { headers: { 'stripe-signature': signature }, data: raw })).status()).toBe(200);
    expect((await db.query("SELECT count(*)::int n FROM pos_jobs WHERE kind='event' AND record_id=$1", [eventId])).rows[0].n).toBe(1);
    await interruptHeldWorker('payment receipt before local money', async () => {
      expect((await db.query('SELECT status FROM pms_booking_payments WHERE id=$1', [paymentId])).rows[0].status).toBe('payment_pending');
      expect((await db.query('SELECT count(*)::int n FROM pms_booking_allocations WHERE payment_id=$1', [paymentId])).rows[0].n).toBe(0);
      expect((await db.query("SELECT count(*)::int n FROM pms_records WHERE property_id=$1 AND kind='entry' AND data_json::jsonb->>'sourceKey'=$2", [fixture.propertyId, `online-payment:${paymentId}`])).rows[0].n).toBe(0);
    });
    await expect.poll(async () => (await db.query('SELECT status FROM pms_booking_payments WHERE id=$1', [paymentId])).rows[0].status, { timeout: interruptedWorkerLeaseRecoveryTimeout }).toBe('confirmed');
    await page.goto(`/book/booking-test?attempt=${encodeURIComponent(booking.attemptToken)}`);
    await expect(page.getByRole('heading', { name: /Booking confirmed/ })).toBeVisible(); await expect(page.getByText('Payment confirmed. A confirmation email and private management link are on the way.', { exact: true })).toBeVisible();
    await page.locator('a[href*="/manage#token="]').click(); await expect(page.getByText('Fictional paid recovery guest', { exact: true })).toBeVisible(); expect(new URL(page.url()).hash).toBe('');
    const original = (await db.query("SELECT id,data_json FROM pms_records WHERE property_id=$1 AND kind='stay' ORDER BY id", [fixture.propertyId])).rows;
    expect(original).toHaveLength(2);
    const selections = page.getByRole('checkbox', { name: `Select Family room ${arrival}`, exact: true }); expect(await selections.count()).toBe(2); await selections.first().check();
    await page.getByRole('button', { name: 'Cancel selected rooms', exact: true }).click(); const drawer = page.getByRole('dialog', { name: 'Review cancellation', exact: true });
    await drawer.getByRole('button', { name: 'Review price and terms', exact: true }).click(); await expect(drawer.getByText('THB 7,062.00', { exact: true })).toBeVisible();
    await drawer.getByRole('button', { name: 'Confirm changes', exact: true }).click(); await expect(drawer).toBeHidden(); await expect(page.getByText(/Awaiting independent approval/)).toBeVisible();
    await control('pause-jobs'); await arm('refunds');
    const refund = (await db.query('SELECT id,state,requested_by,amount_minor FROM pms_booking_refund_requests WHERE payment_id=$1', [paymentId])).rows[0]; expect(refund.state).toBe('requested'); expect(refund.requested_by).toBe('guest:self-service'); expect(Number(refund.amount_minor)).toBe(706200);
    const credentials = JSON.parse(await readFile(join(directory, 'credentials.json'), 'utf8')), user = credentials.users.find((item: { userId: string }) => item.userId === fixture.staff.reviewerId);
    expect(fixture.staff.reviewerId).not.toBe(fixture.staff.authorId);
    expect((await reviewer.request.post('/api/auth/sign-in/email', { headers: { Origin: origin }, data: { email: user.email, password: credentials.password } })).status()).toBe(200);
    await selectFixtureAssignment(reviewer, fixture.companyId, user.assignmentId, 'booking-test'); await review.goto('/holdings/booking-test/finance/cash/refunds');
    const refundSummary = review.locator('#workspace-content summary').filter({ hasText: /^Hotel booking & counter refunds$/, visible: true });
    await expect(refundSummary).toHaveCount(1);
    await refundSummary.click();
    await review.getByRole('button', { name: 'Approve Stripe refund', exact: true }).click();
    await expect.poll(async () => (await db.query('SELECT state FROM pms_booking_refund_requests WHERE id=$1', [refund.id])).rows[0].state).toBe('approved');
    await interruptHeldWorker('refund accepted by provider before local settlement', async () => {
      const current = (await db.query('SELECT state,provider_id,generation FROM pms_booking_refund_requests WHERE id=$1', [refund.id])).rows[0]; expect(current).toMatchObject({ state: 'approved', provider_id: null, generation: 0 });
      const accepted = await providerState(); expect(accepted.keys[`pms-refund-${refund.id}-0`]).toBeTruthy();
    });
    await expect.poll(async () => (await db.query('SELECT state FROM pms_booking_refund_requests WHERE id=$1', [refund.id])).rows[0].state, { timeout: interruptedWorkerLeaseRecoveryTimeout }).toBe('succeeded');
    await page.getByRole('button', { name: 'Refresh booking', exact: true }).click(); await expect(page.getByText(/Refund completed/)).toBeVisible(); await page.reload(); await expect(page.getByText(/Refund completed/)).toBeVisible();
    expect((await db.query('SELECT count(*)::int n FROM pms_booking_refund_attempts WHERE request_id=$1', [refund.id])).rows[0].n).toBe(1);
    const allocations = (await db.query('SELECT count(*)::int n,sum(amount_minor)::text amount FROM pms_booking_allocations WHERE payment_id=$1', [paymentId])).rows[0]; expect(allocations).toEqual({ n: 2, amount: '1412400' });
    const after = (await db.query("SELECT id,data_json FROM pms_records WHERE property_id=$1 AND kind='stay' ORDER BY id", [fixture.propertyId])).rows;
    const originalData = original.map(row => ({ id: row.id, data: JSON.parse(row.data_json) })), afterData = after.map(row => ({ id: row.id, data: JSON.parse(row.data_json) }));
    const unchanged = afterData.filter(row => row.data.state !== 'cancelled'); expect(unchanged).toHaveLength(1); expect(unchanged[0]).toEqual(originalData.find(row => row.id === unchanged[0].id));
    const posted = (await db.query("SELECT COALESCE(SUM(l.debit_minor),0)::text debit,COALESCE(SUM(l.credit_minor),0)::text credit FROM pms_postings p JOIN finance_journal_lines l ON l.journal_id=p.journal_id WHERE p.property_id=$1", [fixture.propertyId])).rows[0]; expect(posted.debit).toBe(posted.credit);
    await expect.poll(async () => (await db.query("SELECT state FROM pos_jobs WHERE kind='event' AND record_id=$1", [eventId])).rows[0].state, { timeout: 140_000 }).toBe('completed');
    const accepted = await providerState(); expect(Object.keys(accepted.keys).filter(key => key === `pms-refund-${refund.id}-0`)).toHaveLength(1);
    expect(accepted.requests.filter((request: { key: string }) => request.key === `pms-refund-${refund.id}-0`)).toHaveLength(2);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.screenshot({ path: info.outputPath('paid-partial-refund-completed.png'), fullPage: true });
    await info.attach('compiled-paid-booking-recovery', { body: JSON.stringify({ run, sourceFingerprint: state.sourceFingerprint, release: state.release, recoveries, paymentBeforeInterruption: 'payment_pending with zero local allocations/entries', refundBeforeInterruption: 'provider accepted; approved locally without provider_id', allocations, refundMinor: 706200, providerRefundAttempts: 1, journals: posted, eventCompleted: true }), contentType: 'application/json' });
  } finally {
    if (paused) await control('resume-jobs');
    const results = await Promise.allSettled([guest.close(), reviewer.close(), db.end()]); const failed = results.find(result => result.status === 'rejected'); if (failed?.status === 'rejected') throw failed.reason;
  }
});
