import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionColumnHeadSql } from '../../../packages/server-records/src/adapters/read-tail-r32-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-retirement reference-edge SQL captured from the deleted generator. Each
// shape stays frozen as a whole-string SHA pin through its r32 delegation.
const pre = {
  references: {
    sql: 'SELECT target_module,target_id,kind,ordinal FROM record_reference_edges WHERE host_module=? AND host_id=? ORDER BY ordinal',
    sha: '06c718e5cfb225e12dd1b41a3bac38884d2af27749b39be73d972ef63a70c7b3',
    binds: 'host_module=? AND host_id=?',
  },
  backlinks: {
    sql: 'SELECT host_module,host_id,kind FROM record_reference_edges WHERE target_module=? AND target_id=? ORDER BY created_at DESC LIMIT 50',
    sha: '81cbb00f14ffb7930c5271408394a09a2e1a66f75195d1011b907970b50582cd',
    binds: 'target_module=? AND target_id=?',
  },
  count: {
    sql: 'SELECT COUNT(*) AS count FROM record_reference_edges WHERE host_module=? AND host_id=?',
    sha: '632c39b5e71db9e95b0e28e941232634b765d1f9e810e9d8ad375f992218d033',
    binds: 'host_module=? AND host_id=?',
  },
} as const;

const edgeSql = (scope: 'host' | 'target', shape: 'references' | 'backlinks' | 'count') => {
  const projection = shape === 'references' ? 'target_module,target_id,kind,ordinal'
    : shape === 'backlinks' ? 'host_module,host_id,kind' : 'COUNT(*) AS count';
  const where = scope === 'host' ? 'host_module=? AND host_id=?' : 'target_module=? AND target_id=?';
  const order = shape === 'references' ? ' ORDER BY ordinal'
    : shape === 'backlinks' ? ' ORDER BY created_at DESC LIMIT 50' : '';
  return projectionColumnHeadSql(projection, 'record_reference_edges', ` WHERE ${where}${order}`);
};

describe('Records read-tail r28 reference-edge shapes after generator retirement', () => {
  test('all three prior edge shapes still render byte-identically through r32', () => {
    const cases = [
      ['references', edgeSql('host', 'references')],
      ['backlinks', edgeSql('target', 'backlinks')],
      ['count', edgeSql('host', 'count')],
    ] as const;
    for (const [shape, actual] of cases) {
      expect(actual).toBe(pre[shape].sql);
      expect(sha(actual)).toBe(pre[shape].sha);
      expect(sha(pre[shape].sql)).toBe(pre[shape].sha);
      expect((actual.match(/SELECT /g) ?? []).length).toBe(1);
      readOnly(actual);
    }
    expect(edgeSql('host', 'references')).toContain(' ORDER BY ordinal');
    expect(edgeSql('target', 'backlinks')).toContain(' ORDER BY created_at DESC LIMIT 50');
    expect(edgeSql('host', 'count')).not.toContain(' ORDER BY ');
  });

  test('the detail generator is deleted; count is a fail-closed native stub', () => {
    expect(records).not.toContain('recordContextDetailSql');
    expect(records).not.toContain('referenceEdgesSql');
    expect(records).not.toMatch(/SELECT /);
    expect(records).toContain('readAddReferenceCount() { throw new NativeApiError(503');
    // r11-r12 heads and cross-owner ports stay pinned.
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain("import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';");
    expect(records).toContain('financeWorkspaceSearchScope} from "@erpx/server-finance/public/document-purchases"');
    expect(records).toContain("import { RecordsIdentityAccessOwnedFacts } from '@erpx/server-identity_access/public/records-records-owned-facts';");
    const legal = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(legal).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(legal).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r28 retirement check', () => {
  test('retirement:ts-statements folds the retained detail family (11 -> 10)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(7);
    expect(total).toBeLessThan(11);
  });
});
