import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, f), 'utf8');

const inbox = src('adapters/inbox.ts');
const application = src('application/inbox.ts');
const presentation = src('adapters/presentation-queries.ts');
const transport = src('public/native-inbox-page.ts');

describe('native-only inbox summary cutover', () => {
  it('replaces the TS page-loop aggregation in the inbox shared helper', () => {
    expect(inbox).toContain('import { nativeInboxSummarySSR }');
    expect(inbox).toContain('export function inboxSummary(_db: SqlDatabase, actor: WorkContext) { return exposed(() => nativeInboxSummarySSR(actor.holdingSlug)); }');
    expect(inbox).not.toContain('application(db).inboxSummary');
    expect(application).not.toContain('async function inboxSummary');
    expect(application).not.toContain('return { publishInboxEvent, inboxPage, inboxSummary,');
  });

  it('switches the in-scope presentation caller to the native owner path', () => {
    expect(presentation).toContain('import { nativeInboxSummarySSR } from "@erpx/server-collaboration/public/native-inbox-page"');
    expect(presentation).not.toContain('import { inboxSummary } from "@erpx/server-collaboration/public/inbox"');
    expect(presentation).not.toContain('inboxSummary(write');
    expect(presentation).toContain('inboxSummary: async (actor: WorkContext) => nativeInboxSummarySSR(actor.holdingSlug)');
  });

  it('keeps the shared export but routes it through the native transport', () => {
    expect(src('public/inbox.ts')).toContain('inboxSummary');
    expect(transport).toContain('export async function nativeInboxSummarySSR(holdingSlug: string): Promise<NativeInboxSummary>');
    expect(transport).toContain("NATIVE_COLLABORATION_UNAVAILABLE");
    expect(transport).toContain('NATIVE_COLLABORATION_CONFIGURATION_INVALID');
  });

  it('retirement:ts-statements: no TS summary aggregation branch remains', () => {
    const count = (text: string, re: RegExp) => (text.match(re) ?? []).length;
    expect(count(inbox, /application\(db\)\.inboxSummary/g)).toBe(0);
    expect(count(application, /async function inboxSummary/g)).toBe(0);
    expect(count(presentation, /inboxSummary\(write/g)).toBe(0);
  });
});
