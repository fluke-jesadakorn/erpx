import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { qualityLineageSeedSql, qualityLineageStepSql } from '../../../packages/server-quality/src/adapters/quality-lineage-recursion-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Pre-dedupe byte oracles captured from the r31/r14 source through the live
// module. Both heads opened with the same `SELECT <columns>,` prefix; the r32
// fold routes both through one owner-local head helper. Every render below is
// unchanged, including spacing and the zero/depth+1 literals.
const seeds = [
  [['h.*', 'inventory_quality_holds h'], 'SELECT h.*,0 depth FROM inventory_quality_holds h', '184ac40a35836065eb5d01aa52586c3ac7d2e57a02521d99ca7323427ba47bdb'],
  [['id hold_id,id root_id', 'inventory_quality_holds'], 'SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds', 'ff7dbe771cf290c6e351789ce29ed6c986070867f851cfb6a6473c9deacb1c11'],
  [['anchors.id,anchors.source_company', 'anchors'], 'SELECT anchors.id,anchors.source_company,0 depth FROM anchors', '89c2c0a9908645fefdd2fb5d78c6403d75aceeeb9fbbe6ea29188b8a0b1ddf59'],
  [['id', 'anchors'], 'SELECT id,0 depth FROM anchors', '0e1caae6813a3d2cdbe39b2c511cc102c637ae72aba14a70f5b8216f46442014'],
] as const;
const steps = [
  [['parent.*', 'child'], 'SELECT parent.*,child.depth+1 FROM lineage child', 'bb776e2ba5ea7b757753e9fcd6deb33eeadbf50ea593647537f815743d8a7af1'],
  [['walk.hold_id,parent.id', 'walk'], 'SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk', 'efd1abdd2b0975be75856675c392314b2fa7169a9362e781e5d4064e6656133a'],
  [['child.id,parent.source_company', 'parent'], 'SELECT child.id,parent.source_company,parent.depth+1 FROM lineage parent', 'a3e35247eb4189d7ab1f9c894a03c606228a55a2443fe5d8430794980ef79e9b'],
  [['child.id', 'parent'], 'SELECT child.id,parent.depth+1 FROM lineage parent', '7f8512eef4e23280e5ec0879a8052fdad7731b649dd8ebf3d9ad5ba073688fb5'],
] as const;

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) {
        total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    }
  };
  walk(srcRoot);
  return total;
};

describe('quality read-tail r32 lineage head dedupe', () => {
  test.each(seeds)('anchor seed %s is byte-identical to the pre-dedupe rendering', (args, before, digest) => {
    const sql = qualityLineageSeedSql(args[0], args[1]);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
  });

  test.each(steps)('recursive member %s is byte-identical to the pre-dedupe rendering', (args, before, digest) => {
    const sql = qualityLineageStepSql(args[0], args[1]);
    expect(sql).toBe(before);
    expect(sha(sql)).toBe(digest);
  });

  test('one owner-local head helper replaces the two inline head prefixes', () => {
    const helper = read('adapters/quality-lineage-recursion-sql.ts');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(helper).toContain('const lineageHead = (columns: string, tail: string) => `SELECT ${columns},${tail}`');
    expect(helper).toContain('lineageHead(columns, `0 depth FROM ${source}`)');
    expect(helper).toContain('lineageHead(columns, `${alias}.depth+1 FROM lineage ${alias}`)');
    expect(helper).not.toContain('=> `SELECT ${columns},0 depth FROM ${source}`');
    expect(helper).not.toContain('=> `SELECT ${columns},${alias}.depth+1 FROM lineage ${alias}`');
  });

  test('binds and guard order are unchanged by the head fold', () => {
    const rendered = [
      ...seeds.map(([[columns, source], before]) => [qualityLineageSeedSql(columns, source), before] as const),
      ...steps.map(([[columns, alias], before]) => [qualityLineageStepSql(columns, alias), before] as const),
    ];
    for (const [sql, before] of rendered) {
      expect(sql).toBe(before);
      expect(sql.match(/\?|\$[0-9]/g)).toBeNull();
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    const helper = read('adapters/quality-lineage-recursion-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('call sites keep the same seed/step/terminal composition', () => {
    const consumers = ['adapters/purchase-sources.ts', 'adapters/sales-package-sources.ts', 'adapters/ecommerce-package-sources.ts'].map(read).join('\n');
    expect(consumers.match(/qualityLineageSeedSql\(/g)).toHaveLength(4);
    expect(consumers.match(/qualityLineageStepSql\(/g)).toHaveLength(4);
    expect(consumers.match(/qualityLineageTerminalSql\(/g)).toHaveLength(4);
    for (const [, before] of [...seeds, ...steps]) expect(consumers).not.toContain(before);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds');
    // The terminal head is a different shape and stays independent.
    expect(read('adapters/quality-lineage-terminal-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });

  test('retirement:ts-statements is strictly lower than the BEFORE=30 baseline', () => {
    expect(kpi()).toBeLessThan(30);
    expect(kpi()).toBe(25);
  });
});
