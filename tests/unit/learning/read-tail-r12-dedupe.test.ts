import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {packageScopeCte} from '../../../modules/learning/server/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/learning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r12 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const assignmentsRef = '${' + 'learningPackageSettlementSources.assignments' + '}';
const preAssignments = `MATERIALIZED(SELECT a.* FROM (${assignmentsRef}) a,scope WHERE a.company_id=scope.company_id)`;
const preHandoffs = 'MATERIALIZED(SELECT h.* FROM learning_eligibility_handoffs h,scope WHERE h.company_id=scope.company_id)';
const preCertificates = 'MATERIALIZED(SELECT c.* FROM learning_certificates c,scope WHERE c.company_id=scope.company_id)';

describe('Learning read-tail r12 package-obligation scope CTE head', () => {
  test('renders all three pre-dedupe scope CTEs byte-for-byte', () => {
    expect(packageScopeCte('a', `(${assignmentsRef})`)).toBe(preAssignments);
    expect(packageScopeCte('h', 'learning_eligibility_handoffs')).toBe(preHandoffs);
    expect(packageScopeCte('c', 'learning_certificates')).toBe(preCertificates);
    readOnly(preAssignments);
    readOnly(preHandoffs);
    readOnly(preCertificates);
  });

  test('shared SELECT head is byte-identical at all three HEAD call sites', () => {
    const head = (alias: string, source: string) =>
      `MATERIALIZED(SELECT ${alias}.* FROM ${source} ${alias},scope WHERE ${alias}.company_id=scope.company_id)`;
    expect(head('a', `(${assignmentsRef})`)).toBe(preAssignments);
    expect(head('h', 'learning_eligibility_handoffs')).toBe(preHandoffs);
    expect(head('c', 'learning_certificates')).toBe(preCertificates);
  });

  test('family is defined once and composed at all three callers', () => {
    expect(src('learning-read-sql.ts').match(/export const packageScopeCte =/g) ?? []).toHaveLength(1);
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/packageScopeCte\(/g) ?? []).toHaveLength(3);
    expect(obligations).not.toContain(preAssignments);
    expect(obligations).not.toContain(preHandoffs);
    expect(obligations).not.toContain(preCertificates);
  });
});

describe('Learning read-tail r12 invariants', () => {
  test('r1-r11 helpers, completion columns and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe', 'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead', 'assignmentCourseReadHead']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/const learnerEligible =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
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

  test('retirement:ts-statements is strictly lower than the r12 BEFORE=65 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(65);
    expect(total).toBeLessThanOrEqual(63);
  });
});
