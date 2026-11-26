import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { createNativeQualityReader, type QualityTransport } from '../../../packages/feature-quality/src/adapters/server/native-quality';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44991',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme' }],
});

const incoming = () => new Headers({
  host: 'acme.example.com',
  cookie: '__Host-erpx-product=session',
  origin: 'https://acme.example.com',
  'x-erpx-context-company': 'company',
  'x-erpx-context-assignment': 'quality-assignment',
  'x-erpx-context-revision': '7',
  'x-erpx-context-assignment-version': '3',
});

const alert = {
  id: 'alert-1',
  hold_id: 'hold-1',
  phase: 'overdue',
  source_version: 1,
  hold_version: 2,
  due_date: '2026-11-20',
  detected_at: 1000,
  version: 1,
  acknowledged_by: null,
  acknowledged_at: null,
  reason: null,
  company_id: 'company',
  branch_id: 'branch',
  status: 'held',
  product_name: 'Packaged goods',
  warehouse_name: 'Main warehouse',
  can_ack: 1,
  recipients: 3,
};

const capture = (body: unknown) => {
  let seen: { url: URL; headers: Headers } | undefined;
  const transport: QualityTransport = vi.fn(async (_method, url, headers) => {
    seen = { url, headers };
    return new Response(JSON.stringify({ data: body }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
  return { transport, seen: () => seen };
};

describe('native Quality escalation queue transport', () => {
  it('reads the owner queue with session and scope headers and full field parity', async () => {
    const { transport, seen } = capture({ items: [alert] });
    const rows = await createNativeQualityReader(config, transport).alertQueue(incoming(), 'acme');
    expect(seen()?.url.pathname).toBe('/api/v1/holdings/acme/quality/alerts');
    expect(seen()?.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(seen()?.headers.get('x-erpx-context-company')).toBe('company');
    expect(rows).toEqual([alert]);
    expect(Object.keys(rows[0]).sort()).toEqual(Object.keys(alert).sort());
  });

  it('fails closed on a row missing a retained field', async () => {
    const { transport } = capture({ items: [{ ...alert, recipients: undefined }] });
    await expect(
      createNativeQualityReader(config, transport).alertQueue(incoming(), 'acme'),
    ).rejects.toMatchObject({ code: 'NATIVE_QUALITY_RESPONSE_INVALID' });
  });
});
