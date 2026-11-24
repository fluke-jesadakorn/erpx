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
    for (const name of ['learning.ts', 'executive-evidence-sources.ts']) {
      const text = src(name);
      expect(text).not.toContain(preCompletionColumns);
      expect(text).toContain('${completionEvidenceColumns}');
      expect(text).toContain('learning-completion-evidence');
    }
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
