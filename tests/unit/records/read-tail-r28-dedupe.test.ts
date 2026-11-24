import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  recordContextDetailSql,
  referenceEdgesSql,
} from '../../../packages/server-records/src/adapters/records';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe reference-edge SQL copied from records.ts:74 at claim. Each shape
// is frozen as a whole-string SHA pin: no table, projection, predicate,
// placeholder or ORDER BY token may drift through the fold into the detail
// generator, and the two binds per shape stay in the same order.
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

describe('Records read-tail r28 referenceEdgesSql family', () => {
  test('all three edge shapes render byte-identically and stay read-only', () => {
    const cases = [
      ['references', referenceEdgesSql('host', 'references')],
      ['backlinks', referenceEdgesSql('target', 'backlinks')],
      ['count', referenceEdgesSql('host', 'count')],
    ] as const;
    for (const [shape, actual] of cases) {
      expect(actual).toBe(pre[shape].sql);
      expect(sha(actual)).toBe(pre[shape].sha);
      expect(sha(pre[shape].sql)).toBe(pre[shape].sha);
      expect((actual.match(/SELECT /g) ?? []).length).toBe(1);
      readOnly(actual);
    }
  });

  test('binds and ordering are preserved through the shared detail generator', () => {
    expect(referenceEdgesSql('host', 'references')).toBe(
      recordContextDetailSql('record_reference_edges', 'target_module,target_id,kind,ordinal', 'host_module=? AND host_id=?', ' ORDER BY ordinal'));
    expect(referenceEdgesSql('target', 'backlinks')).toBe(
      recordContextDetailSql('record_reference_edges', 'host_module,host_id,kind', 'target_module=? AND target_id=?', ' ORDER BY created_at DESC LIMIT 50'));
    expect(referenceEdgesSql('host', 'count')).toBe(
      recordContextDetailSql('record_reference_edges', 'COUNT(*) AS count', 'host_module=? AND host_id=?', ''));

    for (const shape of ['references', 'backlinks', 'count'] as const) {
      const sql = referenceEdgesSql(shape === 'backlinks' ? 'target' : 'host', shape);
      expect(sql).toContain(pre[shape].binds);
      expect((sql.match(/\?/g) ?? []).length).toBe(2); // moduleId, id bound in that order
    }
    expect(referenceEdgesSql('host', 'references')).toContain(' ORDER BY ordinal');
    expect(referenceEdgesSql('target', 'backlinks')).toContain(' ORDER BY created_at DESC LIMIT 50');
    expect(referenceEdgesSql('host', 'count')).not.toContain(' ORDER BY ');
  });

  test('the edge family now delegates to the one detail generator literal; locks and ports stay independent', () => {
    expect(records.match(/const recordContextDetailSql\s*=/g) ?? []).toHaveLength(1);
    expect(records.match(/recordContextDetailSql\(/g) ?? []).toHaveLength(5); // mentions/suggestions/transitions + search references + edge family
    expect(records).toContain('const recordContextDetailSql = (table: string, columns: string, where: string, order: string) =>');
    expect(records).toContain('`SELECT ${columns} FROM ${table} WHERE ${where}${order}`');
    expect(records).toContain("return recordContextDetailSql('record_reference_edges', projection, where, order);");
    expect(records).not.toContain('`SELECT ${projection} FROM record_reference_edges WHERE ${where}${order}`');

    // r11-r27 families, cross-owner ports and all Records source locks stay pinned.
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    expect(records).toContain("import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';");
    expect(records).toContain('financeWorkspaceSearchScope} from "@erpx/server-finance/public/document-purchases"');
    expect(records).toContain("import { RecordsIdentityAccessOwnedFacts } from '@erpx/server-identity_access/public/records-records-owned-facts';");
    const legal = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(legal).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(legal).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(records).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Records read-tail r28 retirement check', () => {
  test('retirement:ts-statements folds the reference-edge family (24 -> 23)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r27 folded the search references SELECT into the record context detail
    // generator (25 -> 24); r28 folds the reference-edge family into that same
    // generator (24 -> 23). No lock, cross-owner port or native seam changed.
    expect(total).toBe(23);
    expect(total).toBeLessThan(24);
  });
});
