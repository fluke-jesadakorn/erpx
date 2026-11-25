import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  existsRowProbe,
  learningAssignmentsExistsProbe,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Pre-r21 rendered bytes, captured verbatim at HEAD ea54dbb94 (learning-read-sql.ts
// blob 8c6bda95ed5a06bb8d262e2f2ac654a2997c5f73). Byte-identity oracles only;
// never live query sources. The two callers keep their own binds; the folded
// head carries none of its own.
const pre = [
  {
    name: 'attachment-read',
    alias: 'selected',
    conditions: 'selected.id=? AND selected.course_id=course.id AND selected.company_id=course.company_id',
    sql: 'EXISTS(SELECT 1 FROM learning_assignments selected WHERE selected.id=? AND selected.course_id=course.id AND selected.company_id=course.company_id)',
    sha: 'db2fee750dfbfb569a393590c4f8e89e85e66b720a2f04bdac18dc27d19e2eae',
    bytes: 146,
    binds: 1,
  },
  {
    name: 'sensitive-facts',
    alias: 'a',
    conditions: "a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled'",
    sql: "EXISTS(SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled')",
    sha: '214f458fa5c68eb34d5fdb48720295fcb0f32db0ef8e63356920d5ea94cf0516',
    bytes: 142,
    binds: 1,
  },
];

// The single folded head still renders from the r17 renderer byte-for-byte.
const head = existsRowProbe('FROM learning_assignments a', 'a.course_id=c.id');
const headSql = 'SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id';
const headSha = 'cf2c9f5006870348575634528142275ddc9663e2e69d9e352fd9617d9f7b8b20';

describe('Learning read-tail r21 assignments-existence head fold', () => {
  test('both rendered probes are byte-identical to the pre-r21 oracles', () => {
    expect(pre).toHaveLength(2);
    for (const p of pre) {
      const rendered = learningAssignmentsExistsProbe(p.alias, p.conditions);
      expect(rendered).toBe(p.sql);
      expect(Buffer.byteLength(rendered)).toBe(p.bytes);
      expect(sha(rendered)).toBe(p.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(p.binds);
      readOnly(rendered);
    }
  });

  test('the folded head is the r17 existsRowProbe head, byte-identically', () => {
    expect(head).toBe(headSql);
    expect(sha(head)).toBe(headSha);
    expect((head.match(/\?/g) ?? []).length).toBe(0);
    readOnly(head);
  });

  test('learningAssignmentsExistsProbe delegates through existsRowProbe; head lives only there', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const existsRowProbe =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const learningAssignmentsExistsProbe =/g) ?? []).toHaveLength(1);
    expect(shared).toContain('`EXISTS(${existsRowProbe(`FROM learning_assignments ${alias}`, conditions)})`');
    const decl = shared.match(/export const learningAssignmentsExistsProbe =[\s\S]*?;\n/)?.[0];
    expect(decl).toBeDefined();
    expect(decl!).not.toContain('SELECT 1');
    expect(decl!).not.toContain('FROM learning_assignments ${alias} WHERE');

    // Prove the live declaration actually routes through the injected renderer.
    const build = new Function(
      'existsRowProbe',
      `${decl!.replace('export ', '').replace('(alias: string, conditions: string)', '(alias, conditions)')}\nreturn learningAssignmentsExistsProbe;`,
    ) as (probe: (source: string, where: string) => string) =>
      (alias: string, conditions: string) => string;
    const delegate = build((source, where) => `<${source}|${where}>`);
    expect(delegate('a', 'c')).toBe('EXISTS(<FROM learning_assignments a|c>)');
  });

  test('both callers, their binds and every lock/native source stay untouched', () => {
    const read = src('attachment-read.ts');
    const sensitive = src('attachments-sensitive-facts.ts');
    expect(read.match(/learningAssignmentsExistsProbe\(/g) ?? []).toHaveLength(1);
    expect(sensitive.match(/learningAssignmentsExistsProbe\(/g) ?? []).toHaveLength(1);
    expect(read).toContain('learningAssignmentsExistsProbe(');
    expect(sensitive).toContain('learningAssignmentsExistsProbe(');
    for (const p of pre) {
      expect(read.includes(p.sql) && sensitive.includes(p.sql)).toBe(false);
    }
    expect(read).toContain('.bind(type,id,companyId,type,id,type,id)');
    expect(sensitive).toContain('.bind(facts.userId,facts.targetId,facts.companyId)');

    const shared = src('learning-read-sql.ts');
    const native = (JSON.parse(src('learning-native-sources.json')) as {nativeSources: string}).nativeSources;
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

describe('Learning read-tail r21 invariants', () => {
  test('r1-r20 helpers stay defined exactly once', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'assignmentCourseJoinHead', 'assignmentCourseReadHead', 'packageScopeCte',
      'benefitConsumptionHead', 'latestEventLateral', 'workspaceImageFrame', 'clockedSourceCte',
      'obligationFactHead', 'learningCourseProjection', 'identitySourceRead', 'jsonbAggImageHead', 'scopeRow',
      'existsRowProbe', 'keyedRowRead', 'imageChildFrame', 'certificateConsistencyProbe',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
  });

  test('retirement:ts-statements KPI is 21 (strictly below the r24 BEFORE=33 baseline)', () => {
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
    expect(total).toBe(21);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(20);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(selectCount(src('learning.ts'))).toBe(0);
  });
});
