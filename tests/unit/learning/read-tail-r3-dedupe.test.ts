import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  learningNativeSources,
  nativeSourceProbe,
  overdueEvidenceColumns,
  learningModuleGrantProbe,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied verbatim from HEAD; used only as byte-identity oracles.
const preNativeSources = `SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'learn.courses' capability FROM learning_courses
 UNION ALL SELECT e.id,c.company_id,NULL::text,NULL::text,'learn.courses' FROM learning_course_events e JOIN learning_courses c ON c.id=e.course_id
 UNION ALL SELECT id,company_id,branch_id,department_id,'learn.assignments' FROM learning_assignments
 UNION ALL SELECT e.id,a.company_id,a.branch_id,a.department_id,CASE WHEN e.action IN ('complete','waive') THEN 'learn.results' ELSE 'learn.assignments' END FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id
 UNION ALL SELECT c.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificates c JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT r.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificate_revocations r JOIN learning_certificates c ON c.id=r.certificate_id AND c.company_id=r.company_id JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT h.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_eligibility_handoffs h JOIN learning_assignments a ON a.id=h.assignment_id AND a.company_id=h.company_id`;
const preOverdueColumns = 'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for';
const preGrantProbe = (alias: string) => `SELECT 1 FROM erpx_json_each(${alias}.modules_json) module WHERE module.value='learning'`;

describe('Learning read-tail r3 byte-identical / parameterized family dedupe', () => {
  test('native source union is moved verbatim to one owner source', () => {
    expect(learningNativeSources).toBe(preNativeSources);
    readOnly(learningNativeSources);
  });

  test('native source probe renders byte-identical SQL for every alias', () => {
    for (const alias of ['learning_source', 'actual_learning', 'native']) {
      expect(nativeSourceProbe(alias)).toBe(`SELECT 1 FROM (${preNativeSources}) ${alias}`);
      readOnly(nativeSourceProbe(alias));
    }
  });

  test('the three native probe call sites are composed, never inlined', () => {
    const text = src('record-purchases.ts');
    expect(text).not.toContain('SELECT 1 FROM (${nativeSources})');
    expect(text.match(/nativeSourceProbe\(/g) ?? []).toHaveLength(3);
    // r7 composed the actor-scope probe, r16 the scope row and r17 the last record probe.
    expect(text.match(/SELECT /g) ?? []).toHaveLength(0);
  });

  test('overdue evidence head is byte-identical at its two call sites', () => {
    expect(overdueEvidenceColumns).toBe(preOverdueColumns);
    readOnly(overdueEvidenceColumns);
    const search = src('executive-evidence-sources.ts');
    expect(search).not.toContain(preOverdueColumns);
    // Wave 2 moved the overdue search read to the native axum port as well.
    expect(search).not.toContain('${overdueEvidenceColumns}');
    expect(search).toContain('nativeLearningEvidenceSearch(');
    const detail = src('learning.ts');
    expect(detail).toContain('nativeLearningEvidenceRow(');
    expect(detail).not.toContain('${overdueEvidenceColumns}');
  });

  test('module-grant probe renders byte-identical SQL for both aliases', () => {
    for (const alias of ['learning_module', 'learning_module_grant']) {
      expect(learningModuleGrantProbe(alias)).toBe(preGrantProbe(alias));
      readOnly(learningModuleGrantProbe(alias));
    }
  });

  test('module-grant probe is defined once and used by both callers', () => {
    for (const name of ['purchases.ts', 'evaluation-source-policy.ts']) {
      const text = src(name);
      expect(text).not.toContain("SELECT 1 FROM erpx_json_each(");
      // r7 composes it through moduleGrantAssignmentProbe; no inline grant survives.
      expect(text).toContain('moduleGrantAssignmentProbe(');
    }
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const overdueEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const nativeSourceProbe =/g) ?? []).toHaveLength(1);
  });
});

describe('Learning read-tail r3 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r3 BEFORE=86 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(86);
  });
});
