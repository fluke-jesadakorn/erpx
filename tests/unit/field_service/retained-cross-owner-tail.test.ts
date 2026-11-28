import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const repo = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(repo, path), 'utf8');
const walk = (dir: string): string[] => {
  const files: string[] = [];
  for (const entry of readdirSync(resolve(repo, dir), { withFileTypes: true })) {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) files.push(...walk(path));
    else if (/\.(ts|tsx)$/.test(entry.name)) files.push(path);
  }
  return files;
};

const RECORDS_PORT = '@erpx/field-service-server/public/record-purchases';
const FINANCE_PORT = '@erpx/field-service-server/public/finance-record-sources';

/** Exact cross-owner importers of the two retained caller-unit ports. */
const IMPORTERS: Record<string, string[]> = {
  'modules/records/server/src/adapters/records.ts': [RECORDS_PORT],
  'platform/runtime/server/src/adapters/executive/metrics.ts': [RECORDS_PORT],
  'modules/finance/server/src/adapters/attachment-purchases.ts': [FINANCE_PORT],
  'modules/finance/server/src/adapters/document-purchases.ts': [FINANCE_PORT],
  'modules/finance/server/src/adapters/package-obligations.ts': [FINANCE_PORT],
  'modules/finance/server/src/adapters/service-payment-purchases.ts': [FINANCE_PORT],
  'tests/integration/shared.inbox/inbox.test.ts': [RECORDS_PORT],
};

describe('field_service retained cross-owner records/finance tail', () => {
  test('the retained caller-unit ports have exactly the known importers', () => {
    const observed: Record<string, string[]> = {};
    for (const file of [...walk('packages'), ...walk('apps'), ...walk('tests')]) {
      if (file.startsWith('modules/field-service/server/') || file.startsWith('tests/unit/field_service/')) continue;
      let source: string;
      try {
        source = read(file);
      } catch (error) {
        // A sibling lane may delete a file between the directory scan and the
        // read; that file cannot hold a retained field_service port.
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') continue;
        throw error;
      }
      const ports = [RECORDS_PORT, FINANCE_PORT].filter(port => source.includes(port));
      if (ports.length) observed[file] = ports;
    }
    expect(observed).toEqual(IMPORTERS);
  });

  test('records registry fallback still calls the owner fail-closed probes', () => {
    const records = read('modules/records/server/src/adapters/records.ts');
    expect(records).toContain(`import {serviceSourceReadScope,requireServiceWorkspaceRecordRead} from "${RECORDS_PORT}"`);
    expect(records.match(/serviceSourceReadScope\(database,actor,"records"\)/g)).toHaveLength(3);
    expect(records).toContain('requireServiceWorkspaceRecordRead(database,actor,moduleId,id)');
    const runtime = read('platform/runtime/server/src/adapters/executive/metrics.ts');
    expect(runtime).toContain(`import {serviceMetricHistoryAccess} from "${RECORDS_PORT}"`);
    expect(runtime).toContain('serviceMetricHistoryAccess(db,a)');
  });

  test('finance still consumes the owner-fixed lineage contract and its aliases', () => {
    expect(read('modules/finance/server/src/adapters/attachment-purchases.ts')).toContain(`import {nativeFieldFinanceSourceReadLocks} from "${FINANCE_PORT}"`);
    expect(read('modules/finance/server/src/adapters/document-purchases.ts')).toContain(`import {nativeFieldFinanceLineage,nativeFieldFinanceSourceReadLocks} from "${FINANCE_PORT}"`);
    expect(read('modules/finance/server/src/adapters/package-obligations.ts')).toContain(`import {nativeFieldFinanceSourceReadLocks} from "${FINANCE_PORT}"`);
    expect(read('modules/finance/server/src/adapters/service-payment-purchases.ts')).toContain(`import {nativeFieldFinanceLineage} from "${FINANCE_PORT}"`);
    // Cross-owner PG oracle imports the legacy alias by name from the TS module.
    const oracle = read('tests/integration/finance/rust-journal-source-postgres.test.rs');
    expect(oracle).toContain('server-field_service/src/adapters/finance-record-sources.ts');
    expect(oracle).toContain('m.fieldServiceFinanceRecordSources.invoice');
  });

  test('owner public surfaces still serve every retained symbol', () => {
    const records = read('modules/field-service/server/src/public/record-purchases.ts');
    for (const name of ['serviceSourceReadScope', 'requireServiceWorkspaceRecordRead', 'serviceMetricHistoryAccess']) expect(records).toContain(name);
    const finance = read('modules/field-service/server/src/public/finance-record-sources.ts');
    for (const name of ['nativeFieldFinanceLineage', 'nativeFieldFinanceSourceReadLocks', 'fieldServiceFinanceRecordSources', 'fieldServiceFinanceSourceReadLocks']) expect(finance).toContain(name);
  });

  test('both ports are backed by Rust-owned contracts with no TS SQL in owner or feature sources', () => {
    const rust = read('modules/field-service/server/src/adapters/record_probe.rs');
    expect(rust).toContain('include_str!("record-probe-sources.json")');
    const financeRust = read('modules/field-service/server/src/adapters/finance_lineage.rs');
    expect(financeRust).toContain('include_str!("finance-lineage-sources.json")');
    for (const root of ['modules/field-service/server/src', 'modules/field-service/ui/src']) {
      let total = 0;
      for (const file of walk(root)) total += (read(file).match(/SELECT /g) ?? []).length;
      expect(total, root).toBe(0);
    }
  });
});
