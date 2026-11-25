import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/executive-metric-projections.ts'), 'utf8');
const rust = readFileSync(resolve(srcRoot, 'adapters/native_exec_metric/domain.rs'), 'utf8');
const nodes = 'SELECT \\"id\\",\\"kind\\",\\"slug\\",\\"display_name\\",\\"parent_id\\",\\"company_id\\",\\"branch_id\\",\\"status\\",\\"hierarchy_revision\\" FROM organization_nodes';

describe('Ecommerce read-tail r8 source-node label family, now native-owned', () => {
  test('Rust owns the byte-identical branch and department labels', () => {
    expect(rust).toContain(nodes);
    expect(rust).toContain('node_name_label("branch_id", "Branch")');
    expect(rust).toContain('node_name_label("department_id", "Unassigned")');
    expect(rust).toContain('COALESCE((SELECT n.display_name FROM ({ORG_NODES_SOURCE}) n WHERE n.id=c.{column} AND n.company_id=c.company_id),\'{fallback}\')');
    expect(rust).toContain('WHERE n.id=c.branch_id AND n.company_id=c.company_id),\'Branch\')');
    expect(rust).toContain('WHERE n.id=c.department_id AND n.company_id=c.company_id),\'Unassigned\')');
  });

  test('the TS adapter emits no label text or organisation projection SQL', () => {
    expect(src).not.toContain('nodeNameLabel');
    expect(src).not.toContain('organizationProjectionSources');
    expect(src).not.toContain('SELECT ');
    expect(src).toContain('nativeExecutiveMetric');
  });

  test('the native labels stay read-only scalar subqueries', () => {
    const statements = rust.slice(0, rust.indexOf("#[cfg(test)]"));
    expect(statements).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});

describe('Ecommerce read-tail r8 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r8 BEFORE=64 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(64);
  });
});
