import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const read = (file: string) =>
  readFileSync(fileURLToPath(new URL(`../../../${file}`, import.meta.url)), 'utf8');

const route = 'packages/feature-pms/src/routes/api/v1/holdings/[holdingSlug]/pms/stripe-payments/route.ts';
const compat = 'apps/compat/src/app/api/v1/holdings/[holdingSlug]/pms/stripe-payments/route.ts';
const pms = 'apps/pms/src/app/api/v1/holdings/[holdingSlug]/pms/stripe-payments/route.ts';

describe('PromptPay payment-status Next route retirement', () => {
  it('removes only the status GET from the shared handler and both app re-exports', () => {
    const source = read(route);
    expect(source).not.toContain('export async function GET');
    expect(source).not.toContain('nativePaymentStatus');
    expect(source).toContain('export async function POST');
    expect(source).toContain('createPmsStripePayment');
    for (const file of [compat, pms]) {
      const reexport = read(file);
      expect(reexport).toContain('export { POST }');
      expect(reexport).not.toContain('GET');
    }
  });

  it('executes PMS SQL in the native owner and owns the exact camelCase payload', () => {
    const sql = read('packages/server-pms/src/adapters/payment_status_postgres.rs');
    expect(sql).toContain('FROM pms_stripe_payments WHERE id=$1 AND company_id=$2');
    expect(sql).toContain('sqlx::query(PAYMENT_STATUS_SQL)');
    const http = read('packages/server-pms/src/adapters/payment_status_http.rs');
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/pms/stripe-payments"');
    expect(http).toContain('get(payment_status)');
    expect(http).toContain('"data": view');
    const view = read('packages/server-pms/src/domain/payment_status.rs');
    expect(view).toContain('#[serde(rename_all = "camelCase")]');
    expect(view).toContain('payer_email_provided: bool');
    expect(read('src/composition/src/private_development_api.rs')).toContain(
      'pms_payment_status_router',
    );
  });

  it('keeps the browser status read on the same bounded GET URL while POST stays on Next', () => {
    const browser = read('packages/feature-pms/src/components/promptpay-counter.tsx');
    expect(browser).toContain('/pms/stripe-payments?id=${encodeURIComponent(id)}');
    expect(browser).toContain('method: "POST"');
  });

  it('routes only the bounded GET read through the proposed native path entry', () => {
    const environment = {
      ERPX_DEV_NATIVE_PATHS: 'acme.jesadakorn.com|GET|/api/v1/holdings/acme/pms/stripe-payments',
    } as unknown as NodeJS.ProcessEnv;
    const base = '/api/v1/holdings/acme/pms/stripe-payments';
    expect(nativeApiPath(environment, `${base}?id=p-1`, 'acme.jesadakorn.com', 'GET')).toBe(true);
    expect(nativeApiPath(environment, base, 'acme.jesadakorn.com', 'POST')).toBe(false);
    expect(nativeApiPath(environment, `${base}?id=p-1`, 'unregistered.example.test', 'GET')).toBe(false);
  });
});
