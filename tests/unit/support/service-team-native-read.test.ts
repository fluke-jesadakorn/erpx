import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));

const source = (path: string) => readFile(new URL(`../../../${path}`, import.meta.url), 'utf8');
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3997',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
});
const ok = (sql: string, values: unknown[]) =>
  new Response(JSON.stringify({ data: { statements: [{ sql, values }] } }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe('support service-team coverage native read', () => {
  it('carries no retired owner SQL in the switched adapters', async () => {
    const history = await source('packages/server-support/src/adapters/flow-settings-facts.ts');
    const teams = await source('packages/server-support/src/adapters/service-team-reads.ts');
    expect(history).not.toContain('SELECT');
    expect(teams).not.toContain('service_team_coverage c JOIN');
    expect(teams).not.toContain('service_team_revisions');
  });

  it('plans service coverage history in Rust and executes on the caller unit', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{
      results: [{ title: 'Service coverage', version: 2, actor: 'owner', at: 1, kind: 'Service coverage' }],
      success: true,
      meta: { changes: 0, duration: 0 },
    }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT t.name title FROM service_team_revisions r', ['company']),
    );
    try {
      const { serviceCoverageSettingsHistory } = await import('@erpx/server-support/adapters/flow-settings-facts');
      await expect(serviceCoverageSettingsHistory(db, 'company')).resolves.toMatchObject([{ version: 2 }]);
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_service_coverage_history',
        values: ['company'],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });

  it('plans service team coverage in Rust with the owner scope values', async () => {
    const prepare = vi.fn(() => ({ bind: vi.fn() }));
    const batch = vi.fn(async () => [{ results: [], success: true, meta: { changes: 0, duration: 0 } }]);
    const db = { prepare, batch } as unknown as SqlDatabase;
    process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG = config;
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok('SELECT t.* FROM service_team_coverage c', ['company', 'it', 'company', null, null]),
    );
    try {
      const { resolveServiceTeam } = await import('@erpx/server-support/adapters/service-team-reads');
      const scope = { company_id: 'company', branch_id: null, department_id: null } as never;
      await expect(resolveServiceTeam(db, scope, 'it')).resolves.toBeNull();
      const call = fetchMock.mock.calls[0]!;
      expect(String(call[0])).toContain('/support/command-plan');
      expect(JSON.parse(String(call[1]!.body))).toMatchObject({
        kind: 'support_rd_service_team_coverage',
        values: ['company', 'it', 'company', null, null],
      });
    } finally {
      fetchMock.mockRestore();
      delete process.env.ERPX_NATIVE_SUPPORT_PLAN_CONFIG;
    }
  });
});
