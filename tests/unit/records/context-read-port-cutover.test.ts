import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionColumnHeadSql } from '../../../packages/server-records/src/adapters/read-tail-r32-sql';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const records = read('packages/server-records/src/adapters/records.ts');
const nativeClient = read('packages/server-records/src/public/native-record-context.ts');
const rustPort = read('packages/server-records/src/adapters/record_context.rs');
const rustReadPath = rustPort.split('#[cfg(test)]')[0] ?? rustPort;

describe('Records record_context native read port cutover', () => {
  test('the former detail head still renders byte-identically through r32', () => {
    const rendered = projectionColumnHeadSql(
      'target_module,target_id,kind,ordinal',
      'record_reference_edges',
      ' WHERE host_module=? AND host_id=? ORDER BY ordinal',
    );
    expect(rendered).toBe(
      'SELECT target_module,target_id,kind,ordinal FROM record_reference_edges WHERE host_module=? AND host_id=? ORDER BY ordinal',
    );
    expect(sha(rendered)).toBe('06c718e5cfb225e12dd1b41a3bac38884d2af27749b39be73d972ef63a70c7b3');
    expect(records).not.toContain('recordContextDetailSql');
    expect(records).not.toContain('referenceEdgesSql');
    expect(records).toContain("import { projectionColumnHeadSql } from './read-tail-r32-sql';");
  });

  test('the exported context read goes through the native client with no TS SQL fallback', () => {
    expect(records).toContain('import { createNativeRecordContextClient, nativeRecordContextConfiguration, nativeRecordContextFailure }');
    expect(records).toContain('process.env.ERPX_NATIVE_RECORDS_CONFIG');
    expect(records).toContain('client.context(await requestHeaders()');
    expect(records).toContain('NATIVE_RECORD_CONTEXT_UNAVAILABLE');
    expect(records).toContain('nativeRecordContextFailure(error)');
    expect(records).not.toContain('application(undefined,actor).recordContext(actor, moduleId, id)');
    expect(nativeClient).not.toMatch(/SELECT /);
  });

  test('the five replaced repository reads are fail-closed native stubs', () => {
    const stubs = records.match(
      /readRecordContextEdgesbacklinksmentionssuggestionshistory\d*\(\) \{ throw new NativeApiError\(503,'NATIVE_RECORDS_UNAVAILABLE'/g,
    ) ?? [];
    expect(stubs).toHaveLength(5);
    expect(records).not.toMatch(/readRecordContextEdgesbacklinksmentionssuggestionshistory\d*\([^)]*\) \{\s*return database\.prepare/);
  });

  test('owner retirement:ts-statements is 11 (context cut 13 -> 12, row-projection fold 12 -> 11)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(7);
    expect(total).toBeLessThan(13);
  });

  test('the native Rust reader keeps bounded, read-only, serializable SQL', () => {
    expect(rustReadPath).toContain('SHOW transaction_isolation');
    expect(rustReadPath).toContain('RECORDS_CONTEXT_SERIALIZABLE_REQUIRED');
    for (const cap of ['LIMIT 51', 'LIMIT 201', 'LIMIT 21']) expect(rustReadPath).toContain(cap);
    expect(rustReadPath).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/);
  });
});
