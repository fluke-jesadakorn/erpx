import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { imageChildFrame } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Pre-r19 rendered bytes, captured verbatim at HEAD 77bc573d300d2b870938de0a5d230464d6488d08
// (learning.ts blob f5eb4b038a22e875ab54faeddac9091eeff328ca). Byte-identity oracles only;
// never live query sources. Fixed fragment values stand in for the unchanged source imports.
const pre = [
  {
    sql: "(SELECT jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id) FROM hr_people p WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) people",
    sha: '209dff9f3cb289a526f5f8ea9acf052ecc26af3ec30117bbced6a21cb92bc70c',
  },
  {
    sql: "(SELECT jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id) FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) engagements",
    sha: '955ad36d1d877faf5f0dbd6bc4f3d09f97fd32bee74ffa0ae7c43788a5994df0',
  },
  {
    sql: "(SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id) FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id) handoffs",
    sha: '59e7d54eeaa82db2b8ae007b5f80f6699791ac145cf4186d353fb1ee96279206',
  },
  {
    sql: "(SELECT jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id) FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id) certificates",
    sha: '828d2113dbf2df2f4292f607422a15fa592b99fdb2741c33c14db858983b7ef5',
  },
];
const preHeadSha = 'c1a66e629ca02af1e6f98cbd43e339f002570f9110dc94be743f5d1dcb95f3e9';

const fragments = {
  people: 'FROM hr_people p WHERE p.company_id=a.company_id AND p.user_id=a.learner_id',
  engagements: 'FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id',
  handoffs: 'FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id',
  certificates: 'FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id',
};

// Evaluate the live declaration against the real shared helper, so a drift in either
// the local delegation or imageChildFrame changes the rendered bytes under test.
const declaration = src('learning.ts').split('\n').find(line => line.startsWith('const assignmentChildImage = '))!;
const assignmentChildImage = new Function('imageChildFrame',
  `${declaration.replace('(aggregate: string, source: string, label: string)', '(aggregate, source, label)')}\nreturn assignmentChildImage;`,
)(imageChildFrame) as (aggregate: string, source: string, label: string) => string;

const rendered = [
  assignmentChildImage("jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id)", fragments.people, 'people'),
  assignmentChildImage("jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id)", fragments.engagements, 'engagements'),
  assignmentChildImage('jsonb_agg(to_jsonb(h) ORDER BY h.id)', fragments.handoffs, 'handoffs'),
  assignmentChildImage('jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id)', fragments.certificates, 'certificates'),
];
const head = `a.course_id,a.course_version,c.company_id course_company,c.required_for,\n ${rendered.join(',\n ')},\n CASE WHEN EXISTS(SELECT 1 <ELIGIBLE>) THEN 1 ELSE 0 END learner_eligible`;

describe('Learning read-tail r19 assignment-child delegation', () => {
  test('shared imageChildFrame is unchanged and learning.ts delegates to it exactly once', () => {
    expect(src('learning-read-sql.ts')).toContain(
      "export const imageChildFrame = (aggregate: string, source: string, separator = ' ') =>\n  `(SELECT ${aggregate}${separator}${source})`;",
    );
    expect(src('learning-read-sql.ts').match(/export const imageChildFrame =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts')).toContain('`${imageChildFrame(aggregate, source)} ${label}`');
    expect(src('learning.ts').match(/imageChildFrame\(/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/assignmentChildImage\(/g) ?? []).toHaveLength(4);
    expect(src('learning.ts')).toContain('.bind(id,actor.companyId)');
    expect(src('learning.ts')).not.toContain('`(SELECT ${aggregate} ${source}) ${label}`');
  });

  test('all four rendered subqueries are byte-identical to the pre-r19 oracles', () => {
    expect(rendered).toHaveLength(4);
    for (let i = 0; i < 4; i++) {
      expect(rendered[i]).toBe(pre[i].sql);
      expect(Buffer.byteLength(rendered[i])).toBe(Buffer.byteLength(pre[i].sql));
      expect(sha(rendered[i])).toBe(pre[i].sha);
      expect(rendered[i]).toContain('SELECT ');
      expect(rendered[i]).not.toContain('?');
    }
    expect(sha(head)).toBe(preHeadSha);
  });

  test('r1-r18 helpers, the r7 comment token and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).toContain('/** r7 — literal SELECT * source text, parameterized only by the owner table. */');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead',
      'assignmentCourseReadHead', 'packageScopeCte', 'benefitConsumptionHead', 'latestEventLateral',
      'identitySourceRead', 'workspaceImageFrame', 'clockedSourceCte', 'obligationFactHead',
      'imageChildFrame', 'certificateConsistencyProbe', 'jsonbAggImageHead', 'scopeRow',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts')).not.toContain('scopeRow(');
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

  test('retirement:ts-statements KPI is 37 (strictly below the r19 BEFORE=38 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(38);
    expect(total).toBe(37);
    expect(selectCount(src('learning.ts'))).toBe(0);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(35);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(selectCount(src('learning-completion-evidence.ts'))).toBe(1);
  });
});
