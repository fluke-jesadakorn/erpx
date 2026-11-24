import { describe, expect, it, vi } from 'vitest';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

const { incoming } = vi.hoisted(() => ({ incoming: new Headers({ cookie: 'erpx-session=actual' }) }));
vi.mock('server-only', () => ({}));
vi.mock('next/headers', () => ({ headers: async () => incoming }));

import { createNextNativeSupportReader } from '../../../packages/feature-sales/src/adapters/server/next-native-support-read';

describe('Sales native Support read cutover', () => {
  it('binds the actor holding slug and incoming headers to the mounted native read port', async () => {
    const load = vi.fn(async (received: Headers, slug: string) => ({ slug, cookie: received.get('cookie') }));
    const actor = { holdingSlug: 'acme' } as WorkContext;
    const result = await createNextNativeSupportReader(actor)(load);
    expect(result).toEqual({ slug: 'acme', cookie: 'erpx-session=actual' });
    expect(load).toHaveBeenCalledExactlyOnceWith(incoming, 'acme');
  });

  it('keeps the Sales customer view on the native reader and removes the TypeScript workspace/recipient SQL', async () => {
    const { readFile } = await import('node:fs/promises');
    const source = await readFile(new URL('../../../packages/feature-sales/src/components/customers/customer-views.tsx', import.meta.url), 'utf8');
    expect(source).toContain('createNativeSupportReader');
    expect(source).toContain('createNextNativeSupportReader');
    expect(source).toContain('ERPX_NATIVE_SUPPORT_READ_CONFIG');
    expect(source).not.toContain('@erpx/server-support/public/support');
    const barrel = await readFile(new URL('../../../packages/server-support/src/public/support.ts', import.meta.url), 'utf8');
    expect(barrel).not.toContain('supportWorkspace,');
    expect(barrel).not.toContain('supportRecipients,');
    const adapter = await readFile(new URL('../../../packages/server-support/src/adapters/support.ts', import.meta.url), 'utf8');
    expect(adapter.match(/SELECT/g)).toHaveLength(1);
  });
});
