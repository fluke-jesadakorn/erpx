import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
vi.mock('server-only', () => ({}));
const incoming = { value: { host: 'acme.jesadakorn.com', company: 'company-1' } as { host: string | null; company: string | null } };
vi.mock('next/headers', () => ({
  headers: async () => new Headers({
    ...(incoming.value.host === null ? {} : { host: incoming.value.host }),
    ...(incoming.value.company === null ? {} : { 'x-erpx-context-company': incoming.value.company }),
  }),
}));
import {
  nativeLane16Configured,
  pushEndpointOwner,
  pushPreferences,
  pushSessionSubscriptionCount,
  sentNoticeDeliveryCount,
  sentNoticeRow,
  sentNoticeRows,
  taskSocialOrigin,
  type Lane16Actor,
} from '../../../packages/server-collaboration/src/adapters/native-lane-16/client';

const ENV = 'ERPX_NATIVE_LANE_16_READ_CONFIG';
const original = process.env[ENV];
const holdings = [
  { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
  { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
  { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
];
const config = JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings });
const db = {} as never;
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const ok = (data: unknown) => new Response(JSON.stringify({ data }), {
  status: 200, headers: { 'content-type': 'application/json' },
});
const transport = (data: unknown) =>
  vi.fn(async () => ok(data));
const actor: Lane16Actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1' };

beforeEach(() => {
  process.env[ENV] = config;
  incoming.value = { host: 'acme.jesadakorn.com', company: 'company-1' };
});
afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
  vi.unstubAllGlobals();
});

describe('native Collaboration lane 16 sent-notice facts', () => {
  it('transports each complete projection on the exact registered route', async () => {
    const calls: { url: URL; headers: Record<string, string>; body: Record<string, unknown> }[] = [];
    const fn = vi.fn(async (url: URL, headers: Record<string, string>, raw: string) => {
      const body = JSON.parse(raw) as Record<string, unknown>;
      calls.push({ url, headers, body });
      const data = (() => {
        switch (body.kind) {
          case 'notices': return {
            kind: 'notices',
            rows: [{ id: 'notice-1', title: 'Hello', accepted_count: 2, created_at: 5, delivered_count: 1 }],
          };
          case 'notice': return {
            kind: 'notice',
            row: {
              id: 'notice-1', title: 'Hello', body: 'Body', recipient_count: 2,
              created_at: 5, acceptedCount: 2, completedCount: 1,
            },
          };
          case 'delivery-count': return { kind: 'delivery-count', n: 1 };
          default: return { kind: 'task-origin', row: { post_id: 'post-1' } };
        }
      })();
      return ok(data);
    });

    expect(await taskSocialOrigin(db, 'task-1', actor, fn)).toEqual({ post_id: 'post-1' });
    expect(await sentNoticeRows(db, 'company-1', 'user-1', actor, fn)).toEqual({
      results: [{ id: 'notice-1', title: 'Hello', accepted_count: 2, created_at: 5, delivered_count: 1 }],
    });
    expect(await sentNoticeRow(db, 'notice-1', 'company-1', 'user-1', actor, fn)).toMatchObject({
      id: 'notice-1', recipient_count: 2, acceptedCount: 2, completedCount: 1,
    });
    expect(await sentNoticeDeliveryCount(db, 'company-1', 'notice:notice-1', actor, fn)).toEqual({ n: 1 });

    expect(calls).toHaveLength(4);
    for (const call of calls) {
      expect(call.url.toString()).toBe(
        'http://127.0.0.1:3198/api/v1/holdings/acme/collaboration/native-lane-16/sent-notices',
      );
      expect(call.headers.host).toBe('acme.jesadakorn.com');
      expect(call.headers.origin).toBe('https://acme.jesadakorn.com');
      expect(call.headers['x-erpx-context-company']).toBe('company-1');
      expect(call.headers['x-erpx-context-holding']).toBe('h001');
      expect(call.headers['x-erpx-context-user']).toBe(
        call.body.kind === 'notices' || call.body.kind === 'notice' ? 'user-1' : undefined,
      );
    }
    expect(nativeLane16Configured()).toBe(true);
  });

  it('selects every registered Holding by WorkContext, never the first one', async () => {
    const seen: URL[] = [];
    const fn = vi.fn(async (url: URL) => {
      seen.push(url);
      return ok({ kind: 'notices', rows: [] });
    });
    incoming.value = { host: 'northstar.jesadakorn.com', company: 'company-2' };
    await sentNoticeRows(db, 'company-2', 'user-2', { holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-2' }, fn);
    expect(seen[0]!.toString()).toContain('/holdings/northstar/');

    incoming.value = { host: 'erpx-internal.jesadakorn.com', company: 'company-3' };
    await sentNoticeRows(db, 'company-3', 'user-3', { holdingId: 'h003', holdingSlug: 'erpx', companyId: 'company-3' }, fn);
    expect(seen[1]!.toString()).toContain('/holdings/erpx/');

    // Legacy caller without a WorkContext is pinned by the exact incoming Host.
    incoming.value = { host: 'acme.jesadakorn.com', company: 'company-1' };
    await sentNoticeRows(db, 'company-1', 'user-1', undefined, fn);
    expect(seen[2]!.toString()).toContain('/holdings/acme/');
  });

  it('fails closed on unknown WorkContext, unknown Host and malformed config', async () => {
    await expect(sentNoticeRows(db, 'company-9', 'user-1', { holdingId: 'h009', holdingSlug: 'ghost', companyId: 'company-9' }, transport([])))
      .rejects.toMatchObject({ status: 409, code: 'LANE_16_WORK_CONTEXT_CHANGED' });
    incoming.value = { host: 'evil.example.test', company: 'company-1' };
    await expect(sentNoticeRows(db, 'company-1', 'user-1', undefined, transport([])))
      .rejects.toMatchObject({ status: 409, code: 'LANE_16_WORK_CONTEXT_CHANGED' });
    incoming.value = { host: 'acme.jesadakorn.com', company: 'company-1' };
    for (const bad of [
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings: [] }),
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings: Array.from({ length: 65 }, (_, i) => ({ ...holdings[0]!, holdingId: `h${i}` })) }),
      JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3198', timeoutMs: 10000, holdings: [{ ...holdings[0]!, host: 'wrong.example.test' }] }),
    ]) {
      process.env[ENV] = bad;
      await expect(sentNoticeRows(db, 'company-1', 'user-1', actor, transport([])))
        .rejects.toMatchObject({ status: 503 });
    }
  });

  it('transports the three push registration reads with strict shapes', async () => {
    const calls: { url: URL; headers: Record<string, string>; body: Record<string, unknown> }[] = [];
    const prefs = {
      enabled: true, work_enabled: true, mention_enabled: true, system_enabled: true,
      calendar_enabled: false, notice_enabled: false, quiet_start_minute: null, quiet_end_minute: null,
    };
    const fn = vi.fn(async (url: URL, headers: Record<string, string>, raw: string) => {
      calls.push({ url, headers, body: JSON.parse(raw) as Record<string, unknown> });
      const kind = calls[calls.length - 1]!.body.kind;
      return ok(kind === 'push-preferences'
        ? { kind, row: prefs }
        : kind === 'push-session-count'
          ? { kind, n: 2 }
          : { kind, row: null });
    });
    expect(await pushPreferences(db, 'company-1', 'user-1', actor, fn)).toEqual(prefs);
    expect(await pushSessionSubscriptionCount(db, 'company-1', 'user-1', 'session-1', actor, fn)).toEqual({ n: 2 });
    expect(await pushEndpointOwner(db, 'company-1', 'hash-1', actor, fn)).toBeNull();
    expect(calls.map(c => c.body)).toEqual([
      { kind: 'push-preferences', companyId: 'company-1', userId: 'user-1' },
      { kind: 'push-session-count', companyId: 'company-1', userId: 'user-1', sessionId: 'session-1' },
      { kind: 'push-endpoint-owner', companyId: 'company-1', endpointHash: 'hash-1' },
    ]);
    for (const call of calls) {
      expect(call.url.toString()).toContain('/collaboration/native-lane-16/push-settings');
      expect(call.headers['x-erpx-context-company']).toBe('company-1');
      expect(call.headers['x-erpx-context-holding']).toBe('h001');
    }
  });

  it('fails closed on malformed push projections', async () => {
    for (const payload of [
      { data: { kind: 'push-preferences', row: { enabled: 'true' } } },
      { data: { kind: 'push-session-count', n: '2' } },
      { data: { kind: 'push-endpoint-owner', row: { id: 'sub-1' } } },
    ]) {
      await expect(pushPreferences(db, 'company-1', 'user-1', actor, transport(payload)))
        .rejects.toMatchObject({ status: 503 });
    }
  });

  it('pins the SQL and result oracles shared with the Rust lane', () => {
    const src = readFileSync(
      'packages/server-collaboration/src/adapters/native_lane_16/postgres.rs', 'utf8',
    );
    const needle = (name: string) => {
      const match = src.match(new RegExp(`pub const ${name}: &str = r#"([\\s\\S]*?)"#;`))
        ?? src.match(new RegExp(`pub const ${name}: &str =\\s*"([^"]+)"`));
      expect(match).not.toBeNull();
      return match![1]!.replace(/\\n/g, '\n');
    };
    expect(sha(needle('SQL_NOTICES'))).toBe('bbe9f92531ee4e811581dd0ea3be5694208a251773270ca54d14f60dc27d81d9');
    expect(sha(needle('SQL_NOTICE'))).toBe('3c600b56ca7e7d793fb77778f18c3494145825402e3f05035113f56e10bdd914');
    expect(sha(needle('SQL_DELIVERY_COUNT'))).toBe('70e04278860ac51d948b29b676e563b0b8d9d63f53e742ee89ca99e83ab83b19');
    expect(sha(needle('SQL_TASK_ORIGIN'))).toBe('908c925016c228ff04f3934739a7da0877bccda710e3107bc8588642034d6f14');
    const render = (sql: string) => { let n = 0; return sql.replace(/\?/g, () => `$${++n}`); };
    expect(sha(render(needle('SQL_NOTICES')))).toBe('d6cee0018f2cbe771d5960313020f476ea2659c2280e9de8a0e6a551068d7dcd');
    expect(sha(needle('RESULT_ORACLE_JSON'))).toBe('2ceaa6b93c6b879c5e5a718e3f8fbc7ae75994ab09a0ec73727807b0cdd3857b');
    const http = readFileSync(
      'packages/server-collaboration/src/adapters/native_lane_16/http.rs', 'utf8',
    );
    expect(http.match(/#\[serde\(rename_all = "camelCase", deny_unknown_fields\)\]/g)).toHaveLength(2);
    for (const field of [
      'company_id', 'sender_id', 'notice_id', 'event_key', 'task_id',
      'user_id', 'session_id', 'endpoint_hash',
    ]) expect(http).toContain(field);
  });

  it('emits no SQL from the replaced TypeScript adapter or the client', () => {
    for (const path of [
      'packages/server-collaboration/src/adapters/people-app-entrypoint-facts.ts',
      'packages/server-collaboration/src/adapters/native-lane-16/client.ts',
    ]) {
      const source = readFileSync(path, 'utf8');
      expect(source).not.toMatch(/SELECT\s|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|pg_advisory/i);
    }
    const push = readFileSync('packages/server-collaboration/src/adapters/web-push.ts', 'utf8');
    expect(push).not.toContain('SELECT * FROM push_preferences');
    expect(push).not.toContain('SELECT id,user_id,session_id FROM push_subscriptions');
    expect(push).not.toContain("countIntNFrom('push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?')");
  });

  it('fails closed on missing config, 5xx and malformed payloads', async () => {
    delete process.env[ENV];
    await expect(sentNoticeRows(db, 'company-1', 'user-1', actor))
      .rejects.toMatchObject({ status: 503, code: 'LANE_16_READ_CONFIGURATION_REQUIRED' });

    process.env[ENV] = config;
    vi.stubGlobal('fetch', vi.fn(async () => new Response('down', { status: 503 })));
    await expect(sentNoticeRows(db, 'company-1', 'user-1', actor))
      .rejects.toMatchObject({ status: 503, code: 'LANE_16_NATIVE_READ_UNAVAILABLE' });

    for (const payload of [
      {},
      { data: { kind: 'notices', rows: {} } },
      { data: { kind: 'notice', rows: [] } },
      { data: { kind: 'notices', rows: [{ id: 'notice-1', title: 'Hello', accepted_count: 2, created_at: 5, delivered_count: '1' }] } },
      { data: { kind: 'delivery-count', n: '1' } },
      { data: { kind: 'task-origin', row: { post_id: 'post-1', injected: 'SELECT 1' } } },
    ]) {
      await expect(sentNoticeRows(db, 'company-1', 'user-1', actor, transport(payload)))
        .rejects.toMatchObject({ status: 503, code: 'LANE_16_NATIVE_READ_RESPONSE_INVALID' });
    }
  });

  it('records the final owner SELECT KPI after lanes 15 and 16', () => {
    const root = 'packages/server-collaboration/src';
    const files = readdirSync(root, { recursive: true }).filter((name): name is string =>
      typeof name === 'string' && name.endsWith('.ts'));
    let count = 0;
    for (const file of files) {
      count += (readFileSync(`${root}/${file}`, 'utf8').match(/SELECT /g) ?? []).length;
    }
    expect(count).toBe(247);
  });
});
