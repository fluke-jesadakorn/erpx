import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import {
  benefitConsumptionHead,
  clockProbe,
  clockedSourceCte,
  identitySourceRead,
  latestEventLateral,
  workspaceImageFrame,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r13 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const consumptionImage = `jsonb_build_object('receipt',to_jsonb(r),'certificate',to_jsonb(c),'handoff',to_jsonb(h),'assignment',to_jsonb(a),'revocations',(SELECT jsonb_agg(to_jsonb(v) ORDER BY v.id) FROM learning_certificate_revocations v WHERE v.certificate_id=c.id))`;
const preWitness = `SELECT COALESCE(jsonb_agg(${consumptionImage} ORDER BY r.id),'[]'::jsonb)::text image
 FROM learning_benefit_consumption_receipts r LEFT JOIN learning_certificates c ON c.id=r.certificate_id LEFT JOIN learning_eligibility_handoffs h ON h.id=r.handoff_id LEFT JOIN learning_assignments a ON a.id=c.assignment_id WHERE r.company_id=? AND r.election_id=?`;
const preSettlement = `SELECT r.*,
 c.id certificate_source_id,c.company_id certificate_company_id,c.person_id certificate_person_id,c.assignment_id,
 c.handoff_id certificate_handoff_id,c.source_assignment_version certificate_source_version,c.eligibility_code certificate_code,
 h.id handoff_source_id,h.company_id handoff_company_id,h.person_id handoff_person_id,h.assignment_id handoff_assignment_id,
 h.source_assignment_version handoff_source_version,h.eligibility_code handoff_code,
 a.id assignment_source_id,a.company_id assignment_company_id,a.version assignment_version,a.status assignment_status,
 ${consumptionImage} image
 FROM learning_benefit_consumption_receipts r LEFT JOIN learning_certificates c ON c.id=r.certificate_id
 LEFT JOIN learning_eligibility_handoffs h ON h.id=r.handoff_id LEFT JOIN learning_assignments a ON a.id=c.assignment_id`;
const preLateralAssignment = 'LEFT JOIN LATERAL(SELECT e.* FROM learning_assignment_events e WHERE e.assignment_id=a.id ORDER BY e.version DESC LIMIT 1) event ON TRUE';
const preLateralCourse = 'LEFT JOIN LATERAL\n (SELECT e.id,e.version,e.status FROM learning_course_events e WHERE e.course_id=c.id ORDER BY e.version DESC LIMIT 1) event ON TRUE';
const preIdentityPurchases = "SELECT to_jsonb(source)-'updated_at' image FROM ${table} source WHERE id=? AND company_id=?";
const preIdentityLearner = 'SELECT learner_id,course_id FROM learning_assignments WHERE id=? AND company_id=?';
const preIdentitySensitive = 'SELECT a.company_id,a.version,a.branch_id,a.department_id,a.status,a.learner_id,\n        CASE WHEN iam_same_person(a.learner_id,?,a.company_id) THEN 1 ELSE 0 END is_learner\n      FROM learning_assignments a WHERE a.id=? AND a.company_id=?';
const preImagePurchases = "SELECT jsonb_build_object(\n  'courses',${coursesImage},\n  'assignments',${assignmentsImage('to_jsonb(a)')}))::text image";
const preImageEvaluation = 'SELECT jsonb_build_object(\'assignments\',${assignmentsImage(`to_jsonb(a)||jsonb_build_object(\'visible\',${scope.where})`)},\'courses\',${coursesImage})::text image';
const preClockEvaluation = 'WITH evaluation_learning_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)';
const preClockMetric = 'WITH learning_metric_clock AS ${clockProbe}';
const preClockSource = 'WITH learning_source_clock AS ${clockProbe}';

describe('Learning read-tail r13 benefit consumption head', () => {
  test('renders both pre-dedupe consumption reads byte-for-byte', () => {
    expect(benefitConsumptionHead(`COALESCE(jsonb_agg(${consumptionImage} ORDER BY r.id),'[]'::jsonb)::text image`) + ' WHERE r.company_id=? AND r.election_id=?').toBe(preWitness);
    expect(benefitConsumptionHead(preSettlement.slice('SELECT '.length, preSettlement.indexOf('\n FROM learning_benefit_consumption_receipts')), '\n ', '\n ')).toBe(preSettlement);
    readOnly(preWitness);
    readOnly(preSettlement);
  });

  test('family is defined once and composed at both callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const benefitConsumptionHead =/g) ?? []).toHaveLength(1);
    const facts = src('benefit-source-facts.ts');
    expect(facts.match(/benefitConsumptionHead\(/g) ?? []).toHaveLength(2);
    expect(facts).not.toContain(preWitness);
    expect(facts).not.toContain(preSettlement);
  });
});

describe('Learning read-tail r13 latest-event LATERAL', () => {
  test('renders both pre-dedupe event LATERALs byte-for-byte', () => {
    expect(latestEventLateral('e.*', 'learning_assignment_events', 'assignment_id', 'a')).toBe(preLateralAssignment);
    expect(latestEventLateral('e.id,e.version,e.status', 'learning_course_events', 'course_id', 'c', '\n ')).toBe(preLateralCourse);
    readOnly(preLateralAssignment);
    readOnly(preLateralCourse);
  });

  test('family is defined once and composed at both callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const latestEventLateral =/g) ?? []).toHaveLength(1);
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/latestEventLateral\(/g) ?? []).toHaveLength(2);
    expect(obligations).not.toContain(preLateralAssignment);
    expect(obligations).not.toContain(preLateralCourse);
  });
});

describe('Learning read-tail r13 identity source read', () => {
  test('renders all three pre-dedupe identity reads byte-for-byte', () => {
    expect(identitySourceRead("to_jsonb(source)-'updated_at' image", '${table} source')).toBe(preIdentityPurchases);
    expect(identitySourceRead('learner_id,course_id', 'learning_assignments')).toBe(preIdentityLearner);
    expect(identitySourceRead(preIdentitySensitive.slice('SELECT '.length, preIdentitySensitive.indexOf('\n      FROM')), 'learning_assignments a', '\n      ', 'a.')).toBe(preIdentitySensitive);
    for (const sql of [preIdentityPurchases, preIdentityLearner, preIdentitySensitive]) readOnly(sql);
  });

  test('family is defined once and composed at all three callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const identitySourceRead =/g) ?? []).toHaveLength(1);
    for (const [file, count] of [['purchases.ts', 1], ['learning.ts', 1], ['attachments-sensitive-facts.ts', 1]] as const) {
      expect(src(file).match(/identitySourceRead\(/g) ?? []).toHaveLength(count);
    }
    expect(src('purchases.ts')).not.toContain(preIdentityPurchases);
    expect(src('learning.ts')).not.toContain(preIdentityLearner);
    expect(src('attachments-sensitive-facts.ts')).not.toContain(preIdentitySensitive);
  });
});

describe('Learning read-tail r13 workspace image frame', () => {
  test('renders both pre-dedupe image frames byte-for-byte', () => {
    expect(workspaceImageFrame(preImagePurchases.slice('SELECT jsonb_build_object('.length, -'::text image'.length - 1))).toBe(preImagePurchases);
    expect(workspaceImageFrame(preImageEvaluation.slice('SELECT jsonb_build_object('.length, -'::text image'.length - 1))).toBe(preImageEvaluation);
    readOnly(preImagePurchases);
    readOnly(preImageEvaluation);
  });

  test('family is defined once and composed at both callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const workspaceImageFrame =/g) ?? []).toHaveLength(1);
    expect(src('purchases.ts').match(/workspaceImageFrame\(/g) ?? []).toHaveLength(1);
    expect(src('people-evaluation-facts.ts').match(/workspaceImageFrame\(/g) ?? []).toHaveLength(1);
    expect(src('purchases.ts')).not.toContain(preImagePurchases);
    expect(src('people-evaluation-facts.ts')).not.toContain(preImageEvaluation);
  });
});

describe('Learning read-tail r13 clocked source CTE', () => {
  test('renders all three pre-dedupe clocked CTEs byte-for-byte', () => {
    expect(clockedSourceCte('evaluation_learning_clock', '')).toBe('WITH evaluation_learning_clock AS MATERIALIZED(SELECT ' + packageClockSql + ' now_ms)');
    expect(clockedSourceCte('learning_metric_clock')).toBe('WITH learning_metric_clock AS ' + clockProbe);
    expect(clockedSourceCte('learning_source_clock')).toBe('WITH learning_source_clock AS ' + clockProbe);
    expect(preClockEvaluation).toContain('MATERIALIZED(SELECT ');
    expect(preClockMetric).toContain('${clockProbe}');
    expect(preClockSource).toContain('${clockProbe}');
    readOnly(clockedSourceCte('learning_metric_clock'));
  });

  test('family is defined once and composed at all three callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const clockedSourceCte =/g) ?? []).toHaveLength(1);
    for (const file of ['evaluation-source-policy.ts', 'metric-purchases.ts', 'record-purchases.ts']) {
      expect(src(file).match(/clockedSourceCte\(/g) ?? []).toHaveLength(1);
    }
    expect(src('evaluation-source-policy.ts')).not.toContain(preClockEvaluation);
    expect(src('metric-purchases.ts')).not.toContain(preClockMetric);
    expect(src('record-purchases.ts')).not.toContain(preClockSource);
  });
});

describe('Learning read-tail r13 invariants', () => {
  test('r1-r12 helpers, completion columns and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of ['actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe', 'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe', 'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe', 'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead', 'assignmentCourseReadHead', 'packageScopeCte']) {
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

  test('retirement:ts-statements is strictly lower than the r13 BEFORE=63 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(63);
    expect(total).toBeLessThanOrEqual(57);
  });
});
