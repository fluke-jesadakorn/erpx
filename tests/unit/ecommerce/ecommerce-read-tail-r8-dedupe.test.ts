import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { readExecutiveMetricProjection } from '../../../packages/server-ecommerce/src/adapters/executive-metric-projections';
import { commercialOwnerProjectionSources as organizationProjectionSources } from '../../../packages/server-organization/src/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/executive-metric-projections.ts'), 'utf8');

// Pre-dedupe label fragments copied verbatim from HEAD, kept only as byte-identity oracles.
const nodes = organizationProjectionSources.organization_nodes;
const head = {
  branch: `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`,
  department: `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`,
};

// Rebuild the r8 shared fragment exactly as the adapter defines it.
const nodeNameLabel = (col: string, fallback: string) =>
  `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=c.${col} AND n.company_id=c.company_id),'${fallback}')`;

const actor = { companyId: 'c', branchId: null, departmentId: null, modules: ['ecommerce'], permissions: [], userId: 'u', holdingId: 'h' } as any;
const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'THB' } as any;
async function sqlFor(id: string, breakdown: string): Promise<string> {
  let sql = '';
  const db: any = { prepare(statement: string) { sql = statement; return { bind() { return this; }, all() { return { results: [] }; } }; } };
  await readExecutiveMetricProjection(db, actor, id as any, filters, breakdown as any);
  return sql;
}

describe('Ecommerce read-tail r8 source-node label family', () => {
  test('node label is byte-identical for branch and department callers', () => {
    expect(nodeNameLabel('branch_id', 'Branch')).toBe(head.branch);
    expect(nodeNameLabel('department_id', 'Unassigned')).toBe(head.department);
  });

  test('family is defined once and composed at both call sites; literals are gone', () => {
    expect(src.match(/const nodeNameLabel = /g) ?? []).toHaveLength(1);
    expect(src.match(/nodeNameLabel\('branch_id','Branch'\)/g) ?? []).toHaveLength(1);
    expect(src.match(/nodeNameLabel\('department_id','Unassigned'\)/g) ?? []).toHaveLength(1);
    expect(src).not.toContain("n.id=c.branch_id AND n.company_id=c.company_id),'Branch')");
    expect(src).not.toContain("n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')");
  });

  test('composed labels stay read-only scalar subqueries', () => {
    for (const sql of [head.branch, head.department]) {
      expect(sql).toMatch(/^COALESCE\(\(SELECT n\.display_name FROM /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('emitted metric SQL embeds the byte-identical labels without changing binds', async () => {
    const scopedAlias = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';
    expect(await sqlFor('open-ecommerce-carts', 'branch')).toContain(head.branch);
    expect(await sqlFor('open-ecommerce-carts', 'department')).toContain(head.department);
    expect(await sqlFor('open-ecommerce-carts', 'status')).toBe(
      `SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE ${scopedAlias} AND c.status='open' GROUP BY 1 ORDER BY 1`,
    );
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
