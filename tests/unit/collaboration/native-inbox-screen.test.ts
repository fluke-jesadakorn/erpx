import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const incoming = { headers: new Headers(), cookie: '' };
vi.mock('next/headers', () => ({
  headers: async () => incoming.headers,
  cookies: async () => ({ toString: () => incoming.cookie }),
}));

import {
  nativeInboxAvailable,
  nativeInboxNotificationsSSR,
  nativeInboxSummarySSR,
} from '../../../modules/collaboration/server/src/public/native-inbox-page';

const root = resolve(process.cwd());
const screen = readFileSync(
  resolve(root, 'platform/workplace/src/routes/holdings/[holdingSlug]/inbox/notifications/_screen.tsx'), 'utf8',
);
const presenter = readFileSync(
  resolve(root, 'modules/collaboration/server/src/adapters/notification-presentation.ts'), 'utf8',
);
const barrel = readFileSync(
  resolve(root, 'modules/collaboration/server/src/public/notification-presentation.ts'), 'utf8',
);
const client = readFileSync(
  resolve(root, 'modules/collaboration/server/src/public/native-inbox-page.ts'), 'utf8',
);

const ENV = 'ERPX_NATIVE_COLLABORATION_CONFIG';
const original = process.env[ENV];
const config = JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:3196', timeoutMs: 1000 });
const item = {
  id: 'delivery-1', eventId: 'event-1', message: 'POS source update', createdAt: 5,
  sourceType: 'collaboration_post', sourceId: 'post-1', category: 'work',
  href: null, readAt: null, archivedAt: null,
};
const page = { items: [item], nextCursor: null, hasMore: false };
const summary = { unread: 1, categories: { work: 1 } };
const ok = (data: unknown) => new Response(JSON.stringify({ data }), {
  status: 200, headers: { 'content-type': 'application/json' },
});

beforeEach(() => {
  process.env[ENV] = config;
  incoming.headers = new Headers({ 'x-erpx-context-company': 'company-1' });
  incoming.cookie = '__Host-erpx-product=session';
});
afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
  vi.unstubAllGlobals();
});

describe('workplace inbox screen native-only cut', () => {
  it('screen has no availability gate and no TS read fallback', () => {
    expect(screen).toContain('const data=await notificationWorkspace(actor,productHost)');
    expect(screen).not.toContain('nativeInboxAvailable');
    expect(screen).not.toContain('notificationWorkspace(actor,productHost):');
    expect(screen).not.toContain('notificationWorkspace(actor,productHost) :');
    expect(screen).not.toContain('inboxPage');
    expect(screen).not.toContain('inboxSummary');
    expect(screen).not.toContain('nativeInboxNotificationsSSR');
    expect(screen).not.toContain('nativeInboxSummarySSR');
  });

  it('presenter serves the native page and summary and deleted the TS SQL reads', () => {
    expect(presenter).toContain('nativeInboxNotificationsSSR(actor.holdingSlug,"?view=unread")');
    expect(presenter).toContain('nativeInboxSummarySSR(actor.holdingSlug)');
    expect(presenter).not.toContain('inboxPage(');
    expect(presenter).not.toContain('inboxSummary(');
    // Purchase-facts gate parity stays owner-side; only the inbox reads moved.
    expect(presenter).toContain('readCompanyPurchaseFacts(write,actor.companyId)');
    expect(presenter).toContain('"work.today","history"');
  });

  it('keeps the presenter export for SSR/test callers', () => {
    expect(barrel).toContain('export {notificationWorkspace}');
    expect(client).toContain('export async function nativeInboxNotificationsSSR');
    expect(client).toContain('export async function nativeInboxSummarySSR');
  });

  it('retirement:ts-statements: TS inbox-screen fallback reads fall 3 -> 0', () => {
    const count = (text: string, re: RegExp) => (text.match(re) ?? []).length;
    // BEFORE: 1 screen fallback behind nativeInboxAvailable() + 2 presenter
    // SQL reads (inboxPage, inboxSummary). AFTER: neither remains.
    const before = 3;
    const after = count(screen, /nativeInboxAvailable\(\)/g)
      + count(presenter, /inboxPage\(write,actor|inboxSummary\(write,actor/g);
    expect(count(screen, /notificationWorkspace\(actor,productHost\)/g)).toBe(1);
    expect(after).toBe(0);
    expect(after).toBeLessThan(before);
  });
});

describe('native inbox SSR transport', () => {
  it('reports availability from transport configuration', () => {
    expect(nativeInboxAvailable()).toBe(true);
    delete process.env[ENV];
    expect(nativeInboxAvailable()).toBe(false);
  });

  it('forwards cookies and context headers on the validated unread page', async () => {
    const calls: Array<{ url: string; headers: Headers }> = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, headers: init.headers as Headers });
      return ok(page);
    }));
    expect(await nativeInboxNotificationsSSR('acme', '?view=unread')).toEqual(page);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe('http://127.0.0.1:3196/api/v1/holdings/acme/inbox/notifications?view=unread');
    expect(calls[0]!.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(calls[0]!.headers.get('x-erpx-context-company')).toBe('company-1');
  });

  it('validates the summary payload and fails closed on malformed data', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ok(summary)));
    expect(await nativeInboxSummarySSR('acme')).toEqual(summary);
    vi.stubGlobal('fetch', vi.fn(async () => ok({ unread: '1', categories: {} })));
    await expect(nativeInboxSummarySSR('acme')).rejects.toMatchObject({ code: 'NATIVE_COLLABORATION_FAILED', status: 503 });
  });

  it('fails closed when native transport is not configured', async () => {
    delete process.env[ENV];
    await expect(nativeInboxSummarySSR('acme')).rejects.toMatchObject({
      code: 'NATIVE_COLLABORATION_UNAVAILABLE', status: 503,
    });
  });
});
