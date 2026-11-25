import { describe, expect, test, vi } from 'vitest';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { qualitySourceArmHead } from '../../../packages/server-quality/src/adapters/quality-source-arm-head';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

const rows = '(${qualityPurchaseSourceRows})';

// Pre-dedupe byte oracles captured at HEAD before the r33 fold, rendered by the
// retired qualitySourceUnionSelect helper. The r33 call sites pass the same
// comma-joined projection and source relation to qualitySourceArmHead, so every
// arm, the UNION ALL separators and the bind vector must stay byte-identical.
const arms = [
  ['s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind', `${rows} s`,
    `SELECT s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind FROM ${rows} s`,
    '79b3ca607d0ec6e8bad3dd3795fe54c6230562a5fea308a21a06f74717a7e4de'],
  ['a.id,s.company_id,s.branch_id,NULL::text,s.kind', `inventory_quality_alerts a JOIN ${rows} s ON s.id=a.hold_id`,
    `SELECT a.id,s.company_id,s.branch_id,NULL::text,s.kind FROM inventory_quality_alerts a JOIN ${rows} s ON s.id=a.hold_id`,
    '85426c2bf5fa96b4e96b6f9e9abc1d82ea8fa3712c6e7ceeab6fa6969254cba7'],
  ['id,company_id,NULL::text,NULL::text,\'advanced\'', 'inventory_quality_specs',
    'SELECT id,company_id,NULL::text,NULL::text,\'advanced\' FROM inventory_quality_specs',
    '354b5c9570bd70582d11626a0583fa7fa4c70b8d355be2a1bec8ae00b742474d'],
] as const;

const kpi = (pattern: RegExp) => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(pattern) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r33 UNION arm head dedupe', () => {
  test.each(arms)('arm %s is byte-identical to the pre-dedupe rendering', (columns, source, before, digest) => {
    const sql = qualitySourceArmHead(columns, source);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
    expect(sql.match(/SELECT /g)).toHaveLength(1);
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the three-arm UNION ALL recomposition is byte-identical and carries no bind', () => {
    const native = arms.map(([columns, source]) => qualitySourceArmHead(columns, source)).join(' UNION ALL ');
    const before = arms.map(([, , sql]) => sql).join(' UNION ALL ');
    expect(native).toBe(before);
    expect(sha(native)).toBe('5522cb7c304430f7ab4b8345187445dff84c68e9e0e37a64ef68354f7cd32b19');
    expect(native.match(/ UNION ALL /g)).toHaveLength(2);
    expect(native.match(/\?|\$[0-9]/g)).toBeNull();
  });

  test('record-purchases.ts routes all six arms through the shared head without the retired helper', () => {
    const records = read('adapters/record-purchases.ts');
    expect(records.match(/qualitySourceArmHead\(/g)).toHaveLength(6);
    expect(records.match(/UNION ALL /g)).toHaveLength(2);
    for (const [columns, source] of arms) {
      expect(records).toContain(`qualitySourceArmHead(${JSON.stringify(columns)},${source.includes('${') ? '`' + source + '`' : JSON.stringify(source)})`);
    }
    for (const [, , before] of arms) expect(records).not.toContain(before);
    expect(records).not.toContain('qualitySourceUnionSelect');
    expect(records).not.toContain('quality-source-union-sql');
    expect(existsSync(resolve(srcRoot, 'adapters/quality-source-union-sql.ts'))).toBe(false);
    expect(records).toContain('args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]');
    expect(records).toContain('nativeQualityReadFirst(RD_HOLD_EXISTS,[id],database)');
    expect(records).toContain('nativeQualityReadFirst(RD_SPEC_EXISTS,[id],database)');
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=29 baseline', () => {
    expect(kpi(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)).toBe(25);
    expect(kpi(/SELECT /g)).toBe(25);
  });
});
