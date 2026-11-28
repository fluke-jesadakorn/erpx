import { describe, expect, it } from 'vitest';
import {
  createNativeBillingClient,
  nativeBillingConfiguration,
  type NativeBillingConfiguration,
  type NativeBillingPageContext,
  type NativeBillingTransport,
  type SeatForecastBaseline,
} from '@erpx/billing-server/public/native-billing';

const actor: NativeBillingPageContext = {
  userId: 'admin',
  holdingId: 'h001',
  holdingSlug: 'acme',
  platform: false,
  role: 'holding_owner',
  assignmentId: 'assignment',
  expiresAt: null,
};

const configuration: NativeBillingConfiguration = {
  privateBaseUrl: 'http://127.0.0.1:3110',
  holdings: [
    { origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' },
  ],
  timeoutMs: 1000,
};

const baseline: SeatForecastBaseline = {
  generatedAt: 1_700_000_000_000,
  forecastMonth: '2027-02',
  sourceMonth: '2027-01',
  model: 'seat_what_if',
  price: null,
  priceEffectiveForMonth: false,
  seats: { count: null, observedAt: null },
  storage: { originalBytes: null, observedAt: null, coverage: 'unavailable' },
};

const context = {
  userId: actor.userId,
  holdingId: actor.holdingId,
  platform: actor.platform,
  role: actor.role,
  assignmentId: actor.assignmentId,
  expiresAt: Date.now() + 60_000,
};

function headers(overrides: Record<string, string> = {}) {
  return new Headers({ host: 'acme.example.test', ...overrides });
}

function response(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('native Billing seat-forecast transport', () => {
  it('reads the exact owner forecast route and returns every field', async () => {
    const calls: Parameters<NativeBillingTransport>[] = [];
    const transport: NativeBillingTransport = async (...args) => {
      calls.push(args);
      return response({ data: baseline, context });
    };
    const client = createNativeBillingClient(nativeBillingConfiguration(JSON.stringify(configuration)), transport);
    const result = await client.forecast(headers(), actor, {
      holdingId: 'h001',
      holdingSlug: 'acme',
    });
    expect(result).toEqual(baseline);
    const [method, url] = calls[0]!;
    expect(method).toBe('GET');
    expect(String(url)).toBe('http://127.0.0.1:3110/api/v1/holdings/acme/billing/forecast');
  });

  it('fails closed on an incomplete owner payload or a foreign host', async () => {
    const partial = { ...baseline, seats: undefined };
    const client = createNativeBillingClient(nativeBillingConfiguration(JSON.stringify(configuration)), async () =>
      response({ data: partial, context }),
    );
    await expect(
      client.forecast(headers(), actor, { holdingId: 'h001', holdingSlug: 'acme' }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_RESPONSE_INVALID' });
    await expect(
      client.forecast(headers({ host: 'evil.example.test' }), actor, {
        holdingId: 'h001',
        holdingSlug: 'acme',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_BILLING_AUTHORITY_INVALID' });
  });
});
