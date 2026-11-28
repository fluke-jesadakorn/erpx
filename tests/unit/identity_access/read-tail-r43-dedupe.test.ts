import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, test } from 'vitest';
import {
  PC_DIAGNOSIS_ASSIGNMENTS,
  PC_DIAGNOSIS_MEMBER_PROBE,
  PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS,
  planIdentityControlRead,
  setIdentityControlReadForTest,
} from '../../../modules/identity-access/server/src/adapters/native-control-read-plan';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const placeholders = (s: string) => (s.match(/\?/g) ?? []).length;

// Verbatim pre-cut access-diagnosis statements. These SHA-256 values are
// byte-identity oracles only, never live query sources.
const ORACLES = [
  {
    name: 'diagnosis platform assignments',
    kind: PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS,
    sql: 'SELECT role,valid_from,valid_until,revoked_at FROM platform_assignments WHERE user_id=?',
    bytes: 87,
    binds: 1,
    sha: 'b7c983288f3c6cf06570336190a5b8a2d51623bcbfcaa1889eabae82aa124306',
  },
  {
    name: 'diagnosis member probe',
    kind: PC_DIAGNOSIS_MEMBER_PROBE,
    sql: 'SELECT 1 FROM assignments WHERE user_id=? LIMIT 1',
    bytes: 49,
    binds: 1,
    sha: 'de943279962f366dbf7a96ff64802a8adfcf31237d3e629acbb0daca3b22b813',
  },
  {
    name: 'diagnosis assignment list',
    kind: PC_DIAGNOSIS_ASSIGNMENTS,
    sql: 'SELECT role,company_id,branch_id,scope_kind,scope_node_id,valid_from,valid_until,revoked_at FROM assignments WHERE user_id=?',
    bytes: 124,
    binds: 1,
    sha: '9725070043e9c2c53adf3b00a903855ecb8ae3285087503d28b3fd9ff5e647b0',
  },
] as const;

describe('Identity access read-tail r43 byte oracles', () => {
  test('the three retired access-diagnosis statements carry their recorded SHA-256', () => {
    for (const o of ORACLES) {
      expect(Buffer.byteLength(o.sql), o.name).toBe(o.bytes);
      expect(placeholders(o.sql), o.name).toBe(o.binds);
      expect(sha256(o.sql), o.name).toBe(o.sha);
      expect(o.sql.startsWith('SELECT '), o.name).toBe(true);
      expect(o.sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the Rust registry pins the same bytes and hashes for all three kinds', () => {
    const port = read('adapters/control_read_plan.rs');
    for (const o of ORACLES) {
      expect(port, o.name).toContain(`pub const ${o.kind.toUpperCase()}: &str`);
      expect(port, o.name).toContain(o.sql.replace(/"/g, '\\"'));
      expect(port, o.name).toContain(o.sha);
      // Each kind resolves to its statement and one bind in the closed plan.
      expect(port, o.name).toContain(`SQL_DIAGNOSIS`);
    }
    expect(port).toContain('diagnosis_plans_pin_three_static_single_bind_reads');
    expect(port).toContain('assert_eq!(planned.sql.matches(\'?\').count(), 1');
  });

  test('the TypeScript client mirrors every kind, arity and statement hash', () => {
    const client = read('adapters/native-control-read-plan.ts');
    for (const o of ORACLES) {
      expect(client, o.name).toContain(o.kind);
      expect(client, o.name).toContain(o.sha);
      expect(client, o.name).toContain(`]: ${o.binds},`);
    }
  });
});

describe('Identity access read-tail r43 plan wiring', () => {
  afterEach(() => setIdentityControlReadForTest(undefined));

  test('access-diagnosis no longer inlines the three statements and plans each read', () => {
    const source = read('adapters/access-diagnosis.ts');
    for (const o of ORACLES) expect(source, o.name).not.toContain(o.sql);
    expect((source.match(/planIdentityControlRead\(/g) ?? []).length).toBe(3);
    expect((source.match(/prepare\(plan\.sql\)\.bind\(\.\.\.plan\.values\)/g) ?? []).length).toBe(3);
    for (const symbol of ['PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS', 'PC_DIAGNOSIS_MEMBER_PROBE', 'PC_DIAGNOSIS_ASSIGNMENTS'])
      expect(source, symbol).toContain(symbol);
    // The cross-owner organization-node reads are recorded blockers and stay.
    expect(source).toContain('organizationSources.access_diagnosis_organization_nodes_n_from');
    expect(source).toContain('organizationSources.access_diagnosis_organization_nodes_from');
    expect(source).toContain('organizationSources.access_diagnosis_organization_nodes_b_from');
    expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('each kind plans only through a verified reply and fails closed on unexpected bytes', async () => {
    for (const o of ORACLES) {
      setIdentityControlReadForTest(async (_kind, values) => ({ sql: o.sql, values: [...values] }));
      const planned = await planIdentityControlRead(o.kind, ['user-1']);
      expect(planned.sql, o.name).toBe(o.sql);
      expect(planned.values, o.name).toEqual(['user-1']);
    }
    setIdentityControlReadForTest(async () => ({ sql: 'SELECT 1', values: ['user-1'] }));
    await expect(planIdentityControlRead(PC_DIAGNOSIS_MEMBER_PROBE, ['user-1'])).rejects.toMatchObject({
      code: 'IDENTITY_CONTROL_READ_RESPONSE_INVALID',
    });
    await expect(planIdentityControlRead(PC_DIAGNOSIS_MEMBER_PROBE, [])).rejects.toMatchObject({
      code: 'IDENTITY_CONTROL_READ_INVALID',
    });
  });

  test('the native owner registry stays read-only and the prior plan kinds are untouched', () => {
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
    const port = read('adapters/control_read_plan.rs');
    for (const sql of ORACLES.map((o) => o.sql))
      expect(port, sql).toContain(sql.replace(/"/g, '\\"'));
    for (const kind of [
      'PC_REVOKED_SESSION',
      'PC_WORKFLOW_ACTIVE_CONTEXT',
      'PC_QUEUED_ASSIGNMENT_VERSION',
      'PC_PREF_PARENT_BINDING_SHARE',
    ])
      expect(port, kind).toContain(kind);
  });
});

describe('Identity access read-tail r43 retirement check', () => {
  test('retirement:ts-statements is 368 after access-diagnosis plan cut', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(369);
    expect(total).toBe(368);
  });
});
