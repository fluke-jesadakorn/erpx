import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  requireServiceWorkspaceRecordRead,
  serviceMetricHistoryAccess,
  serviceSourceReadScope,
} from '../../../modules/field-service/server/src/adapters/record-purchases';

const srcRoot = resolve(__dirname, '../../../modules/field-service/server/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const probes = JSON.parse(read('adapters/record-probe-sources.json')) as {
  sourceReadScopeProbe: string;
  workspaceRecordProbe: string;
};

// Byte oracles captured from the retired r3 TypeScript literals in
// `record-purchases.ts` before the native-owner contract cutover.
const IDS = 'SELECT id FROM field_service_agreements UNION SELECT id FROM field_service_orders';
const BEFORE = {
  sourceReadScopeProbe: `SELECT 1 FROM (${IDS}) native_field_record WHERE native_field_record.id=workspace_records.id`,
  workspaceRecordProbe: `SELECT 1 FROM (${IDS}) native_field_record WHERE id=?`,
} as const;

describe('field_service read-tail r3 native record probe', () => {
  test('native JSON contract keeps the exact retired probe statements', () => {
    expect(probes.sourceReadScopeProbe).toBe(BEFORE.sourceReadScopeProbe);
    expect(probes.workspaceRecordProbe).toBe(BEFORE.workspaceRecordProbe);
    expect(sha(probes.sourceReadScopeProbe)).toBe('79c64976f6e5d07073d8053d10924015cfc6f81779d936ead86b33ebcef17eea');
    expect(Object.keys(probes)).toEqual(['sourceReadScopeProbe', 'workspaceRecordProbe']);
    for (const sql of Object.values(probes)) {
      expect(sql).toContain('native_field_record');
      expect(sql).not.toMatch(/;|FOR (UPDATE|SHARE)|INSERT INTO|DELETE FROM/i);
    }
    expect(probes.workspaceRecordProbe.match(/\?/g)).toHaveLength(1);
  });

  test('TypeScript adapter carries no SQL literal and no caller-built probe', () => {
    const adapter = read('adapters/record-purchases.ts');
    expect(adapter).not.toContain('SELECT ');
    expect(adapter).not.toContain('nativeIds');
    expect(adapter).not.toContain('nativeFieldRecordProbe');
    expect(adapter).toContain('import contract from "./record-probe-sources.json"');
    expect(adapter).toContain('probes.sourceReadScopeProbe');
    expect(adapter).toContain('probes.workspaceRecordProbe');
  });

  test('read scope and single-record guard execute the owner-fixed statements', async () => {
    const prepared: string[] = [];
    const bound: unknown[][] = [];
    const database = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind(...args: unknown[]) {
            bound.push(args);
            return { first: async () => null };
          },
        };
      },
    };
    const actor = {} as never;
    const scope = await serviceSourceReadScope(database as never, actor, 'records');
    expect(scope.where).toBe(`(workspace_records.module_id<>'field_service' AND NOT EXISTS(${probes.sourceReadScopeProbe}))`);
    expect(scope.args).toEqual([]);
    expect(prepared).toEqual([]);
    await expect(requireServiceWorkspaceRecordRead(database as never, actor, 'field_service', 'missing')).rejects.toMatchObject({ status: 503, code: 'FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED' });
    expect(prepared).toEqual([]);
    await expect(requireServiceWorkspaceRecordRead(database as never, actor, 'projects', 'missing')).resolves.toBeUndefined();
    expect(prepared).toEqual([probes.workspaceRecordProbe]);
    expect(bound).toEqual([['missing']]);
  });

  test('single-record guard fails closed when a generic image matches a native id', async () => {
    const database = {
      prepare: () => ({ bind: () => ({ first: async () => ({ id: 'native' }) }) }),
    };
    await expect(requireServiceWorkspaceRecordRead(database as never, {} as never, 'projects', 'native')).rejects.toMatchObject({ status: 503, code: 'FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED' });
  });

  test('metric history access still delegates to the native report reader', async () => {
    const native = await import('../../../modules/field-service/server/src/adapters/native-executive');
    const spy = vi.spyOn(native, 'nativeFieldReportAccess').mockResolvedValue(true);
    const access = await serviceMetricHistoryAccess({} as never, {} as never);
    expect(access).toEqual({ 'open-service-orders': true, 'service-billing-releases': true, 'field-visit-completion': true });
    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  test('retirement:ts-statements is 0 after r3 (BEFORE=3)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(0);
  });
});
