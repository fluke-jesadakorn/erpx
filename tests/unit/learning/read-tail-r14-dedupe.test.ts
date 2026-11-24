import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { obligationFactHead, workspaceImageFrame } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r14 literals copied verbatim from HEAD. Byte-identity oracles only, never live sources.
const preFactHeads = [
  "SELECT 'course' kind,c.id,c.version,c.status state,",
  "UNION ALL SELECT 'assignment',a.id,a.version,a.status,",
  "UNION ALL SELECT 'handoff',h.id,h.source_assignment_version,h.status,",
  "UNION ALL SELECT 'certificate',c.id,c.source_assignment_version,c.completion_kind,",
  "UNION ALL SELECT 'revocation',r.id,0::bigint,'revoked',",
] as const;
const preImageFrame = "SELECT jsonb_build_object('course',to_jsonb(course),'assignments',\n (SELECT jsonb_agg(jsonb_build_object('assignment',to_jsonb(assignment),\n 'handoff',(SELECT to_jsonb(handoff) FROM learning_eligibility_handoffs handoff WHERE handoff.assignment_id=assignment.id AND handoff.company_id=assignment.company_id),\n 'certificates',(SELECT jsonb_agg(jsonb_build_object('certificate',to_jsonb(certificate),'revocation',to_jsonb(revocation)) ORDER BY certificate.id)\n FROM learning_certificates certificate LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=certificate.id\n WHERE certificate.assignment_id=assignment.id AND certificate.company_id=assignment.company_id)) ORDER BY assignment.id)\n FROM learning_assignments assignment WHERE assignment.course_id=course.id AND assignment.company_id=course.company_id AND (?::text='learning_course' OR assignment.id=?)))::text image";

describe('Learning read-tail r14 obligation fact-row head', () => {
  test('renders all five pre-dedupe fact heads byte-for-byte', () => {
    const rendered = [
      obligationFactHead('', "'course' kind", 'c.id,c.version,c.status state'),
      obligationFactHead('UNION ALL ', "'assignment'", 'a.id,a.version,a.status'),
      obligationFactHead('UNION ALL ', "'handoff'", 'h.id,h.source_assignment_version,h.status'),
      obligationFactHead('UNION ALL ', "'certificate'", 'c.id,c.source_assignment_version,c.completion_kind'),
      obligationFactHead('UNION ALL ', "'revocation'", "r.id,0::bigint,'revoked'"),
    ];
    rendered.forEach((sql, i) => expect(sql).toBe(preFactHeads[i]));
    preFactHeads.forEach(readOnly);
  });

  test('family is defined once and composed at all five callers', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const obligationFactHead =/g) ?? []).toHaveLength(1);
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/obligationFactHead\(/g) ?? []).toHaveLength(5);
    for (const head of preFactHeads) expect(obligations).not.toContain(head);
  });
});

describe('Learning read-tail r14 attachment image frame', () => {
  test('renders the pre-dedupe attachment image frame byte-for-byte', () => {
    const body = preImageFrame.slice('SELECT jsonb_build_object('.length, -')::text image'.length);
    expect(workspaceImageFrame(body)).toBe(preImageFrame);
    readOnly(preImageFrame);
  });

  test('family is defined once and composed at the attachment reader', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const workspaceImageFrame =/g) ?? []).toHaveLength(1);
    expect(src('purchases.ts').match(/workspaceImageFrame\(/g) ?? []).toHaveLength(1);
    expect(src('people-evaluation-facts.ts').match(/workspaceImageFrame\(/g) ?? []).toHaveLength(1);
    const attachment = src('attachment-read.ts');
    expect(attachment.match(/workspaceImageFrame\(/g) ?? []).toHaveLength(1);
    expect(attachment).not.toContain(preImageFrame);
  });
});

describe('Learning read-tail r14 invariants', () => {
  test('r1-r13 helpers, native sources and all 7 owner LOCK TABLE statements stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
      'assignmentsImage', 'sourceLiteral', 'metricLabelProjection', 'learningAssignmentsExistsProbe',
      'currentAssignmentScopeHead', 'learningCourseProjection', 'assignmentCourseJoinHead',
      'assignmentCourseReadHead', 'packageScopeCte', 'benefitConsumptionHead', 'latestEventLateral',
      'identitySourceRead', 'workspaceImageFrame', 'clockedSourceCte',
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

  test('retirement:ts-statements is strictly lower than the r13 BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(57);
    expect(total).toBe(52);
  });
});
