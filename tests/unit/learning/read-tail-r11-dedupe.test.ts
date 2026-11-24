import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {assignmentCourseReadHead} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r11 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preAssignmentTail = 'FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?';
const preWitness = "SELECT a.course_id,a.course_version,c.company_id course_company,c.required_for,\n ${assignmentChildImage(\"jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id)\", `${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} WHERE p.company_id=a.company_id AND p.user_id=a.learner_id`, \"people\")},\n ${assignmentChildImage(\"jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id)\", `${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id`, \"engagements\")},\n ${assignmentChildImage(\"jsonb_agg(to_jsonb(h) ORDER BY h.id)\", \"FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id\", \"handoffs\")},\n ${assignmentChildImage(\"jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id)\", \"FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id\", \"certificates\")},\n CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible\n FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?";
const preRequiredFor = "SELECT c.required_for FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?";
const witnessProjection = preWitness.slice('SELECT '.length, preWitness.length - ('\n '.length + preAssignmentTail.length));

describe('Learning read-tail r11 assignment x course read head (no company join key)', () => {
  test('renders both pre-dedupe reads byte-for-byte', () => {
    expect(assignmentCourseReadHead(witnessProjection, '\n ')).toBe(preWitness);
    expect(assignmentCourseReadHead('c.required_for')).toBe(preRequiredFor);
    readOnly(preWitness);
    readOnly(preRequiredFor);
  });

  test('shared head is byte-identical at the two HEAD call sites', () => {
    expect(preWitness).toContain(`\n ${preAssignmentTail}`);
    expect(preRequiredFor).toBe(`SELECT c.required_for ${preAssignmentTail}`);
  });

  test('family is defined once and composed at both callers', () => {
    expect(src('learning-read-sql.ts').match(/export const assignmentCourseReadHead =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/assignmentCourseReadHead\(/g) ?? []).toHaveLength(2);
    expect(src('learning.ts')).not.toContain(preAssignmentTail);
    expect(src('learning.ts')).not.toContain(preWitness);
    expect(src('learning.ts')).not.toContain(preRequiredFor);
  });
});

describe('Learning read-tail r11 invariants', () => {
  test('r1-r10 helpers, completion columns and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe', 'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
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

  test('retirement:ts-statements is strictly lower than the r11 BEFORE=66 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(66);
    expect(total).toBeLessThanOrEqual(65);
  });
});
