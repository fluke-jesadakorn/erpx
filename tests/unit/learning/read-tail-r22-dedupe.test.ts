import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  existsRowProbe,
  learningModuleGrantProbe,
  moduleGrantAssignmentProbe,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Pre-r22 rendered bytes, captured verbatim at HEAD 0087c6e2 (learning-read-sql.ts
// blob 6124229c7d092d774b6742b93cafb2f043d95449). Byte-identity oracles only;
// never live query sources. The probe carries no bind and keeps none after the fold.
const pre = [
  {
    name: 'purchases',
    alias: 'learning_module',
    sql: "SELECT 1 FROM erpx_json_each(learning_module.modules_json) module WHERE module.value='learning'",
    sha: 'bed65cc634965b261f8d2faf79620d7bcfae769bb652c36b703df1e97ea5cbbe',
    bytes: 95,
    binds: 0,
  },
  {
    name: 'evaluation-source-policy',
    alias: 'learning_module_grant',
    sql: "SELECT 1 FROM erpx_json_each(learning_module_grant.modules_json) module WHERE module.value='learning'",
    sha: 'f23eb4cd9c6e42a57234b2d1b4b481ec4370ae9806728148a4e5fa813e389e8f',
    bytes: 101,
    binds: 0,
  },
  {
    name: 'r3-regression',
    alias: 'a',
    sql: "SELECT 1 FROM erpx_json_each(a.modules_json) module WHERE module.value='learning'",
    sha: '90fc739ece488908a9488e697701b6cb826992f7af9d07caec7a195f38bbc27b',
    bytes: 81,
    binds: 0,
  },
];

// The two live module-grant callers keep their conditions and their binds; only
// the grant head is owned by the renderer. Rendered caller bytes are unchanged.
const preCallers = [
  {
    name: 'purchases',
    alias: 'learning_module',
    conditions: 'learning_module.id=? AND learning_module.company_id=learning_scope.company_id\n AND learning_module.user_id=? AND learning_module.role=?',
    sql: "EXISTS(SELECT 1 FROM assignments learning_module WHERE learning_module.id=? AND learning_module.company_id=learning_scope.company_id\n AND learning_module.user_id=? AND learning_module.role=? AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module.modules_json) module WHERE module.value='learning'))",
    sha: '68e68517be87ac501bc00370d9101544e647f63f05c1f86cb1507b1770da8033',
    bytes: 299,
    binds: 3,
  },
  {
    name: 'evaluation-source-policy',
    alias: 'learning_module_grant',
    conditions: 'learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id',
    sql: "EXISTS(SELECT 1 FROM assignments learning_module_grant WHERE learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module_grant.modules_json) module WHERE module.value='learning'))",
    sha: '0bb786c76055a3408b33c7e8474d5f481a115b208c6402cce092e54746fdcdb2',
    bytes: 321,
    binds: 3,
  },
];

// The single folded head still renders from the r17 renderer byte-for-byte.
const head = existsRowProbe('FROM erpx_json_each(a.modules_json) module', "module.value='learning'");
const headSql = "SELECT 1 FROM erpx_json_each(a.modules_json) module WHERE module.value='learning'";
const headSha = '90fc739ece488908a9488e697701b6cb826992f7af9d07caec7a195f38bbc27b';

describe('Learning read-tail r22 module-grant head fold', () => {
  test('every rendered grant probe is byte-identical to its pre-r22 oracle', () => {
    expect(pre).toHaveLength(3);
    for (const p of pre) {
      const rendered = learningModuleGrantProbe(p.alias);
      expect(rendered).toBe(p.sql);
      expect(Buffer.byteLength(rendered)).toBe(p.bytes);
      expect(sha(rendered)).toBe(p.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(p.binds);
      readOnly(rendered);
    }
  });

  test('both live callers keep their rendered bytes and their three binds', () => {
    expect(preCallers).toHaveLength(2);
    for (const c of preCallers) {
      const rendered = moduleGrantAssignmentProbe(c.alias, c.conditions);
      expect(rendered).toBe(c.sql);
      expect(Buffer.byteLength(rendered)).toBe(c.bytes);
      expect(sha(rendered)).toBe(c.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(c.binds);
      readOnly(rendered);
    }
  });

  test('the folded head is the r17 existsRowProbe head, byte-identically', () => {
    expect(head).toBe(headSql);
    expect(sha(head)).toBe(headSha);
    expect((head.match(/\?/g) ?? []).length).toBe(0);
    readOnly(head);
  });

  test('learningModuleGrantProbe delegates through existsRowProbe; head lives only there', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const existsRowProbe =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
    expect(shared).toContain('existsRowProbe(`FROM erpx_json_each(${alias}.modules_json) module`, "module.value=\'learning\'")');
    const decl = shared.match(/export const learningModuleGrantProbe =[\s\S]*?;\n/)?.[0];
    expect(decl).toBeDefined();
    expect(decl!).not.toContain('SELECT 1');
    expect(decl!).not.toContain('WHERE module.value');

    // Prove the live declaration actually routes through the injected renderer.
    const build = new Function(
      'existsRowProbe',
      `${decl!.replace('export ', '').replace('(alias: string)', '(alias)')}\nreturn learningModuleGrantProbe;`,
    ) as (probe: (source: string, where: string) => string) =>
      (alias: string) => string;
    const delegate = build((source, where) => `<${source}|${where}>`);
    expect(delegate('a')).toBe("<FROM erpx_json_each(a.modules_json) module|module.value='learning'>");
  });

  test('both callers, their binds and every lock/native source stay untouched', () => {
    const purchases = src('purchases.ts');
    const evaluation = src('evaluation-source-policy.ts');
    for (const name of ['purchases.ts', 'evaluation-source-policy.ts']) {
      const text = src(name);
      expect(text.match(/moduleGrantAssignmentProbe\(/g) ?? []).toHaveLength(1);
      expect(text).not.toContain('SELECT 1 FROM erpx_json_each(');
    }
    expect(purchases).toContain('moduleGrantAssignmentProbe(');
    expect(evaluation).toContain('moduleGrantAssignmentProbe(');

    const shared = src('learning-read-sql.ts');
    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);

    let locks = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          locks += (text.match(/LOCK TABLE/g) ?? []).length;
          expect(text).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/i);
        }
      }
    };
    walk(srcRoot);
    expect(locks).toBe(7);
  });
});

describe('Learning read-tail r22 invariants', () => {
  test('r1-r21 helpers stay defined exactly once', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'assignmentCourseJoinHead', 'assignmentCourseReadHead', 'packageScopeCte',
      'benefitConsumptionHead', 'latestEventLateral', 'workspaceImageFrame', 'clockedSourceCte',
      'obligationFactHead', 'learningCourseProjection', 'identitySourceRead', 'jsonbAggImageHead', 'scopeRow',
      'existsRowProbe', 'keyedRowRead', 'imageChildFrame', 'certificateConsistencyProbe',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
  });

  test('retirement:ts-statements KPI is 32 (strictly below the r24 BEFORE=33 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(33);
    expect(total).toBe(32);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(30);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(selectCount(src('learning-completion-evidence.ts'))).toBe(1);
    expect(selectCount(src('learning.ts'))).toBe(0);
  });
});
