import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  assignmentCourseReadHead,
  identitySourceRead,
  keyedRowRead,
  sourceLiteral,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Pre-r24 rendered source literals, captured verbatim at the task-start source
// (learning-read-sql.ts blob 4776d9adc8d479a1b21663ea153513dd0cfa2bee).
// Byte-identity oracles only; never live query sources. Binds stay with callers.
const pre = [
  {
    name: 'handoffs',
    table: 'learning_eligibility_handoffs',
    sql: 'SELECT * FROM learning_eligibility_handoffs',
    sha: '1aa23aea7b8a4ab6b91bb30373bedbd0c1983aede8615c3a8daf5a4255a400cc',
    bytes: 43,
    binds: 0,
  },
  {
    name: 'certificates',
    table: 'learning_certificates',
    sql: 'SELECT * FROM learning_certificates',
    sha: '582fb4afb4b94f5d0b075cd3e19a021d1a9f1c3a808b868bb4dc259eef07eb09',
    bytes: 35,
    binds: 0,
  },
  {
    name: 'assignments',
    table: 'learning_assignments',
    sql: 'SELECT * FROM learning_assignments',
    sha: 'dcd81a487b1d08ce75b4b411493eb411ed635478a7ecb6efa03d6915e137b1ce',
    bytes: 34,
    binds: 0,
  },
  {
    name: 'arbitrary',
    table: 'learning_some_table',
    sql: 'SELECT * FROM learning_some_table',
    sha: 'dd1fe39da571deee215d0d941aaa0440fc48335dd1b65eaa7a0336ed4742b4a8',
    bytes: 33,
    binds: 0,
  },
];

// keyedRowRead is the shared r17 single-row read renderer. Its declaration is
// unchanged by r24; its own r17 renderings must keep their exact pre-r24 bytes.
const keyedRowDecl = "export const keyedRowRead = (projection: string, frame: string, where = '', tail = '') =>\n  `SELECT ${projection}${frame}${where}${tail}`;";
const keyedOracle = [
  {
    name: 'r17-from',
    sql: keyedRowRead('a.*', ' FROM learning_assignments a'),
    sha: '9e0680eaa2e0fa3b51081ac5243d887ecc4f727e52f9ea037871de1d4cd93177',
    bytes: 38,
  },
  {
    name: 'r17-where',
    sql: keyedRowRead('a.id', ' FROM assignments a', ' WHERE a.id=?', ''),
    sha: '75ae7333e18342077bf797bbbdd8757f3cfa5e989d4136eae331e74644c659c7',
    bytes: 43,
  },
  {
    name: 'r17-tail',
    sql: keyedRowRead('1', ' FROM t', '', ' ORDER BY 1'),
    sha: '60a7bd5094c130a15a9031c7d32c261d1d0ed0ed5a44072146ef59a2e9659e71',
    bytes: 26,
  },
  {
    name: 'r17-empty',
    sql: keyedRowRead('*', '', '', ''),
    sha: '9e43d568486a92e96f6d486821183a032f4327247853e44f1fa6679714fdb47b',
    bytes: 8,
  },
];

describe('Learning read-tail r24 source-literal fold', () => {
  test('every rendered source literal is byte-identical to its pre-r24 oracle', () => {
    expect(pre).toHaveLength(4);
    for (const p of pre) {
      const rendered = sourceLiteral(p.table);
      expect(rendered).toBe(p.sql);
      expect(Buffer.byteLength(rendered)).toBe(p.bytes);
      expect(sha(rendered)).toBe(p.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(p.binds);
      readOnly(rendered);
    }
  });

  test('sourceLiteral is byte-identical to the r17 keyedRowRead head with a "*" projection and FROM frame', () => {
    for (const p of pre) {
      expect(sourceLiteral(p.table)).toBe(keyedRowRead('*', ` FROM ${p.table}`));
    }
  });

  test('sourceLiteral delegates through keyedRowRead; its inline SELECT is gone', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const sourceLiteral =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const keyedRowRead =/g) ?? []).toHaveLength(1);
    expect(shared).toContain("export const sourceLiteral = (table: string) => keyedRowRead('*', ` FROM ${table}`);");
    const decl = shared.match(/export const sourceLiteral =[\s\S]*?;\n/)?.[0];
    expect(decl).toBeDefined();
    expect(decl!).not.toContain('SELECT');
    expect(decl!).not.toContain('`SELECT * FROM ${table}`');

    // Prove the live declaration actually routes through the injected renderer.
    const build = new Function(
      'keyedRowRead',
      `${decl!.replace('export ', '').replace('(table: string)', '(table)')}\nreturn sourceLiteral;`,
    ) as (renderer: (projection: string, frame: string, where: string, tail: string) => string) =>
      (table: string) => string;
    const delegate = build((projection, frame, where = '', tail = '') => `<${projection}|${frame}|${where}|${tail}>`);
    expect(delegate('learning_certificates')).toBe('<*| FROM learning_certificates||>');
  });

  test('the shared renderer is unchanged and its r17 renderings keep their bytes', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).toContain(keyedRowDecl);
    for (const o of keyedOracle) {
      expect(sha(o.sql)).toBe(o.sha);
      expect(Buffer.byteLength(o.sql)).toBe(o.bytes);
      readOnly(o.sql);
    }
  });

  test('all callers, binds, native sources and locks stay untouched', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const sourceLiteral =/g) ?? []).toHaveLength(1);
    const obligations = src('package-obligations.ts');
    expect(obligations).toContain("handoffs:sourceLiteral('learning_eligibility_handoffs')");
    expect(obligations).toContain("certificates:sourceLiteral('learning_certificates')");
    expect(obligations).not.toContain('SELECT * FROM learning_eligibility_handoffs');
    expect(obligations).not.toContain('SELECT * FROM learning_certificates');
    const metrics = src('metric-purchases.ts');
    expect(metrics).toContain("sql:sourceLiteral('learning_assignments')");
    expect(metrics).not.toContain('SELECT * FROM learning_assignments');

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

describe('Learning read-tail r24 invariants', () => {
  test('r1-r23 helpers stay defined exactly once', () => {
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

  test('retirement:ts-statements KPI is 20 (strictly below the r24 BEFORE=33 baseline)', () => {
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
    expect(total).toBe(20);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(19);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    expect(selectCount(src('learning.ts'))).toBe(0);
  });
});

// r25 fold: assignmentCourseReadHead is exactly the r13 identitySourceRead head
// with the assignments x courses source and key 'a.'. The rendered bytes and
// binds are frozen here; the single query head now lives in identitySourceRead.
const ASSIGNMENT_COURSE_SOURCE = 'learning_assignments a JOIN learning_courses c ON c.id=a.course_id';
const pre25 = [
  {
    name: 'capability-witness',
    projection: 'c.required_for',
    separator: ' ',
    sql: 'SELECT c.required_for FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?',
    sha: 'd8f474a0375baffd74344f39e91672669427f28127b2f02a622bff0c56df51ae',
    bytes: 125,
    binds: 2,
  },
  {
    name: 'assignment-witness',
    projection: `a.course_id,a.course_version,c.company_id course_company,c.required_for,
  (SELECT jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id) FROM hr_people p WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) people,
  CASE WHEN TRUE THEN 1 ELSE 0 END learner_eligible`,
    separator: '\n ',
    sql: `SELECT a.course_id,a.course_version,c.company_id course_company,c.required_for,
  (SELECT jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id) FROM hr_people p WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) people,
  CASE WHEN TRUE THEN 1 ELSE 0 END learner_eligible
 FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?`,
    sha: '060bb55bf636b223bd9444cea800fc9d39a18caa644913133e2d28e0c99e54c6',
    bytes: 381,
    binds: 2,
  },
];

describe('Learning read-tail r25 assignment-course head fold', () => {
  test('every rendered head is byte-identical to its pre-r25 oracle', () => {
    expect(pre25).toHaveLength(2);
    for (const p of pre25) {
      const rendered = assignmentCourseReadHead(p.projection, p.separator);
      expect(rendered).toBe(p.sql);
      expect(Buffer.byteLength(rendered)).toBe(p.bytes);
      expect(sha(rendered)).toBe(p.sha);
      expect((rendered.match(/\?/g) ?? []).length).toBe(p.binds);
      readOnly(rendered);
    }
  });

  test('assignmentCourseReadHead delegates through the owner-local identitySourceRead head', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const assignmentCourseReadHead =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const identitySourceRead =/g) ?? []).toHaveLength(1);
    const decl = shared.match(/export const assignmentCourseReadHead =[\s\S]*?;\n/)?.[0];
    expect(decl).toBeDefined();
    expect(decl!).not.toContain('SELECT');
    expect(decl!).toContain('identitySourceRead(projection,');
    expect(decl!).toContain(`'${ASSIGNMENT_COURSE_SOURCE}'`);
    expect(decl!).toContain("separator, 'a.')");

    const build = new Function(
      'identitySourceRead',
      `${decl!.replace('export ', '').replace('(projection: string, separator = ', '(projection, separator = ')}\nreturn assignmentCourseReadHead;`,
    ) as (renderer: (projection: string, source: string, separator: string, key: string) => string) =>
      (projection: string, separator: string) => string;
    const delegate = build((projection, source, separator, key) =>
      `<${projection}|${source}|${separator}|${key}>`,
    );
    expect(delegate('c.required_for', ' ')).toBe(
      `<c.required_for|${ASSIGNMENT_COURSE_SOURCE}| |a.>`,
    );
  });

  test('the shared renderer is unchanged and the r20 course key stays byte-frozen', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).toContain(
      "export const identitySourceRead = (projection: string, source: string, separator = ' ', key = '') =>\n  `SELECT ${projection}${separator}FROM ${source} WHERE ${key}id=? AND ${key}company_id=?`;",
    );
    for (const p of pre25) {
      expect(assignmentCourseReadHead(p.projection, p.separator)).toBe(
        identitySourceRead(p.projection, ASSIGNMENT_COURSE_SOURCE, p.separator, 'a.'),
      );
    }
    const course = identitySourceRead('to_jsonb(c) image', 'learning_courses c', ' ', 'c.');
    expect(course).toBe('SELECT to_jsonb(c) image FROM learning_courses c WHERE c.id=? AND c.company_id=?');
    readOnly(course);
  });

  test('call sites, binds, native sources and locks stay untouched', () => {
    const learning = src('learning.ts');
    expect((learning.match(/assignmentCourseReadHead\(/g) ?? []).length).toBe(2); // 2 call sites
    expect(learning).toContain('.bind(id,actor.companyId)');
    expect(learning).toContain('.bind(id,a.companyId)');
    expect(learning).not.toContain('FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=?');

    const native = (JSON.parse(src('learning-native-sources.json')) as { nativeSources: string }).nativeSources;
    expect((native.match(/SELECT /g) ?? []).length).toBe(7);
    expect((native.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);

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
