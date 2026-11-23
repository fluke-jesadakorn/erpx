import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeCrmReadAll,
  nativeCrmReadFirst,
  CRM_RD_COLLAB_NOTIFICATION_LEAD,
} from '../../../packages/server-crm/src/adapters/native-crm-read';
import { collaborationNotificationLead } from '../../../packages/server-crm/src/adapters/collaboration-notification-lead';

const envKey = 'ERPX_NATIVE_CRM_CONFIG';
const original = process.env[envKey];
const plannedSql = 'SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?';
const stubFetch = (payload: unknown, status = 200) => {
  const fetchMock = vi.fn(async () => ({
    status,
    body: { cancel: vi.fn(async () => undefined) },
    json: async () => payload,
  }));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
};

beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44994',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  let batched: { sql: string; values: unknown[] }[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
      };
    },
    async batch(statements: { __sql?: string; __values?: unknown[] }[]) {
      batched = statements.map((s) => ({ sql: s.__sql ?? prepared[0] ?? '', values: s.__values ?? bound[0] ?? [] }));
      return [{ results: [{ lead_id: 'lead-1' }], success: true, meta: { changes: 0, duration: 0 } }];
    },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared, get batched() { return batched; } };
};

describe('native CRM caller-unit read plan', () => {
  it('exposes an allow-listed CRM_RD_* kind with no SQL on the wire', () => {
    expect(CRM_RD_COLLAB_NOTIFICATION_LEAD).toBe('crm_rd_collab_notification_lead');
  });

  it('plans the owner read in Rust and runs it on the caller unit', async () => {
    const fetchMock = stubFetch({ data: { statements: [{ sql: plannedSql, values: ['activity-1', 'company'] }] } });
    const { db } = fakeDb();
    const row = await nativeCrmReadFirst<{ lead_id: string }>(CRM_RD_COLLAB_NOTIFICATION_LEAD, ['activity-1', 'company'], db as never);
    expect(row).toEqual({ lead_id: 'lead-1' });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toContain('/api/v1/holdings/acme/crm/command-plan');
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('crm_rd_collab_notification_lead');
    expect(body.values).toEqual(['activity-1', 'company']);
    expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('switched adapter carries no SQL and returns the planned row', async () => {
    stubFetch({ data: { statements: [{ sql: plannedSql, values: ['activity-1', 'company'] }] } });
    const { db, prepared } = fakeDb();
    const row = await collaborationNotificationLead(db as never, 'activity-1', 'company');
    expect(row).toEqual({ lead_id: 'lead-1' });
    expect(prepared).toEqual([plannedSql]);
  });

  it('fails closed when the native transport is not configured', async () => {
    delete process.env[envKey];
    await expect(nativeCrmReadAll(CRM_RD_COLLAB_NOTIFICATION_LEAD, ['activity-1', 'company'], fakeDb().db as never))
      .rejects.toMatchObject({ status: 503 });
  });
});
