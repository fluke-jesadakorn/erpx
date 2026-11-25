import { describe, expect, test, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PEOPLE_PAYROLL_RD_ACTIVE_PAYEE,
  PEOPLE_PAYROLL_RD_COMMAND_RECEIPT_PRIOR,
  PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_ID,
  PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_SHIFT,
} from '../../../packages/server-people_payroll/src/adapters/native-people-payroll-read';
import {
  activePayee,
  commandReceiptPrior,
  rosterAssignmentById,
  rosterAssignmentsByShift,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r53-native';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/${name}`, import.meta.url), 'utf8');
const rust = () => src('adapters/native_read_sql.rs');

// Verbatim retired literals from pp-read-tail-r20-sources.ts:7,8 and
// pp-read-tail-r15-sources.ts:6,7 at BEFORE=1221. Byte-identity oracles only.
const byId = 'SELECT * FROM people_roster_assignments WHERE id=?';
const byShift = "SELECT * FROM people_roster_assignments WHERE shift_id=? AND state='assigned'";
const prior = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const payee = "SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL";

type Case = {
  name: string;
  kind: string;
  sql: string;
  binds: number;
  values: unknown[];
  rs: string;
  run: (db: never) => Promise<unknown>;
};
const cases: Case[] = [
  {
    name: 'roster-by-id', kind: PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_ID, sql: byId, binds: 1,
    values: ['assignment-1'], rs: 'ROSTER_ASSIGNMENT_BY_ID', run: db => rosterAssignmentById(db, 'assignment-1'),
  },
  {
    name: 'roster-by-shift', kind: PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_SHIFT, sql: byShift, binds: 1,
    values: ['shift-1'], rs: 'ROSTER_ASSIGNMENT_BY_SHIFT', run: db => rosterAssignmentsByShift(db, 'shift-1'),
  },
  {
    name: 'receipt-prior', kind: PEOPLE_PAYROLL_RD_COMMAND_RECEIPT_PRIOR, sql: prior, binds: 4,
    values: ['key-1', 'people.directory', 'user-1', 'company-1'], rs: 'COMMAND_RECEIPT_PRIOR',
    run: db => commandReceiptPrior(db, 'key-1', 'people.directory', 'user-1', 'company-1'),
  },
  {
    name: 'active-payee', kind: PEOPLE_PAYROLL_RD_ACTIVE_PAYEE, sql: payee, binds: 2,
    values: ['person-1', 'company-1'], rs: 'ACTIVE_PAYEE', run: db => activePayee(db, 'person-1', 'company-1'),
  },
];

const envKey = 'ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993', timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
        async first() { return null; },
      };
    },
    async batch() { return [{ results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared };
};

describe('People Payroll read-tail r53 native read tail', () => {
  test('bridge constants are allow-listed discriminators, not SQL', () => {
    expect(PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_ID).toBe('people_payroll_rd_roster_assignment_by_id');
    expect(PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_SHIFT).toBe('people_payroll_rd_roster_assignment_by_shift');
    expect(PEOPLE_PAYROLL_RD_COMMAND_RECEIPT_PRIOR).toBe('people_payroll_rd_command_receipt_prior');
    expect(PEOPLE_PAYROLL_RD_ACTIVE_PAYEE).toBe('people_payroll_rd_active_payee');
    for (const c of cases) expect(c.kind, c.name).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR\s+UPDATE|FOR\s+SHARE/);
    expect(src('adapters/native-people-payroll-read.ts').match(/export const PEOPLE_PAYROLL_RD_/g) ?? []).toHaveLength(21);
  });

  test('Rust registry plans the retired literals byte-for-byte with exact bind arity', () => {
    const registry = rust();
    for (const c of cases) {
      expect(c.sql.match(/\?/g) ?? [], c.name).toHaveLength(c.binds);
      expect(registry, c.name + ' kind').toContain(`"${c.kind}"`);
      expect(registry, c.name + ' literal').toContain(`const SQL_${c.rs}: &str = ${JSON.stringify(c.sql)};`);
      const block = registry.slice(registry.indexOf(`const READ_${c.rs}: NativeReadStatement`), registry.indexOf(`const READ_${c.rs}: NativeReadStatement`) + 320);
      expect(block, c.name + ' bind count').toContain(`bind_count: ${c.binds},`);
      expect(registry, c.name + ' registry entry').toContain(`READ_${c.rs},`);
    }
    expect(registry.match(/^const READ_[A-Z_]+: NativeReadStatement =/gm) ?? []).toHaveLength(21);
    expect(registry).toContain('assert_eq!(READS.len(), 21);');
  });

  test('each wrapper sends its exact kind/binds and runs the planned statement on the caller unit', async () => {
    for (const c of cases) {
      const bodies: { kind: string; values: unknown[] }[] = [];
      vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
        bodies.push({ kind: body.kind, values: body.values });
        return new Response(JSON.stringify({
          data: { statements: [{ sql: c.sql, values: body.values }] },
        }), { status: 200, headers: { 'content-type': 'application/json' } });
      }));
      const { db, bound, prepared } = fakeDb();
      await c.run(db as never);

      expect(bodies, c.name).toEqual([{ kind: c.kind, values: c.values }]);
      expect(bodies[0]?.values, c.name + ' bind arity').toHaveLength(c.binds);
      expect(bound, c.name + ' caller unit binds').toEqual([c.values]);
      expect(prepared, c.name + ' planned literal').toEqual([c.sql]);
      const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
      expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/people-payroll/command-plan');
      expect((init.headers as Record<string, string>).origin).toBe('https://acme.example.com');
      vi.unstubAllGlobals();
    }
  });

  test('former inline callers route through the native wrappers and keep their locks', () => {
    const receiptCallers = [
      'payroll-other-remittance-unit.ts', 'payroll-purchases.ts', 'performance-purchases.ts',
      'people-purchases.ts', 'workforce-purchases.ts', 'roster-purchases.ts',
    ];
    for (const name of receiptCallers) {
      expect(src(`adapters/${name}`), name).toContain("from './pp-read-tail-r53-native'");
      expect((src(`adapters/${name}`).match(/commandReceiptPrior\(/g) ?? []), name).toHaveLength(1);
      expect(src(`adapters/${name}`), name).not.toContain('PP_RD15_COMMAND_RECEIPT_PRIOR_SQL');
    }
    for (const name of ['payroll-adjustments.ts', 'payroll-readiness.ts', 'people-payroll.ts']) {
      expect(src(`adapters/${name}`), name).toMatch(/activePayee(?:<[^()]*>)?\(/);
      expect(src(`adapters/${name}`), name).not.toContain('PP_RD15_ACTIVE_PAYEE_SQL');
    }
    const roster = src('adapters/people-roster.ts');
    expect(roster.match(/rosterAssignmentById<RosterAssignment>\(/g) ?? []).toHaveLength(4);
    expect(roster.match(/rosterAssignmentsByShift<RosterAssignment>\(/g) ?? []).toHaveLength(2);
    expect(roster).not.toContain('PP_RD20_ROSTER_ASSIGN_BY_ID_SQL');
    expect(roster).not.toContain('PP_RD20_ROSTER_ASSIGN_BY_SHIFT_SQL');
    expect(roster.match(/SELECT \* FROM people_roster_assignments WHERE shift_id=\? AND state='assigned' FOR UPDATE/g) ?? []).toHaveLength(3);
    expect(src('adapters/payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('adapters/payroll-adjustments.ts')).toContain('FOR SHARE');
    expect(src('adapters/pp-read-tail-r15-sources.ts')).not.toContain('command_receipts');
    expect(src('adapters/pp-read-tail-r20-sources.ts')).not.toContain('people_roster_assignments');
  });

  test('retirement:ts-statements is strictly below BEFORE=1221 and pinned at 1214', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1221);
    expect(total).toBe(1201);
  });
});
