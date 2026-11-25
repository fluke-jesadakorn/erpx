import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { learningAssignmentsExistsProbe, metricLabelProjection } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r8 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const srcSql = 'SOURCE_SQL';
const scopeA = 'a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)';
const completionLabel = "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
const completionFrom = `learning_assignment_events e JOIN (${srcSql}) a ON a.id=e.assignment_id JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE ${scopeA} AND e.action='complete' AND e.status='completed' AND e.created_at>=? AND e.created_at<?`;
const preCompletion = `SELECT ${completionLabel} label,count(*)::text value FROM ${completionFrom} GROUP BY 1 ORDER BY 1`;
const overdueLabel = 'c.title';
const overdueFrom = `(${srcSql}) a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE ${scopeA} AND c.required_for IS NOT NULL AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND a.status IN ('assigned','in_progress')`;
const preOverdue = `SELECT ${overdueLabel} label,count(*)::text value FROM ${overdueFrom} GROUP BY 1 ORDER BY 1`;

describe('Learning read-tail r8 byte-identical family', () => {
  test('metric projection renders both original breakdown queries byte-for-byte', () => {
    expect(metricLabelProjection(completionLabel, completionFrom)).toBe(preCompletion);
    expect(metricLabelProjection(overdueLabel, overdueFrom)).toBe(preOverdue);
    readOnly(metricLabelProjection(overdueLabel, overdueFrom));
  });

  test('family is defined once and composed at both callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const metricLabelProjection =/g) ?? []).toHaveLength(1);
    const projections = src('executive-metric-projections.ts');
    expect(projections.match(/metricLabelProjection\(/g) ?? []).toHaveLength(2);
    expect(projections).not.toContain('label,count(*)::text value FROM learning_assignment_events');
    expect(projections).not.toContain('label,count(*)::text value FROM (${source.sql})');
  });

  test('r1-r7 helpers and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral']) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
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

describe('Learning read-tail r8b assignments existence probe family', () => {
  // Pre-dedupe literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
  const preAttachmentRead = "EXISTS(SELECT 1 FROM learning_assignments selected WHERE selected.id=? AND selected.course_id=course.id AND selected.company_id=course.company_id)";
  const preSensitiveFacts = "EXISTS(SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled')";

  test('renders both cross-file probe heads byte-for-byte', () => {
    expect(learningAssignmentsExistsProbe('selected', 'selected.id=? AND selected.course_id=course.id AND selected.company_id=course.company_id')).toBe(preAttachmentRead);
    expect(learningAssignmentsExistsProbe('a', "a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled'")).toBe(preSensitiveFacts);
    readOnly(learningAssignmentsExistsProbe('a', "a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled'"));
  });

  test('the shared probe is defined once and composed at both callers', () => {
    expect(src('learning-read-sql.ts').match(/export const learningAssignmentsExistsProbe =/g) ?? []).toHaveLength(1);
    expect(src('attachment-read.ts').match(/learningAssignmentsExistsProbe\(/g) ?? []).toHaveLength(1);
    expect(src('attachments-sensitive-facts.ts').match(/learningAssignmentsExistsProbe\(/g) ?? []).toHaveLength(1);
    expect(src('attachment-read.ts')).not.toContain(preAttachmentRead);
    expect(src('attachments-sensitive-facts.ts')).not.toContain(preSensitiveFacts);
  });
});

describe('Learning read-tail r8 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=71 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(71);
  });
});
