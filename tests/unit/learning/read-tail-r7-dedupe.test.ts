import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  actorScopeExistsProbe,
  assignmentsImage,
  moduleGrantAssignmentProbe,
  sourceLiteral,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r7 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const assignmentRead = (alias: string) => `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'records.read')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
const preGrantEvaluation = `EXISTS(SELECT 1 FROM assignments learning_module_grant WHERE learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module_grant.modules_json) module WHERE module.value='learning'))`;
const preGrantPurchases = `EXISTS(SELECT 1 FROM assignments learning_module WHERE learning_module.id=? AND learning_module.company_id=learning_scope.company_id
 AND learning_module.user_id=? AND learning_module.role=? AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module.modules_json) module WHERE module.value='learning'))`;
const preScopeEvaluation = `EXISTS(SELECT 1 FROM(SELECT a.company_id,a.branch_id,a.department_id)learning_actor_scope WHERE ${assignmentRead('learning_actor_scope')})`;
const preScopeRecords = `EXISTS(SELECT 1 FROM (SELECT learning_source.company_id,?::text branch_id,?::text department_id) learning_actor_scope WHERE ${assignmentRead('learning_actor_scope')})`;

describe('Learning read-tail r7 byte-identical families', () => {
  test('assignment module-grant probe renders both original literals byte-for-byte', () => {
    expect(moduleGrantAssignmentProbe('learning_module_grant', 'learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id')).toBe(preGrantEvaluation);
    expect(moduleGrantAssignmentProbe('learning_module', `learning_module.id=? AND learning_module.company_id=learning_scope.company_id
 AND learning_module.user_id=? AND learning_module.role=?`)).toBe(preGrantPurchases);
    readOnly(moduleGrantAssignmentProbe('g', 'g.id=?'));
  });

  test('actor-scope probe renders both original literals byte-for-byte', () => {
    expect(actorScopeExistsProbe('a.company_id,a.branch_id,a.department_id')).toBe(preScopeEvaluation);
    expect(actorScopeExistsProbe('learning_source.company_id,?::text branch_id,?::text department_id', ' ')).toBe(preScopeRecords);
    readOnly(actorScopeExistsProbe('a.company_id,a.branch_id,a.department_id'));
  });

  test('assignments image subquery renders both original projections byte-for-byte', () => {
    expect(assignmentsImage('to_jsonb(a)')).toBe('(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM learning_assignments a WHERE a.company_id=?)');
    expect(assignmentsImage(`to_jsonb(a)||jsonb_build_object('visible',TRUE)`)).toBe(`(SELECT jsonb_agg(to_jsonb(a)||jsonb_build_object('visible',TRUE) ORDER BY a.id) FROM learning_assignments a WHERE a.company_id=?)`);
    readOnly(assignmentsImage('to_jsonb(a)'));
  });

  test('SELECT * source literal renders every original table source byte-for-byte', () => {
    for (const table of ['learning_assignments', 'learning_eligibility_handoffs', 'learning_certificates']) {
      expect(sourceLiteral(table)).toBe(`SELECT * FROM ${table}`);
      readOnly(sourceLiteral(table));
    }
  });

  test('each family is defined once and composed at every caller', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    for (const [name, helper, count] of [
      ['evaluation-source-policy.ts', 'moduleGrantAssignmentProbe', 1],
      ['purchases.ts', 'moduleGrantAssignmentProbe', 1],
      ['record-purchases.ts', 'actorScopeExistsProbe', 1],
      ['evaluation-source-policy.ts', 'actorScopeExistsProbe', 1],
      ['purchases.ts', 'assignmentsImage', 1],
      ['people-evaluation-facts.ts', 'assignmentsImage', 1],
      ['metric-purchases.ts', 'sourceLiteral', 1],
      ['package-obligations.ts', 'sourceLiteral', 2],
    ] as const) {
      const text = src(name);
      expect(text.match(new RegExp(`${helper}\\(`, 'g')) ?? []).toHaveLength(count);
    }
    expect(src('purchases.ts')).not.toContain("SELECT 1 FROM assignments learning_module WHERE");
    expect(src('evaluation-source-policy.ts')).not.toContain('SELECT 1 FROM assignments learning_module_grant WHERE');
    expect(src('record-purchases.ts')).not.toContain('learning_actor_scope WHERE ${assignmentReadSql');
    expect(src('metric-purchases.ts')).not.toContain("'SELECT * FROM learning_assignments'");
  });

  test('r1-r6 helpers and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'overdueEvidenceColumns', 'learningModuleGrantProbe']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    let locks = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) locks += (readFileSync(path, 'utf8').match(/LOCK TABLE/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(locks).toBe(7);
  });
});

describe('Learning read-tail r7 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=77 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(77);
    expect(total).toBeLessThanOrEqual(72);
  });
});
