import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  learningNativeSources,
  nativeSourceProbe,
  learningModuleGrantProbe,
} from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied verbatim from HEAD; used only as byte-identity oracles.
const preNativeSources = `SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'learn.courses' capability FROM learning_courses
 UNION ALL SELECT e.id,c.company_id,NULL::text,NULL::text,'learn.courses' FROM learning_course_events e JOIN learning_courses c ON c.id=e.course_id
 UNION ALL SELECT id,company_id,branch_id,department_id,'learn.assignments' FROM learning_assignments
 UNION ALL SELECT e.id,a.company_id,a.branch_id,a.department_id,CASE WHEN e.action IN ('complete','waive') THEN 'learn.results' ELSE 'learn.assignments' END FROM learning_assignment_events e JOIN learning_assignments a ON a.id=e.assignment_id
 UNION ALL SELECT c.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificates c JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT r.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_certificate_revocations r JOIN learning_certificates c ON c.id=r.certificate_id AND c.company_id=r.company_id JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 UNION ALL SELECT h.id,a.company_id,a.branch_id,a.department_id,'learn.certificates' FROM learning_eligibility_handoffs h JOIN learning_assignments a ON a.id=h.assignment_id AND a.company_id=h.company_id`;
const preOverdueColumns = 'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for';
const preGrantProbe = (alias: string) => `SELECT 1 FROM erpx_json_each(${alias}.modules_json) module WHERE module.value='learning'`;

describe('Learning read-tail r3 byte-identical / parameterized family dedupe', () => {
  test('native source union is moved verbatim to one owner source', () => {
    expect(learningNativeSources).toBe(preNativeSources);
    readOnly(learningNativeSources);
  });

  test('native source probe renders byte-identical SQL for every alias', () => {
    for (const alias of ['learning_source', 'actual_learning', 'native']) {
      expect(nativeSourceProbe(alias)).toBe(`SELECT 1 FROM (${preNativeSources}) ${alias}`);
      readOnly(nativeSourceProbe(alias));
    }
  });

  test('the three native probe call sites are composed, never inlined', () => {
    const text = src('record-purchases.ts');
    expect(text).not.toContain('SELECT 1 FROM (${nativeSources})');
    expect(text.match(/nativeSourceProbe\(/g) ?? []).toHaveLength(3);
    // r7 composed the actor-scope probe, r16 the scope row and r17 the last record probe.
    expect(text.match(/SELECT /g) ?? []).toHaveLength(0);
  });

  test('overdue evidence head is byte-identical to the native Rust owner', () => {
    expect(src('native_lane_31/postgres.rs')).toContain(preOverdueColumns);
    readOnly(preOverdueColumns);
    const search = src('executive-evidence-sources.ts');
    expect(search).not.toContain(preOverdueColumns);
    // Wave 2 moved the overdue search read to the native axum port as well.
    expect(search).not.toContain('${overdueEvidenceColumns}');
    expect(search).toContain('nativeLearningEvidenceSearch(');
    const detail = src('learning.ts');
    expect(detail).toContain('nativeLearningEvidenceRow(');
    expect(detail).not.toContain('${overdueEvidenceColumns}');
  });

  test('module-grant probe renders byte-identical SQL for both aliases', () => {
    for (const alias of ['learning_module', 'learning_module_grant']) {
      expect(learningModuleGrantProbe(alias)).toBe(preGrantProbe(alias));
      readOnly(learningModuleGrantProbe(alias));
    }
  });

  test('module-grant probe is defined once and used by both callers', () => {
    for (const name of ['purchases.ts', 'evaluation-source-policy.ts']) {
      const text = src(name);
      expect(text).not.toContain("SELECT 1 FROM erpx_json_each(");
      // r7 composes it through moduleGrantAssignmentProbe; no inline grant survives.
      expect(text).toContain('moduleGrantAssignmentProbe(');
    }
    const shared = src('learning-read-sql.ts');
    expect(shared.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for');
    expect(shared.match(/export const nativeSourceProbe =/g) ?? []).toHaveLength(1);
  });
});

describe('Learning overdue-evidence oracle migrated to the native owner', () => {
  test('the TypeScript constant is deleted and the Rust owner pins its bytes', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).not.toContain('overdueEvidenceColumns');
    expect((shared.match(/SELECT /g) ?? []).length).toBe(20);
    const rust = src('native_lane_31/postgres.rs');
    expect(rust).toContain('pub const OVERDUE_SQL: &str = concat!(');
    expect(rust).toContain(preOverdueColumns);
    expect(rust).toContain('overdue_evidence_oracle_migrated_from_typescript');
  });

  test('no runtime TypeScript source imports the retired constant', () => {
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) expect(readFileSync(path, 'utf8')).not.toContain('overdueEvidenceColumns');
      }
    };
    walk(srcRoot);
  });
});

describe('Learning native source family moved into the native owner contract', () => {
  const nativeText = src('learning-native-sources.json');
  const nativeContract = JSON.parse(nativeText) as {nativeSources: string; nativeSourceProbe: string};

  test('the contract carries the retired union bytes and no caller value', () => {
    expect(nativeContract.nativeSources).toBe(preNativeSources);
    expect(Buffer.byteLength(nativeText)).toBe(1397);
    expect(createHash('sha256').update(nativeText).digest('hex')).toBe('cdf39beb66d1b850542ae2d07a0987542a8346a97bf94a2669da5d2d1e21105b');
    expect((nativeContract.nativeSources.match(/SELECT /g) ?? []).length).toBe(7);
    expect((nativeContract.nativeSources.match(/UNION ALL SELECT /g) ?? []).length).toBe(6);
    expect(nativeContract.nativeSources).not.toContain('?');
    expect(nativeContract.nativeSourceProbe).toBe('SELECT 1 FROM ({nativeSources}) {alias}');
    readOnly(nativeContract.nativeSources);
  });

  test('the TypeScript adapter owns no statement byte and renders the contract', () => {
    const shared = src('learning-read-sql.ts');
    expect(shared).not.toContain('SELECT id,company_id,NULL::text branch_id');
    expect(shared).not.toContain('SELECT 1 FROM (${learningNativeSources})');
    expect(shared.match(/export const learningNativeSources=contract\.nativeSources;/g) ?? []).toHaveLength(1);
    expect(shared.match(/fill\(contract\.nativeSourceProbe, \{nativeSources: learningNativeSources, alias\}\)/g) ?? []).toHaveLength(1);
    for (const alias of ['learning_source', 'actual_learning', 'native']) {
      expect(nativeSourceProbe(alias)).toBe(`SELECT 1 FROM (${nativeContract.nativeSources}) ${alias}`);
      readOnly(nativeSourceProbe(alias));
    }
  });

  test('the native Rust owner binds and pins the migrated bytes', () => {
    const rust = src('learning_native_sources.rs');
    expect(rust).toContain('include_str!("learning-native-sources.json")');
    expect(rust).toContain('LEARNING_NATIVE_SOURCES_JSON');
    expect(rust).toContain('learning_native_source_union_sql');
    expect(rust).toContain('render_learning_native_source_probe');
    expect(rust).toContain('native_source_union_oracle_migrated_from_typescript');
    expect(rust).toContain('cdf39beb66d1b850542ae2d07a0987542a8346a97bf94a2669da5d2d1e21105b');
    expect(readFileSync(resolve(srcRoot, 'public', 'learning_native_sources.rs'), 'utf8')).toContain('learning_native_source_union_sql');
  });

  test('retirement:ts-statements KPI is 21 (native source family cut 29 -> 21)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBe(21);
    expect(selectCount(src('learning-read-sql.ts'))).toBe(20);
    expect(selectCount(src('executive-metric-projections.ts'))).toBe(1);
  });
});

describe('Learning read-tail r3 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r3 BEFORE=86 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(86);
  });
});
