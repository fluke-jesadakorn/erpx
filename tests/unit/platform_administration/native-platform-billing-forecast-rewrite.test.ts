import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { nativeApiPath, nativeApiRewrite } from '../../../scripts/local/native-api-proxy';

const host = 'erpx-internal.jesadakorn.com';
const rewrite = '/api/v1/platform/billing/forecast=/api/v1/holdings/erpx/billing/forecast';
const environment = {
  ERPX_DEV_NATIVE_PATHS: `${host}|GET|/api/v1/platform/billing/forecast`,
  ERPX_DEV_NATIVE_REWRITES: rewrite,
};

describe('platform Billing forecast native rewrite', () => {
  it('routes the platform GET to the ERPX holding forecast and keeps the customer selector', () => {
    const url = '/api/v1/platform/billing/forecast?holdingId=h001';
    expect(nativeApiPath(environment, url, host, 'GET')).toBe(true);
    expect(nativeApiRewrite(environment, url)).toBe(
      '/api/v1/holdings/erpx/billing/forecast?holdingId=h001',
    );
    expect(nativeApiPath(environment, url, host, 'POST')).toBe(false);
    expect(nativeApiPath(environment, url, 'unregistered.example.test', 'GET')).toBe(false);
  });

  it('keeps the browser caller on the public path that the gateway rewrites', () => {
    const source = readFileSync(
      new URL(
        '../../../packages/feature-billing/src/components/next-month-forecast.tsx',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).toContain('/api/v1/platform/billing/forecast?holdingId=');
    expect(source).not.toContain('/api/v1/holdings/erpx/billing/forecast');
  });
});
