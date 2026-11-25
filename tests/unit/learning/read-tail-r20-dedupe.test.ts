import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { identitySourceRead, learningAssignmentsExistsProbe, learningCourseProjection } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r20 rendered bytes, captured verbatim at HEAD 614418b37bd2aa6fdd9ea437956b9992bdd2054b
// (learning-read-sql.ts blob fc68cae1d5a77fe288f174d3f66743489af70f46). Byte-identity
// oracles only; never live query sources. Fixed fragments stand in for unchanged imports.
const pre = [
  {
    name: 'purchase',
    sql: "SELECT c.*,NULL::text branch_id,NULL::text department_id,(to_jsonb(c)-'version'-'updated_at')::text image FROM learning_courses c WHERE c.id=? AND c.company_id=?",
    sha: '00e1a355b022869ba26b831bf00ffbf1848ebaefc4761f2ae30ca95735fb89a9',
    binds: 2,
  },
  {
    name: 'sensitive',
    sql: `SELECT c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,
        CASE WHEN EXISTS(SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled') THEN 1 ELSE 0 END is_learner
      FROM learning_courses c WHERE c.id=? AND c.company_id=?`,
    sha: '8209ccb5e9c7da2d5a7a79b2302523cd3cddeca13899eb175273b73e99db8ccc',
    binds: 3,
  },
  {
    name: 'reference',
    sql: 'SELECT to_jsonb(c) image FROM learning_courses c WHERE c.id=? AND c.company_id=?',
    sha: '8409af64631bca3c9369723eaa1a17c0f582103dd99a2378c305a116da5ab7a1',
    binds: 2,
  },
];

const sensitiveProbe = learningAssignmentsExistsProbe('a', "a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled'");
const calls = [
  { name: 'purchase', projection: "c.*,NULL::text branch_id,NULL::text department_id,(to_jsonb(c)-'version'-'updated_at')::text image", separator: ' ' },
  {
    name: 'sensitive',
    projection: `c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,\n        CASE WHEN ${sensitiveProbe} THEN 1 ELSE 0 END is_learner`,
    separator: '\n      ',
  },
  { name: 'reference', projection: 'to_jsonb(c) image', separator: ' ' },
];
const rendered = calls.map(c => learningCourseProjection(c.projection, c.separator));
// The folded head is the existing r13 owner-local renderer with a fixed 'c.' key.
const delegated = calls.map(c => identitySourceRead(c.projection, 'learning_courses c', c.separator, 'c.'));

describe('Learning read-tail r20 learning_courses single-row head fold', () => {
  test('all three rendered reads are byte-identical to the pre-r20 oracles', () => {
    expect(rendered).toHaveLength(3);
    for (let i = 0; i < 3; i++) {
      expect(calls[i].name).toBe(pre[i].name);
      expect(rendered[i]).toBe(pre[i].sql);
      expect(Buffer.byteLength(rendered[i])).toBe(Buffer.byteLength(pre[i].sql));
      expect(sha(rendered[i])).toBe(pre[i].sha);
      expect((rendered[i].match(/\?/g) ?? []).length).toBe(pre[i].binds);
      expect(rendered[i]).toContain('FROM learning_courses c WHERE c.id=? AND c.company_id=?');
      readOnly(rendered[i]);
    }
  });

  test('delegating the head to identitySourceRead renders the same bytes', () => {
    for (let i = 0; i < 3; i++) {
      expect(delegated[i]).toBe(rendered[i]);
      expect(delegated[i]).toBe(pre[i].sql);
      expect(sha(delegated[i])).toBe(pre[i].sha);
    }
  });

  test('the one query head lives only in identitySourceRead; callers keep their binds', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const learningCourseProjection =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const identitySourceRead =/g) ?? []).toHaveLength(1);
    expect(shared).toContain("identitySourceRead(projection, 'learning_courses c', separator, 'c.')");
    expect(shared).not.toContain('`SELECT ${projection}${separator}FROM learning_courses c WHERE c.id=? AND c.company_id=?`');
    for (const file of ['attachment-purchases.ts', 'attachments-sensitive-facts.ts', 'learning.ts']) {
      expect(src(file).match(/learningCourseProjection\(/g) ?? []).toHaveLength(1);
      expect(src(file)).not.toContain('FROM learning_courses c WHERE c.id=? AND c.company_id=?');
    }
    expect(src('attachment-purchases.ts')).toContain('.bind(id,actor.companyId)');
    expect(src('attachments-sensitive-facts.ts')).toContain('.bind(facts.userId,facts.targetId,facts.companyId)');
    expect(src('learning.ts')).toContain('.bind(courseId,actor.companyId)');
  });
});

describe('Learning read-tail r20 invariants', () => {
  test('r1-r19 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'assignmentCourseJoinHead', 'assignmentCourseReadHead', 'packageScopeCte',
      'benefitConsumptionHead', 'latestEventLateral', 'workspaceImageFrame', 'clockedSourceCte',
      'obligationFactHead', 'imageChildFrame', 'certificateConsistencyProbe', 'jsonbAggImageHead', 'scopeRow',
      'existsRowProbe', 'keyedRowRead', 'learningCourseProjection', 'identitySourceRead',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts')).toContain('`${imageChildFrame(aggregate, source)} ${label}`');
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

  test('retirement:ts-statements KPI is 31 (strictly below the r24 BEFORE=33 baseline)', () => {
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
    expect(total).toBe(31);
    expect(selectCount(src('learning.ts'))).toBe(0);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(30);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
  });
});
