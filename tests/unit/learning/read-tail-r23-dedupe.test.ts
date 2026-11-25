import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  imageChildFrame,
  scopeRow,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Pre-r23 rendered scope rows, captured verbatim at the task-start source
// (learning-read-sql.ts blob e1df4d32068247d9820b687c8a8155858cefd484).
// Byte-identity oracles only; never live query sources. Binds stay with callers.
const pre = [
  {
    name: 'actor-scope',
    columns: '${columns}',
    sql: '(SELECT ${columns})',
    sha: '76b2a4825f428087db08d884d901e1bff777ba8f237e13b36129cdd4d4fd8149',
    bytes: 19,
    binds: 0,
  },
  {
    name: 'record-scope',
    columns: '?::text id,?::text company_id',
    sql: '(SELECT ?::text id,?::text company_id)',
    sha: '3baa8f84f0626cf0e98f343fe9a62159b6b1ee24960a03f07731a090a3ec677e',
    bytes: 38,
    binds: 2,
  },
  {
    name: 'settlement-scope',
    columns: '?::text company_id',
    sql: '(SELECT ?::text company_id)',
    sha: 'ab105d6a6b61cd796b415ffda0fc43d9afac1afca484ab1443ca6084589cc4ef',
    bytes: 27,
    binds: 1,
  },
];

// imageChildFrame is the shared r15 subquery-frame renderer. Its declaration is
// unchanged by r23; its own r15 renderings must keep their exact pre-r23 bytes.
const imageChildDecl = "export const imageChildFrame = (aggregate: string, source: string, separator = ' ') =>\n  `(SELECT ${aggregate}${separator}${source})`;";
const imageOracle = [
  {
    name: 'r15-handoff',
    sql: imageChildFrame('to_jsonb(handoff)', 'FROM learning_eligibility_handoffs handoff WHERE handoff.assignment_id=assignment.id AND handoff.company_id=assignment.company_id'),
    sha: '96723712cd1c1e8f4cf9837ec01189ad79207bcc57f4846631e4f0172944c50a',
    bytes: 156,
  },
  {
    name: 'r15-certificates',
    sql: imageChildFrame("jsonb_agg(jsonb_build_object('certificate',to_jsonb(certificate),'revocation',to_jsonb(revocation)) ORDER BY certificate.id)", `FROM learning_certificates certificate LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=certificate.id
 WHERE certificate.assignment_id=assignment.id AND certificate.company_id=assignment.company_id`, '\n '),
    sha: '69f4c47bf432f2c2d1818acfb5bd27ca4714ac6d1149ac681af6c202354ebf48',
    bytes: 367,
  },
];

describe('Learning read-tail r23 scope-row frame fold', () => {
  test('every rendered scope row is byte-identical to its pre-r23 oracle', () => {
    expect(pre).toHaveLength(3);
    for (const p of pre) {
      const rendered = scopeRow(p.columns);
      expect(rendered).toBe(p.sql);
      expect(Buffer.byteLength(rendered)).toBe(p.bytes);
      expect(sha(rendered)).toBe(p.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(p.binds);
      readOnly(rendered);
    }
  });

  test('scopeRow is byte-identical to the r15 imageChildFrame head with empty source and separator', () => {
    for (const p of pre) {
      expect(scopeRow(p.columns)).toBe(imageChildFrame(p.columns, '', ''));
    }
  });

  test('scopeRow delegates through imageChildFrame; its inline SELECT is gone', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const scopeRow =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const imageChildFrame =/g) ?? []).toHaveLength(1);
    expect(shared).toContain("export const scopeRow = (columns: string) => imageChildFrame(columns, '', '');");
    const decl = shared.match(/export const scopeRow =[\s\S]*?;\n/)?.[0];
    expect(decl).toBeDefined();
    expect(decl!).not.toContain('SELECT');
    expect(decl!).not.toContain('(SELECT ${columns})');

    // Prove the live declaration actually routes through the injected renderer.
    const build = new Function(
      'imageChildFrame',
      `${decl!.replace('export ', '').replace('(columns: string)', '(columns)')}\nreturn scopeRow;`,
    ) as (frame: (aggregate: string, source: string, separator: string) => string) =>
      (columns: string) => string;
    const delegate = build((aggregate, source, separator) => `<${aggregate}|${source}|${separator}>`);
    expect(delegate('X')).toBe('<X||>');
  });

  test('the shared renderer is unchanged and its r15 renderings keep their bytes', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).toContain(imageChildDecl);
    for (const o of imageOracle) {
      expect(sha(o.sql)).toBe(o.sha);
      expect(Buffer.byteLength(o.sql)).toBe(o.bytes);
      readOnly(o.sql);
    }
  });

  test('all callers, binds, native sources and locks stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    expect((shared.match(/scopeRow\(/g) ?? []).length).toBeGreaterThanOrEqual(1);
    expect(shared).toContain('EXISTS(SELECT 1 FROM${spacing}${scopeRow(columns)}${spacing}learning_actor_scope');
    const records = src('record-purchases.ts');
    expect(records.match(/scopeRow\(/g) ?? []).toHaveLength(1);
    expect(records).not.toContain('(SELECT ?::text id,?::text company_id)');
    expect(records).toContain('.bind(...scope.args,id,actor.companyId)');
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/scopeRow\(/g) ?? []).toHaveLength(1);
    expect(obligations).not.toContain('(SELECT ?::text company_id)');
    expect(obligations).toContain('.bind(input.companyId)');

    const native = shared.match(/export const learningNativeSources=`([\s\S]*?)`;/)?.[1];
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

describe('Learning read-tail r23 invariants', () => {
  test('r1-r22 helpers stay defined exactly once', () => {
    const shared = src('learning-read-sql.ts');
    for (const name of [
      'actorScopeProbe', 'clockProbe', 'coursesImage', 'learningNativeSources', 'nativeSourceProbe',
      'overdueEvidenceColumns', 'learningModuleGrantProbe', 'moduleGrantAssignmentProbe', 'actorScopeExistsProbe',
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
    expect(selectCount(src('learning-read-sql.ts'))).toBe(30);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(selectCount(src('learning.ts'))).toBe(0);
  });
});
