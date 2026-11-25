import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {assignmentCourseJoinHead,learningAssignmentsExistsProbe,learningCourseProjection} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r10 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preCoursePurchase = "SELECT c.*,NULL::text branch_id,NULL::text department_id,(to_jsonb(c)-'version'-'updated_at')::text image FROM learning_courses c WHERE c.id=? AND c.company_id=?";
const sensitiveProbe = learningAssignmentsExistsProbe('a', "a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled'");
const preCourseSensitive = `SELECT c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,
        CASE WHEN ${sensitiveProbe} THEN 1 ELSE 0 END is_learner
      FROM learning_courses c WHERE c.id=? AND c.company_id=?`;
const preCourseReference = 'SELECT to_jsonb(c) image FROM learning_courses c WHERE c.id=? AND c.company_id=?';
const assignmentProjection = (owned: boolean) => `a.*,jsonb_build_object('assignment',to_jsonb(a)-'version'-'updated_at'${owned ? "-'status'-'attendance'-'score_bps'-'result_evidence'-'completion_reason'" : ''},'course',to_jsonb(c)-'version'-'updated_at')::text image`;
const preAttachmentAssignment = (owned: boolean) => `SELECT ${assignmentProjection(owned)}
 FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=?`;
const evaluationWhere = 'a.company_id=?';
const preEvaluationEvidence = `SELECT a.id,c.title,a.status,a.version,a.updated_at FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.company_id=? AND a.learner_id=? AND a.status='completed' AND (?::text IS NULL OR a.id=?) AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND ${evaluationWhere} ORDER BY a.updated_at DESC LIMIT 100`;

describe('Learning read-tail r10 learning_courses single-row projection family', () => {
  test('renders all three pre-dedupe course projections byte-for-byte', () => {
    expect(learningCourseProjection("c.*,NULL::text branch_id,NULL::text department_id,(to_jsonb(c)-'version'-'updated_at')::text image")).toBe(preCoursePurchase);
    expect(learningCourseProjection(`c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,
        CASE WHEN ${sensitiveProbe} THEN 1 ELSE 0 END is_learner`, '\n      ')).toBe(preCourseSensitive);
    expect(learningCourseProjection('to_jsonb(c) image')).toBe(preCourseReference);
    readOnly(preCoursePurchase);
    readOnly(preCourseSensitive);
    readOnly(preCourseReference);
  });

  test('family is defined once and composed at all three callers', () => {
    expect(src('learning-read-sql.ts').match(/export const learningCourseProjection =/g) ?? []).toHaveLength(1);
    expect(src('attachment-purchases.ts').match(/learningCourseProjection\(/g) ?? []).toHaveLength(1);
    expect(src('attachments-sensitive-facts.ts').match(/learningCourseProjection\(/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/learningCourseProjection\(/g) ?? []).toHaveLength(1);
    expect(src('attachment-purchases.ts')).not.toContain(preCoursePurchase);
    expect(src('attachments-sensitive-facts.ts')).not.toContain('FROM learning_courses c WHERE c.id=? AND c.company_id=?');
    expect(src('learning.ts')).not.toContain(preCourseReference);
  });
});

describe('Learning read-tail r10 assignment x course join head family', () => {
  test('renders both pre-dedupe join heads byte-for-byte', () => {
    for (const owned of [false, true]) {
      expect(`${assignmentCourseJoinHead(assignmentProjection(owned), '\n ')} WHERE a.id=? AND a.company_id=?`).toBe(preAttachmentAssignment(owned));
      readOnly(preAttachmentAssignment(owned));
    }
    expect(`${assignmentCourseJoinHead('a.id,c.title,a.status,a.version,a.updated_at')} WHERE a.company_id=? AND a.learner_id=? AND a.status='completed' AND (?::text IS NULL OR a.id=?) AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND ${evaluationWhere} ORDER BY a.updated_at DESC LIMIT 100`).toBe(preEvaluationEvidence);
    readOnly(preEvaluationEvidence);
  });

  test('family is defined once and composed at both callers', () => {
    expect(src('learning-read-sql.ts').match(/export const assignmentCourseJoinHead =/g) ?? []).toHaveLength(1);
    expect(src('attachment-purchases.ts').match(/assignmentCourseJoinHead\(/g) ?? []).toHaveLength(1);
    expect(src('people-evaluation-facts.ts').match(/assignmentCourseJoinHead\(/g) ?? []).toHaveLength(1);
    expect(src('attachment-purchases.ts')).not.toContain('FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.id=?');
    expect(src('people-evaluation-facts.ts')).not.toContain('FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.company_id=?');
  });
});

describe('Learning read-tail r10 invariants', () => {
  test('r1-r9 helpers and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe', 'currentAssignmentScopeHead']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
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

  test('retirement:ts-statements is strictly lower than the r10 BEFORE=69 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(69);
    expect(total).toBeLessThanOrEqual(66);
  });
});
