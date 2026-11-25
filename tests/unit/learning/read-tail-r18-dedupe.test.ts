import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { existsRowProbe } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r18 literal copied verbatim from HEAD (git blob 7fa261f5cdd...). Byte-identity oracle only, never a live source.
const preLearnerEligible = "EXISTS(SELECT 1 ${identityaccessSources.learning_adapters_learning_assignments_x_from} ${peoplepayrollSources.learning_adapters_learning_hr_people_p_join} ON p.user_id=x.user_id AND p.company_id=x.company_id ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=a.company_id AND x.user_id=a.learner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=a.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=a.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')";

// Identical source-fragment values feed both renderings; only the framing differs.
const fragments = {
  identityaccessSources: { learning_adapters_learning_assignments_x_from: 'FROM learning_assignments x' },
  peoplepayrollSources: {
    learning_adapters_learning_hr_people_p_join: 'JOIN hr_people p',
    learning_adapters_learning_hr_engagements_e_join: 'JOIN hr_engagements e',
  },
};
const render = (declaration: string) => new Function(
  'existsRowProbe', 'identityaccessSources', 'peoplepayrollSources',
  `${declaration}\nreturn learnerEligible;`,
)(existsRowProbe, fragments.identityaccessSources, fragments.peoplepayrollSources) as string;

const currentDeclaration = src('learning.ts').split('\n').find(line => line.startsWith('const learnerEligible = '))!;
const preRendered = render(`const learnerEligible = \`${preLearnerEligible}\`;`);
const currentRendered = render(currentDeclaration);

describe('Learning read-tail r18 learnerEligible existsRowProbe', () => {
  test('renders the pre-r18 EXISTS probe byte-for-byte with unchanged binds', () => {
    expect(currentRendered).toBe(preRendered);
    expect(currentRendered.startsWith('EXISTS(SELECT 1 FROM learning_assignments x JOIN hr_people p ON p.user_id=x.user_id')).toBe(true);
    expect(currentRendered).toContain("AND e.status='active')");
    expect((preLearnerEligible.match(/\?/g) ?? []).length).toBe(0);
    expect((currentRendered.match(/\?/g) ?? []).length).toBe(0);
    readOnly(preRendered);
    readOnly(currentRendered);
  });

  test('helper is defined once and composed at the one new caller with binds unchanged', () => {
    expect(src('learning-read-sql.ts').match(/export const existsRowProbe =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts').match(/existsRowProbe\(/g) ?? []).toHaveLength(1);
    expect(src('learning.ts')).not.toContain(preLearnerEligible);
    expect(src('learning.ts')).toContain('const learnerEligible = `EXISTS(${existsRowProbe(`');
    expect(src('learning.ts')).toContain('.bind(id,actor.companyId)');
  });
});

describe('Learning read-tail r18 deferred families and invariants', () => {
  test('r5/r6 helpers stay defined once; r19 delegates the r5 child frame to imageChildFrame', () => {
    expect(src('learning.ts').match(/const assignmentChildImage =/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts').match(/const orgNodeLabel =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts')).toContain('`${imageChildFrame(aggregate, source)} ${label}`');
    expect(src('executive-metric-projections.ts')).toContain('COALESCE((SELECT n.display_name');
    expect(src('learning.ts').match(/imageChildFrame\(/g) ?? []).toHaveLength(1);
    expect(src('executive-metric-projections.ts')).not.toContain('scopeRow(');
  });

  test('r1-r17 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
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

  test('retirement:ts-statements is strictly lower than the r18 BEFORE=39 baseline (r20 pins 36)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(39);
    expect(total).toBe(36);
    expect(selectCount(src('learning.ts'))).toBe(0);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(34);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(selectCount(src('learning-completion-evidence.ts'))).toBe(1);
  });
});
