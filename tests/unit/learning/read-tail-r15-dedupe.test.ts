import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { certificateConsistencyProbe, imageChildFrame } from '../../../modules/learning/server/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/learning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r15 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preHandoff = `(SELECT to_jsonb(handoff) FROM learning_eligibility_handoffs handoff WHERE handoff.assignment_id=assignment.id AND handoff.company_id=assignment.company_id)`;
const preCertificates = `(SELECT jsonb_agg(jsonb_build_object('certificate',to_jsonb(certificate),'revocation',to_jsonb(revocation)) ORDER BY certificate.id)
 FROM learning_certificates certificate LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=certificate.id
 WHERE certificate.assignment_id=assignment.id AND certificate.company_id=assignment.company_id)`;
const preAssignments = `(SELECT jsonb_agg(jsonb_build_object('assignment',to_jsonb(assignment),
 'handoff',${preHandoff},
 'certificates',${preCertificates}) ORDER BY assignment.id)
 FROM learning_assignments assignment WHERE assignment.course_id=course.id AND assignment.company_id=course.company_id AND (?::text='learning_course' OR assignment.id=?))`;
const preBody = `'course',to_jsonb(course),'assignments',
 ${preAssignments}`;
const preNotExistsAssignment = `NOT EXISTS
 (SELECT 1 FROM handoffs h JOIN certificates c ON c.handoff_id=h.id AND c.company_id=h.company_id
 WHERE h.assignment_id=a.id AND h.source_assignment_version=a.event_version AND h.eligibility_code=a.required_for
 AND c.assignment_id=a.id AND c.source_assignment_version=h.source_assignment_version
 AND c.eligibility_code=h.eligibility_code AND c.person_id=h.person_id AND c.completion_kind=a.status))`;
const preNotExistsHandoff = `NOT EXISTS(SELECT 1 FROM certificates c WHERE c.handoff_id=h.id AND c.assignment_id=a.id
 AND c.source_assignment_version=h.source_assignment_version AND c.person_id=h.person_id
 AND c.eligibility_code=h.eligibility_code AND c.completion_kind=a.status)`;

// The three nested child frames as rendered by the r15 helper, in source order.
const renderedHandoff = imageChildFrame('to_jsonb(handoff)', 'FROM learning_eligibility_handoffs handoff WHERE handoff.assignment_id=assignment.id AND handoff.company_id=assignment.company_id');
const renderedCertificates = imageChildFrame('jsonb_agg(jsonb_build_object(\'certificate\',to_jsonb(certificate),\'revocation\',to_jsonb(revocation)) ORDER BY certificate.id)', `FROM learning_certificates certificate LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=certificate.id
 WHERE certificate.assignment_id=assignment.id AND certificate.company_id=assignment.company_id`, '\n ');
const renderedAssignments = imageChildFrame(`jsonb_agg(jsonb_build_object('assignment',to_jsonb(assignment),
 'handoff',${renderedHandoff},
 'certificates',${renderedCertificates}) ORDER BY assignment.id)`, `FROM learning_assignments assignment WHERE assignment.course_id=course.id AND assignment.company_id=course.company_id AND (?::text='learning_course' OR assignment.id=?)`, '\n ');

const renderedNotExistsAssignment = certificateConsistencyProbe('NOT EXISTS\n (', 'FROM handoffs h JOIN certificates c ON c.handoff_id=h.id AND c.company_id=h.company_id', '\n WHERE h.assignment_id=a.id AND h.source_assignment_version=a.event_version AND h.eligibility_code=a.required_for\n AND c.assignment_id=a.id AND c.source_assignment_version=h.source_assignment_version\n AND c.eligibility_code=h.eligibility_code AND c.person_id=h.person_id AND c.completion_kind=a.status)');
const renderedNotExistsHandoff = certificateConsistencyProbe('NOT EXISTS(', 'FROM certificates c WHERE c.handoff_id=h.id AND c.assignment_id=a.id\n AND c.source_assignment_version=h.source_assignment_version AND c.person_id=h.person_id\n AND c.eligibility_code=h.eligibility_code AND c.completion_kind=a.status', '');

describe('Learning read-tail r15 assignment child frames', () => {
  test('all three nested child frames and the whole body render byte-for-byte', () => {
    expect(renderedHandoff).toBe(preHandoff);
    expect(renderedCertificates).toBe(preCertificates);
    expect(renderedAssignments).toBe(preAssignments);
    expect(`'course',to_jsonb(course),'assignments',
 ${renderedAssignments}`).toBe(preBody);
    for (const sql of [preHandoff, preCertificates, preAssignments]) readOnly(sql);
  });

  test('family is defined once and composed at all three sites in the attachment reader', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const imageChildFrame =/g) ?? []).toHaveLength(1);
    const attachment = src('attachment-read.ts');
    expect(attachment.match(/imageChildFrame\(/g) ?? []).toHaveLength(3);
    expect(attachment).not.toContain(preAssignments);
    expect(attachment).not.toContain(preHandoff);
    expect(attachment).not.toContain(preCertificates);
    expect(attachment).toContain('.bind(type,id,companyId,type,id,type,id)');
  });
});

describe('Learning read-tail r15 certificate-consistency NOT EXISTS pair', () => {
  test('both settlement probes render byte-for-byte with their original order and close', () => {
    expect(renderedNotExistsAssignment).toBe(preNotExistsAssignment);
    expect(renderedNotExistsHandoff).toBe(preNotExistsHandoff);
    readOnly(renderedNotExistsAssignment);
    readOnly(renderedNotExistsHandoff);
  });

  test('family is defined once and composed at both callers with no inlining', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const certificateConsistencyProbe =/g) ?? []).toHaveLength(1);
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/certificateConsistencyProbe\(/g) ?? []).toHaveLength(2);
    expect(obligations).not.toContain(preNotExistsAssignment);
    expect(obligations).not.toContain(preNotExistsHandoff);
    expect(obligations).toContain('.bind(input.companyId)');
  });
});

describe('Learning read-tail r15 invariants', () => {
  test('r1-r14 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead',
      'assignmentCourseReadHead', 'packageScopeCte', 'benefitConsumptionHead', 'latestEventLateral',
      'identitySourceRead', 'workspaceImageFrame', 'clockedSourceCte', 'obligationFactHead',
    ]) {
      expect(shared.match(new RegExp(`export const ${name} ?=`, 'g')) ?? []).toHaveLength(1);
    }
    const native = (JSON.parse(src('learning-native-sources.json')) as {nativeSources: string}).nativeSources;
    expect(native).toBeDefined();
    expect((native!.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native!.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
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

  test('retirement:ts-statements is strictly lower than the r15 BEFORE=52 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(52);
    expect(total).toBeLessThanOrEqual(49);
  });
});
