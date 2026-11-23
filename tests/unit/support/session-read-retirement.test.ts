import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

const source = (path: string) => readFile(new URL(`../../../${path}`, import.meta.url), 'utf8');

describe('support session read retirement', () => {
  it('keeps the public session barrel type-only', async () => {
    const barrel = await source('packages/server-support/src/public/support-sessions.ts');
    expect(barrel).toContain('export type SupportSessionWorkspace');
    expect(barrel).toContain('export type SupportSessionDetail');
    expect(barrel).not.toMatch(/export\s*\{[^}]*supportSession/);
  });

  it('leaves no TypeScript session SELECT behind', async () => {
    const adapter = await source('packages/server-support/src/adapters/support-sessions.ts');
    expect(adapter).not.toContain('SELECT');
  });

  it('binds the Collaboration session target to the native Support session reader and drops the Work case', async () => {
    const notifications = await source('packages/server-collaboration/src/adapters/staff-notifications.ts');
    expect(notifications).toContain('createNativeSupportReader(raw).session(await headers(), actor.holdingSlug, id)');
    expect(notifications).not.toContain("import('@erpx/server-support/public/support-sessions')");
    const work = await source('packages/server-work_projection/src/adapters/work-items.ts');
    expect(work).not.toContain('supportSessionWork');
  });

  it('preserves native 403/404 denial codes on the session detail target', async () => {
    const { createNativeSupportReader } = await import(
      '../../../packages/feature-support/src/adapters/server/native-support-read'
    );
    const config = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3105',
      timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
    });
    const incoming = new Headers({ host: 'acme.example.test' });
    const denied = (code: string, status: number) => createNativeSupportReader(config, async () =>
      new Response(JSON.stringify({ error: { code, message: 'Denied.' } }), {
        status,
        headers: { 'content-type': 'application/json' },
      }));
    await expect(denied('SUPPORT_SESSION_NOT_FOUND', 404).session(incoming, 'acme', 'absent'))
      .rejects.toMatchObject({ status: 404, code: 'SUPPORT_SESSION_NOT_FOUND' });
    await expect(denied('PERMISSION_REQUIRED', 403).session(incoming, 'acme', 'absent'))
      .rejects.toMatchObject({ status: 403, code: 'PERMISSION_REQUIRED' });
  });
});
