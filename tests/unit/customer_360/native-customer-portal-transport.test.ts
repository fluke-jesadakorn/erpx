import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { obligationFactHead } from '../../../packages/server-customer_360/src/adapters/obligation-fact-head';
import {
  createNativeCustomerPortalClient,
  nativeCustomerPortalConfiguration,
  NativeCustomerPortalError,
} from '../../../packages/server-customer_360/src/adapters/native-customer-portal';
import {
  RD_PORTAL_ACTION_BY_BINDING,
  RD_PORTAL_ACTION_BY_KEY,
  nativeC360ReadConfiguration,
  nativeC360ReadStatement,
} from '../../../packages/server-customer_360/src/adapters/native-customer-360-read';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44997',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = new Headers({
  host: 'acme.example.com',
  cookie: '__Host-erpx-product=session',
  origin: 'https://acme.example.com',
  'x-erpx-context-company': 'company',
  'x-erpx-workspace-branch': 'main',
  'x-erpx-untrusted-header': 'ignored',
});
const item = {
  binding_id: 'binding',
  binding_version: 2,
  order_id: 'order:1',
  number: 'SO-1',
  kind: 'sales',
  state: 'confirmed',
  due_date: '2026-12-01',
  currency: 'USD',
  updated_at: 100,
  fulfillments: [],
  actions: [],
  sharedFiles: [],
};
const projection = { items: [item], projectionVersion: 'customer-order-v1' };
const outcome = {
  id: 'customer-portal-action-1',
  bindingId: 'binding',
  orderId: 'order:1',
  action: 'acknowledge_delivery',
  status: 'recorded',
  createdAt: 100,
};
const ok = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { 'content-type': 'application/json' },
  });

describe('native customer-portal transport', () => {
  it('accepts one bounded private Holding port and rejects unsafe configuration', () => {
    expect(() => nativeCustomerPortalConfiguration(config())).not.toThrow();
    for (const overrides of [
      { privateBaseUrl: 'http://127.0.0.1:3100' },
      { privateBaseUrl: 'http://127.0.0.1:3101' },
      { privateBaseUrl: 'http://127.0.0.1:3201' },
      { privateBaseUrl: 'http://localhost:44997' },
      { privateBaseUrl: 'http://127.0.0.1:44997/path' },
      { timeoutMs: 100 },
      { holdings: [] },
      { holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com/path', holdingSlug: 'acme', holdingId: 'h001' }] },
      { holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'Acme', holdingId: 'h001' }] },
      {
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://other.example.com', holdingSlug: 'acme', holdingId: 'h002' },
        ],
      },
    ])
      expect(() => nativeCustomerPortalConfiguration(config(overrides))).toThrow(
        NativeCustomerPortalError,
      );
  });

  it('forwards only bounded session and Holding authority on a projection read', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: projection }));
    const result = await createNativeCustomerPortalClient(config(), send).projection(
      incoming,
      'acme',
    );
    expect(result).toEqual(projection);
    const [url, init] = send.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/customer-portal',
    );
    expect(init.method).toBe('GET');
    const headers = init.headers as Headers;
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-workspace-branch')).toBe('main');
    expect(headers.get('x-erpx-untrusted-header')).toBeNull();
  });

  it('binds an exact idempotent command to the private Holding target', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: outcome }, 201));
    const input = { bindingId: 'binding', expectedBindingVersion: 2, action: 'acknowledge_delivery' };
    const result = await createNativeCustomerPortalClient(config(), send).command(
      incoming,
      'acme',
      input,
      'command-key-1',
    );
    expect(result).toEqual(outcome);
    const [url, init] = send.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/customer-portal',
    );
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual(input);
    const headers = init.headers as Headers;
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('idempotency-key')).toBe('command-key-1');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  });

  it('rejects an unregistered host or a body over the bounded limit before transport', async () => {
    const send = vi.fn().mockResolvedValue(ok({ data: projection }));
    const client = createNativeCustomerPortalClient(config(), send);
    await expect(
      client.projection(new Headers({ host: 'foreign.example.com' }), 'acme'),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID' });
    await expect(client.projection(incoming, 'other')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_AUTHORITY_INVALID',
    });
    await expect(
      client.command(incoming, 'acme', { note: 'x'.repeat(50001) }, 'command-key-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_CUSTOMER_PORTAL_BODY_LIMIT', status: 413 });
    expect(send).not.toHaveBeenCalled();
  });

  it('surfaces bounded owner errors and rejects malformed payloads', async () => {
    const denied = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ error: { code: 'CUSTOMER_PORTAL_BINDING_EXPIRED', message: 'Expired.' } }),
          { status: 403, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    await expect(denied.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'CUSTOMER_PORTAL_BINDING_EXPIRED',
      status: 403,
    });

    const malformed = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockResolvedValue(ok({ data: { ...projection, projectionVersion: 'wrong' } })),
    );
    await expect(malformed.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_RESPONSE_INVALID',
    });

    const unavailable = createNativeCustomerPortalClient(
      config(),
      vi.fn().mockRejectedValue(new Error('offline')),
    );
    await expect(unavailable.projection(incoming, 'acme')).rejects.toMatchObject({
      code: 'NATIVE_CUSTOMER_PORTAL_UNAVAILABLE',
      status: 503,
    });
  });
});

const c360SrcRoot = resolve(__dirname, '../../../packages/server-customer_360/src');
const c360Src = (name: string) => readFileSync(resolve(c360SrcRoot, 'adapters', name), 'utf8');
const c360Sha = (s: string) => createHash('sha256').update(s).digest('hex');
const c360ReadOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};
// Pre-dedupe emitted heads copied verbatim from the r9 BEFORE=15 baseline. Whole-string
// byte identity is the oracle: no label, alias, quote or token may drift.
const obligationHeads = {
  action: {
    args: ['action', ' kind', 'request.id', 'request', ' fact'] as const,
    sql: "SELECT 'action' kind,request.id,to_jsonb(request) fact,NOT ",
    sha: '2d568f2bcd15a437312ed98b7f883e8a734d1ac7cdea574370e1011ce8964abc',
  },
  acknowledgement: {
    args: ['acknowledgement', '', 'ack.id', 'ack', ''] as const,
    sql: "SELECT 'acknowledgement',ack.id,to_jsonb(ack),NOT ",
    sha: '8a7d169761b7630de2877eeeaf3cde7a558dbe42ceee443963b12541736e04ed',
  },
} as const;

describe('Customer 360 obligation-fact query-head family', () => {
  it.each([
    ['action', obligationHeads.action],
    ['acknowledgement', obligationHeads.acknowledgement],
  ] as const)('%s renders byte-identically and stays read-only', (_name, member) => {
    const actual = obligationFactHead(...member.args as [string, string, string, string, string]);
    expect(actual).toBe(member.sql);
    expect(c360Sha(actual)).toBe(member.sha);
    c360ReadOnly(actual);
  });

  it('one generator literal with two call sites and no inlined projection head', () => {
    const shared = c360Src('obligation-fact-head.ts');
    expect(shared.match(/export const obligationFactHead\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const obligations = c360Src('package-obligations.ts');
    expect(obligations.match(/obligationFactHead\(/g) ?? []).toHaveLength(2);
    expect(obligations).not.toContain(obligationHeads.action.sql);
    expect(obligations).not.toContain(obligationHeads.acknowledgement.sql);
    expect(obligations).toContain("obligationFactHead('action',' kind','request.id','request',' fact')");
    expect(obligations).toContain("obligationFactHead('acknowledgement','','ack.id','ack','')");
  });

  it('r3/r5-r9 families, locks, native reads and cross-owner pins stay independent', () => {
    const reads = c360Src('customer-read-sources.ts');
    expect(reads).toContain('const scopedRow ='); // r5
    expect(reads).toContain('export function customerVisiblePredicate('); // r5
    expect(reads).toContain('export const companyScopeSource ='); // r8
    const timeline = c360Src('timeline.ts');
    expect(timeline).toContain('const eventSelect='); // r6
    expect(timeline).toContain('const eventRow='); // r7
    expect(reads).toContain('export const scopedProjection ='); // r13 head family
    expect(timeline).toContain('scopedProjection } from "./customer-read-sources"'); // r13
    const obligations = c360Src('package-obligations.ts');
    expect(obligations).toContain('const supportConclusionExists='); // r3
    expect(obligations).toContain('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('assertPortalAuthorityPackageObligationSources');
  });

  it('retirement r12: orphaned customer_portal_actions final-root projection is removed', () => {
    const ownerRoot = resolve(c360SrcRoot, '..');
    expect(existsSync(resolve(c360SrcRoot, 'adapters/final-root-owner-projection-sources.ts'))).toBe(false);
    expect(existsSync(resolve(c360SrcRoot, 'public/final-root-owner-projection-sources.ts'))).toBe(false);
    const pkg = JSON.parse(readFileSync(resolve(ownerRoot, 'package.json'), 'utf8')) as {
      exports: Record<string, unknown>;
    };
    expect(pkg.exports['./adapters/final-root-owner-projection-sources']).toBeUndefined();
    expect(pkg.exports['./public/final-root-owner-projection-sources']).toBeUndefined();
    const removed = '@erpx/server-customer_360/public/final-root-owner-projection-sources';
    const removedAdapter = '@erpx/server-customer_360/adapters/final-root-owner-projection-sources';
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) files.push(path);
      }
    };
    walk(c360SrcRoot);
    walk(resolve(ownerRoot, '../server-customer_authority/src'));
    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      expect(source).not.toContain(removed);
      expect(source).not.toContain(removedAdapter);
    }
    const consumer = readFileSync(
      resolve(ownerRoot, '../server-customer_authority/src/adapters/portal-access.ts'),
      'utf8',
    );
    expect(consumer).not.toContain('customer360ProjectionSources');
  });

  it('r15 native read plan: config is strict and unset fails closed', () => {
    const key = 'ERPX_NATIVE_CUSTOMER_360_READ_CONFIG';
    const previous = process.env[key];
    try {
      delete process.env[key];
      expect(nativeC360ReadConfiguration(process.env[key])).toBeUndefined();
      expect(() => nativeC360ReadConfiguration('{')).toThrow();
      expect(() => nativeC360ReadConfiguration(JSON.stringify({
        privateBaseUrl: 'http://127.0.0.1:3100', timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
      }))).toThrow(); // reserved port
      expect(() => nativeC360ReadConfiguration(JSON.stringify({
        privateBaseUrl: 'http://127.0.0.1:44998', timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
        portalHoldingSlug: 'missing',
      }))).toThrow();
      const cfg = nativeC360ReadConfiguration(JSON.stringify({
        privateBaseUrl: 'http://127.0.0.1:44998', timeoutMs: 1000,
        holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
        portalHoldingSlug: 'acme',
      }));
      expect(cfg?.portalHoldingSlug).toBe('acme');
    } finally {
      if (previous === undefined) delete process.env[key];
      else process.env[key] = previous;
    }
  });

  it('r15 native read plan: byte-pinned statements, strict admission and no TS fallback', async () => {
    const key = 'ERPX_NATIVE_CUSTOMER_360_READ_CONFIG';
    const previous = process.env[key];
    process.env[key] = config({
      privateBaseUrl: 'http://127.0.0.1:44998',
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
    const send = (payload: unknown, status = 200) => async () =>
      new Response(JSON.stringify(payload), { status, headers: { 'content-type': 'application/json' } });
    try {
      const head = 'SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE ';
      const byKey = await nativeC360ReadStatement(RD_PORTAL_ACTION_BY_KEY, ['request-key'], send({ data: { statement: { sql: head + 'request_key=?', values: ['request-key'] } } }));
      expect(byKey.sql).toBe(head + 'request_key=?');
      expect(byKey.values).toEqual(['request-key']);
      await expect(nativeC360ReadStatement(RD_PORTAL_ACTION_BY_BINDING, ['binding', 'acknowledge_delivery'],
        send({ data: { statement: { sql: head + 'binding_id=? AND action=?', values: ['binding', 'acknowledge_delivery'] } } }))).resolves.toMatchObject({
        sql: head + 'binding_id=? AND action=?',
      });
      await expect(nativeC360ReadStatement('c360_rd_unknown', [], send({}))).rejects.toThrow();
      await expect(nativeC360ReadStatement(RD_PORTAL_ACTION_BY_KEY, ['request-key'],
        send({ data: { statement: { sql: 'INSERT INTO customer_portal_actions VALUES (?)', values: ['x'] } } }))).rejects.toThrow();
      await expect(nativeC360ReadStatement(RD_PORTAL_ACTION_BY_KEY, ['request-key'], send({}, 503))).rejects.toThrow();
      const portal = readFileSync(resolve(c360SrcRoot, 'adapters/portal.ts'), 'utf8');
      expect(portal).toContain('nativeC360ReadStatement(');
      expect(portal).not.toContain('WHERE ${predicate}');
    } finally {
      if (previous === undefined) delete process.env[key];
      else process.env[key] = previous;
    }
  });

  it('retirement r15:ts-statements is strictly lower than BEFORE=11 (expected 10)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(c360SrcRoot);
    expect(total).toBe(10);
    expect(total).toBeLessThan(11);
  });
});
