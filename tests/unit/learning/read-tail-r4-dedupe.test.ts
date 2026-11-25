import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { completionEvidenceColumns } from '../../../packages/server-learning/src/adapters/learning-completion-evidence';
import { overdueEvidenceColumns } from '../../../packages/server-learning/src/adapters/learning-read-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literal copied verbatim from HEAD (canonical one-line form); used
// only as a byte-identity oracle, never as a live query source.
const preCompletionColumns = 'SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title';

describe('Learning read-tail r4 byte-identical completion-evidence family dedupe', () => {
  test('completion evidence head is moved verbatim to one owner source', () => {
    expect(completionEvidenceColumns).toBe(preCompletionColumns);
    readOnly(completionEvidenceColumns);
  });

  test('the head is distinct from the r3 overdue head and r3 helpers stay put', () => {
    expect(completionEvidenceColumns).not.toBe(overdueEvidenceColumns);
    const r3 = src('learning-read-sql.ts');
    expect(r3.match(/export const overdueEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(r3.match(/export const learningNativeSources=/g) ?? []).toHaveLength(1);
    expect(r3.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
  });

  test('both native evidence call sites are composed, never inlined', () => {
    const search = src('executive-evidence-sources.ts');
    expect(search).not.toContain(preCompletionColumns);
    // Wave 2 moved the completion search read to the native axum port as well.
    expect(search).not.toContain('${completionEvidenceColumns}');
    expect(search).not.toContain('learning-completion-evidence');
    expect(search).toContain('nativeLearningEvidenceSearch(');
    const detail = src('learning.ts');
    expect(detail).toContain('nativeLearningEvidenceRow(');
    expect(detail).not.toContain('${completionEvidenceColumns}');
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
  });

  test('no write or lock text entered the shared source', () => {
    readOnly(src('learning-completion-evidence.ts'));
  });
});

describe('Learning read-tail r4 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r4 BEFORE=82 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(82);
  });
});

// Pre-dedupe assignment-child literals copied verbatim from the r4 baseline;
// byte-identity oracles only, never live query sources.
const preChildImage = [
  `(SELECT jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id) \${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) people,`,
  `(SELECT jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id) \${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} \${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) engagements,`,
  `(SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id) FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id) handoffs,`,
  `(SELECT jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id) FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id) certificates,`,
];

describe('Learning read-tail r5 parameterized assignment-child family dedupe', () => {
  test('one parameterized source replaces the four inlined child-image statements', () => {
    const text = src('learning.ts');
    for (const literal of preChildImage) expect(text).not.toContain(literal);
    // r19 delegates the r5 child frame to imageChildFrame; the rendered bytes are unchanged.
    expect(text).toContain('const assignmentChildImage = (aggregate: string, source: string, label: string) => `${imageChildFrame(aggregate, source)} ${label}`;');
    expect(text.match(/assignmentChildImage\(/g) ?? []).toHaveLength(4);
  });

  test('r3/r4 helper files and every lock statement stay untouched', () => {
    const r3 = src('learning-read-sql.ts');
    expect(r3.match(/export const learningNativeSources=/g) ?? []).toHaveLength(1);
    expect(r3.match(/export const overdueEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(r3.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
    expect(src('learning-completion-evidence.ts').match(/export const completionEvidenceColumns =/g) ?? []).toHaveLength(1);
    expect(src('learning.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
  });

  test('retirement:ts-statements stays strictly below the r5 BEFORE=81 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(81);
  });
});
