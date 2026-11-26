import { describe, expect, test, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

const native = vi.hoisted(() => ({ rows: vi.fn() }));
vi.mock('@erpx/server-quality/public/quality-evidence', () => ({
  nativeQualityEvidenceRows: (...args: unknown[]) => native.rows(...args),
  nativeQualityEvidenceConfigured: () => true,
}));

import { createExecutiveEvidenceApplication, type ExecutiveEvidencePorts } from '@erpx/server-runtime/application/executive/evidence';
import { ApiError } from '@erpx/server-runtime/public/http';

const root = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

const actor = {
  userId: 'u', name: 'Owner', email: 'owner@example.test', assignmentId: 'a', role: 'holding_owner',
  companyId: 'company', branchId: null, departmentId: null,
  holdingId: 'h1', holdingSlug: 'acme', holdingName: 'Acme',
  scope: { kind: 'holding', holdingId: 'h1', holdingSlug: 'acme', holdingName: 'Acme', companyId: 'company' },
  assignmentScope: 'holding', inherited: false, modules: ['quality'], assignments: [], workspaces: [],
} as unknown as WorkContext;
const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' };
const row = { id: 'i1', hold_id: 'h1', result: 'pass', created_at: 1000, reviewed_version: 2, product_name: 'Widget', warehouse_name: 'Main' };
const input = { metric: 'inspection-pass-rate', filters, query: 'q', limit: 1 };

function strict<T extends object>(facts: Partial<T>): T {
  return new Proxy(facts, {
    get(target, name) {
      if (name === 'then') return undefined;
      if (name in target) return target[name as keyof T];
      return () => { throw Error('Unexpected port: ' + String(name)); };
    },
  }) as T;
}
function app(repository: Partial<ExecutiveEvidencePorts['repository']> = {}, overrides: Partial<ExecutiveEvidencePorts> = {}) {
  return createExecutiveEvidenceApplication(strict<ExecutiveEvidencePorts>({
    clock: { now: () => 77 },
    digest: async () => 'evidence-query',
    repository: strict(repository),
    currentAuthority: async () => {},
    metricCatalogFor: async () => [{ id: 'inspection-pass-rate', available: true, label: 'Inspections', description: 'Inspections', unit: 'count', dateBasis: 'event' }],
    assertProductionModuleEnabled: async () => {},
    ...overrides,
  }));
}

describe('executive inspection evidence uses the strict native transport', () => {
  test('forwards the bounded context and preserves hasMore/items/nextCursor', async () => {
    native.rows.mockResolvedValueOnce([row, { ...row, id: 'i2' }]);
    const factsAudit = vi.fn(async () => {});
    const result = await app({ factsAudit }).searchExecutiveEvidence(actor, input) as {
      provider: string; metric: string; fields: string[]; hasMore: boolean; nextCursor: string | null;
      queryHash: string; items: { id: string; label: string; status: string; version: number; href: string }[];
    };
    expect(native.rows).toHaveBeenCalledWith(
      { holdingId: 'h1', holdingSlug: 'acme', companyId: 'company', branchId: null },
      { from: '2026-01-01', to: '2026-01-31', currency: 'USD', query: 'q', limit: 1, cursor: null },
    );
    expect(result).toMatchObject({
      provider: 'quality-records',
      metric: 'inspection-pass-rate',
      fields: ['result', 'created_at', 'reviewed_version', 'product_name', 'warehouse_name'],
      hasMore: true,
      queryHash: 'evidence-query',
    });
    expect(result.items).toHaveLength(1);
    expect(result.items[0]).toMatchObject({ id: 'i1', label: 'Widget', status: 'pass', version: 2, href: '/quality/h1' });
    expect(result.nextCursor).toBe(Buffer.from(JSON.stringify({ createdAt: 1000, id: 'i1' })).toString('base64url'));
    expect(factsAudit).toHaveBeenCalledOnce();
  });

  test('fails closed without native config and never reaches a TS SQL fallback', async () => {
    native.rows.mockRejectedValueOnce(new ApiError(503, 'NATIVE_QUALITY_EVIDENCE_CONFIGURATION_REQUIRED', 'The native Quality evidence service is unavailable.'));
    const retired = vi.fn(async () => ({ rows: [row] }));
    const repository = { factsAudit: vi.fn(async () => {}), factsSearchQualityInspectionEvidence: retired };
    await expect(app(repository).searchExecutiveEvidence(actor, input)).rejects.toMatchObject({
      status: 503, code: 'NATIVE_QUALITY_EVIDENCE_CONFIGURATION_REQUIRED',
    });
    expect(retired).not.toHaveBeenCalled();
  });

  test('the retired SQL member is deleted and the strict transport is exported', () => {
    expect(read('packages/server-quality/src/adapters/executive-main-evidence-sources.ts')).not.toContain('factsSearchQualityInspectionEvidence');
    const consumer = read('packages/server-runtime/src/application/executive/evidence.ts');
    expect(consumer).not.toContain('r.factsSearchQualityInspectionEvidence');
    expect(consumer).toContain('nativeQualityEvidenceRows');
    expect(read('packages/server-quality/src/adapters/quality-evidence-client.ts')).toContain('ERPX_NATIVE_QUALITY_EVIDENCE_CONFIG');
    const pkg = JSON.parse(read('packages/server-quality/package.json')) as { exports: Record<string, string> };
    expect(pkg.exports['./adapters/quality-evidence-client']).toBe('./src/adapters/quality-evidence-client.ts');
    expect(pkg.exports['./public/quality-evidence']).toBe('./src/public/quality-evidence.ts');
  });
});
