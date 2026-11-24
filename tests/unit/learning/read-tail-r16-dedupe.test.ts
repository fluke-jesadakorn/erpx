import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { jsonbAggImageHead, scopeRow } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r16 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preCourseImage = '(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM learning_courses c WHERE c.company_id=?)';
const preRevocationImage = '(SELECT jsonb_agg(to_jsonb(v) ORDER BY v.id) FROM learning_certificate_revocations v WHERE v.certificate_id=c.id)';
const preLearnerAssignments = 'SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) image ';
const preActorScope = '(SELECT ${columns})';
const preRecordScope = '(SELECT ?::text id,?::text company_id)';
const preSettlementScope = '(SELECT ?::text company_id)';

// The three image heads and three scope rows as rendered by the r16 helpers.
const renderedCourseImage = `(${jsonbAggImageHead('c')} FROM learning_courses c WHERE c.company_id=?)`;
const renderedRevocationImage = `(${jsonbAggImageHead('v')} FROM learning_certificate_revocations v WHERE v.certificate_id=c.id)`;
const renderedLearnerAssignments = `${jsonbAggImageHead('x')} image `;
const renderedActorScope = scopeRow('${columns}');
const renderedRecordScope = scopeRow('?::text id,?::text company_id');
const renderedSettlementScope = scopeRow('?::text company_id');

describe('Learning read-tail r16 jsonb_agg image head', () => {
  test('all three image heads render byte-for-byte with their original tails', () => {
    expect(renderedCourseImage).toBe(preCourseImage);
    expect(renderedRevocationImage).toBe(preRevocationImage);
    expect(renderedLearnerAssignments).toBe(preLearnerAssignments);
    for (const sql of [renderedCourseImage, renderedRevocationImage, renderedLearnerAssignments]) readOnly(sql);
  });

  test('family is defined once and composed at three callers with binds unchanged', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const jsonbAggImageHead =/g) ?? []).toHaveLength(1);
    expect(shared).not.toContain(preCourseImage);
    expect(shared).toContain('(${jsonbAggImageHead(\'c\')} FROM learning_courses c WHERE c.company_id=?)');

    const benefits = src('benefit-source-facts.ts');
    expect(benefits.match(/jsonbAggImageHead\(/g) ?? []).toHaveLength(1);
    expect(benefits).not.toContain(preRevocationImage);
    expect(benefits).toContain('.bind(companyId,electionId)');

    const learning = src('learning.ts');
    expect(learning.match(/jsonbAggImageHead\(/g) ?? []).toHaveLength(1);
    expect(learning).not.toContain(preLearnerAssignments);
    expect(learning).toContain('.bind(actor.companyId,learnerId)');
  });
});

describe('Learning read-tail r16 scope row', () => {
  test('all three scope rows render byte-for-byte with their original columns', () => {
    expect(renderedActorScope).toBe(preActorScope);
    expect(renderedRecordScope).toBe(preRecordScope);
    expect(renderedSettlementScope).toBe(preSettlementScope);
    for (const sql of [renderedActorScope, renderedRecordScope, renderedSettlementScope]) readOnly(sql);
  });

  test('family is defined once and composed at three callers with binds unchanged', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const scopeRow =/g) ?? []).toHaveLength(1);
    expect(shared).toContain('EXISTS(SELECT 1 FROM${spacing}${scopeRow(columns)}${spacing}learning_actor_scope');

    const records = src('record-purchases.ts');
    expect(records.match(/scopeRow\(/g) ?? []).toHaveLength(1);
    expect(records).not.toContain(preRecordScope);
    expect(records).toContain('.bind(...scope.args,id,actor.companyId)');

    const obligations = src('package-obligations.ts');
    expect(obligations.match(/scopeRow\(/g) ?? []).toHaveLength(1);
    expect(obligations).not.toContain(preSettlementScope);
    expect(obligations).toContain('.bind(input.companyId)');
  });
});

describe('Learning read-tail r16 invariants', () => {
  test('r1-r15 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead',
      'assignmentCourseReadHead', 'packageScopeCte', 'benefitConsumptionHead', 'latestEventLateral',
      'identitySourceRead', 'workspaceImageFrame', 'clockedSourceCte', 'obligationFactHead',
      'imageChildFrame', 'certificateConsistencyProbe',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
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

  test('retirement:ts-statements is strictly lower than the r16 BEFORE=49 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(49);
    // r17 lowers this further; r16 only pins that its own baseline decreased.
    expect(total).toBeLessThanOrEqual(45);
  });
});
