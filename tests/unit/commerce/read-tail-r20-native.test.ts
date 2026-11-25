import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const norm = (value: string) => value.replace(/\s+/g, '');

const plans = read('packages/server-commerce/src/adapters/native_commerce_read_sql.rs');
const registry = read('packages/server-commerce/src/public/native_read.rs');
const client = read('packages/server-commerce/src/adapters/native-commerce-read.ts');
const harness = read('tests/unit/commerce/rust-commerce-order-http.test.rs');
const adapters = (file: string) => `packages/server-commerce/src/adapters/${file}`;

/** G4 / retire.commerce.native-read-r20: five warehouse-config reads move from
 * the registry-only public port to the mounted `/commerce/command-plan`
 * resolver. Each keeps its exact statement and every positional bind; the two
 * `(?::text IS NULL OR branch_id=?)` positions stay separate and carry the
 * caller's own branch values. `warehouse.identity` and `pos-facts.ts` remain
 * excluded. */
const kinds = [
  {
    kind: 'commerce_rd_warehouse_page_rows',
    registryKind: 'warehouse.page-rows',
    constName: 'RD_WAREHOUSE_PAGE_ROWS',
    planConst: 'PLAN_WAREHOUSE_PAGE_ROWS',
    SQL: 'SELECT *,name AS page_key FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?)',
    binds: 3,
    branchPositions: 2,
    authority: 'commerce-pages.ts',
    callers: ['commerce-pages.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_warehouse_master_rows',
    registryKind: 'warehouse.master-rows',
    constName: 'RD_WAREHOUSE_MASTER_ROWS',
    planConst: 'PLAN_WAREHOUSE_MASTER_ROWS',
    SQL: 'SELECT * FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name',
    binds: 3,
    branchPositions: 2,
    authority: 'commerce.ts',
    callers: ['commerce.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_warehouse_active',
    registryKind: 'warehouse.active',
    constName: 'RD_WAREHOUSE_ACTIVE',
    planConst: 'PLAN_WAREHOUSE_ACTIVE',
    SQL: "SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)",
    binds: 4,
    branchPositions: 2,
    authority: 'guarded-source-dedupe-sql.ts',
    callers: [
      'commerce.ts',
      'ecommerce-sources.ts',
      'initial-physical-stock-count-sources.ts',
      'inventory-counts-sources.ts',
      'production-command-sources.ts',
    ],
    mode: 'First',
  },
  {
    kind: 'commerce_rd_warehouses_active',
    registryKind: 'warehouses.active',
    constName: 'RD_WAREHOUSES_ACTIVE',
    planConst: 'PLAN_WAREHOUSES_ACTIVE',
    SQL: "SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name",
    binds: 3,
    branchPositions: 2,
    authority: 'guarded-source-dedupe-sql.ts',
    callers: ['ecommerce-sources.ts', 'inventory-counts-sources.ts', 'production-command-sources.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_warehouse_branch_active',
    registryKind: 'warehouse.branch-active',
    constName: 'RD_WAREHOUSE_BRANCH_ACTIVE',
    planConst: 'PLAN_WAREHOUSE_BRANCH_ACTIVE',
    SQL: "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",
    binds: 2,
    branchPositions: 0,
    authority: 'guarded-source-dedupe-sql.ts',
    callers: ['pms-command-facts.ts', 'pms-hospitality-setup-facts.ts'],
    mode: 'All',
  },
] as const;

describe('commerce read-tail r20 native warehouse kinds', () => {
  it('registers the five warehouse kinds with exact statements and bind counts', () => {
    for (const { kind, SQL, binds } of kinds) {
      expect(plans, kind).toContain(`"${kind}"`);
      expect(plans, kind).toContain(SQL);
      expect(plans, kind).toContain(`bind_count: ${binds},`);
      expect(SQL.match(/\?/g)?.length).toBe(binds);
      expect(SQL).toMatch(/^SELECT /);
      expect(SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
      expect(SQL).toContain('commerce_warehouses');
    }
    // 9 prior kinds (PMS/sales editor/POS startup/settings) + 6 product kinds
    // from r19 + these 5 warehouse kinds.
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 20);');
    for (const { constName } of kinds) {
      expect(plans, constName).toContain(`READ_${constName.slice(3)},`);
    }
  });

  it('pins the public registry plans and the Rust parity harness cases', () => {
    for (const file of [
      'commerce-pages.ts',
      'commerce.ts',
      'guarded-source-dedupe-sql.ts',
      'ecommerce-sources.ts',
    ]) {
      expect(registry, file).toContain(`include_str!("../adapters/${file}")`);
    }
    for (const { kind, registryKind, constName, planConst, SQL } of kinds) {
      expect(registry, kind).toContain(`"${registryKind}"`);
      expect(registry, constName).toContain(`pub const ${constName}`);
      expect(registry, planConst).toContain(`pub const ${planConst}`);
      expect(registry, `${kind} registry SQL`).toContain(SQL);
      expect(harness, kind).toContain(`"${kind}"`);
      expect(harness, `${kind} harness SQL`).toContain(SQL);
    }
  });

  it('keeps byte parity between the native plans and the retained authority text', () => {
    for (const file of new Set(kinds.map(({ authority }) => authority))) {
      const text = read(adapters(file));
      expect(text, `${file} keeps normalized authority text`).toContain('SELECT\t');
      for (const { SQL, kind, authority } of kinds.filter(kind => kind.authority === file)) {
        expect(norm(text), `${kind} in ${authority}`).toContain(norm(SQL));
        expect(text, `${kind} executable token in ${authority}`).not.toContain(`'${SQL}'`);
        expect(text, `${kind} executable token in ${authority}`).not.toContain(`"${SQL}"`);
        expect(text, `${kind} tabbed authority in ${authority}`).toContain(`SELECT\t${SQL.slice('SELECT '.length)}`);
      }
    }
  });

  it('passes both branch positions through with caller-supplied distinct values', () => {
    // Six migrated site signatures keep `branchId` and `branchId2` independent.
    const distinct = [
      ['commerce.ts', 'facts.branchId, facts.branchId2', 'RD_WAREHOUSE_MASTER_ROWS'],
      ['commerce.ts', 'facts.branchId, facts.branchId2', 'RD_WAREHOUSE_ACTIVE'],
      ['inventory-counts-sources.ts', 'branchId, branchId2', 'RD_WAREHOUSES_ACTIVE'],
      ['inventory-counts-sources.ts', 'branchId, branchId2', 'RD_WAREHOUSE_ACTIVE'],
      ['production-command-sources.ts', 'branchId, branchId2', 'RD_WAREHOUSES_ACTIVE'],
      ['production-command-sources.ts', 'branchId, branchId2', 'RD_WAREHOUSE_ACTIVE'],
    ] as const;
    expect(distinct).toHaveLength(6);
    for (const [file, signature, constName] of distinct) {
      const text = read(adapters(file));
      expect(text, `${constName} in ${file}`).toContain(constName);
      expect(text, `${signature} in ${file}`).toContain(signature);
    }
    // The harness sends different values to the two branch bind positions and
    // asserts the command-plan reply echoes every positional value verbatim.
    expect(harness).toContain('["company-1","branch-1","branch-2"]');
    expect(harness).toContain('["warehouse-1","company-1","branch-1","branch-2"]');
    expect(harness).toContain('"{kind} values pass through verbatim"');
    expect(plans).toContain('warehouse_reads_keep_two_distinct_branch_positions');
    expect(registry).toContain('warehouse_plans_keep_both_branch_bind_positions');
    expect(kinds.filter(kind => kind.branchPositions === 2)).toHaveLength(4);
  });

  it('exposes the kinds as TypeScript bridge consts and delegates callers with first/all parity', () => {
    for (const { kind, constName, callers, mode } of kinds) {
      expect(client, kind).toContain(`'${kind}'`);
      expect(client, constName).toContain(`export const ${constName}`);
      for (const file of callers) {
        const text = read(adapters(file));
        expect(text, file).toContain(`nativeCommerceRead${mode}`);
        expect(text, file).toContain(constName);
      }
    }
    const bridge = client.match(/export const RD_WAREHOUSE[A-Z_]*/g) ?? [];
    expect(bridge).toHaveLength(kinds.length);
  });

  it('leaves warehouse.identity and pos-facts.ts excluded', () => {
    const guards = read(adapters('guarded-source-dedupe-sql.ts'));
    expect(registry).toContain('"warehouse.identity"');
    expect(guards).toContain('COMMERCE_WAREHOUSE_IDENTITY_SQL');
    expect(guards).toContain("'SELECT id,company_id,branch_id,status FROM commerce_warehouses'");
    expect(plans).not.toContain('commerce_rd_warehouse_identity');
    const pos = read(adapters('pos-facts.ts'));
    expect(pos).toContain('SELECT * FROM commerce_warehouses WHERE company_id=? AND status=\'active\' AND (?::text IS NULL OR branch_id=?) ORDER BY name');
    expect(plans).not.toContain('commerce_rd_pos_warehouse');
  });

  it('pins the owner TypeScript SELECT KPI at 406 (before 413)', () => {
    const src = resolve(root, 'packages/server-commerce/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(406);
  });
});
