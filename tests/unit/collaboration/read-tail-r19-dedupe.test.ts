import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialCommentAncestorStep } from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const socialHome = readFileSync(resolve(adapters, 'social-home.ts'), 'utf8');
// Byte-identical recursive-CTE ancestor step that appeared in both social-home.ts
// comment tree queries. Only whitespace between SQL tokens differs pre-dedupe.
const ANCESTOR_STEP = "SELECT parent.* FROM ancestors child JOIN social_comments parent ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id WHERE parent.state='published'";

describe('Collaboration read-tail r19 byte-identical ancestor-step dedupe', () => {
  test('socialCommentAncestorStep stays byte-identical to the pre-dedupe fragments', () => {
    expect(socialCommentAncestorStep).toBe(ANCESTOR_STEP);
    expect(socialCommentAncestorStep.split('SELECT ').length - 1).toBe(1);
  });

  test('the shared fragment is declared exactly once and the inline literals are gone', () => {
    expect(shared.match(/export const socialCommentAncestorStep=/g) ?? []).toHaveLength(1);
    // one import + two call sites
    expect(socialHome.split('socialCommentAncestorStep').length - 1).toBe(3);
    expect(socialHome).not.toContain('SELECT parent.* FROM ancestors child JOIN social_comments parent');
  });

  test('keeps the fragment read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    expect(socialCommentAncestorStep).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(socialHome).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });
});

describe('Collaboration read-tail r19 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r19 BEFORE=413 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r19 deduped the 2x byte-identical recursive ancestor-step fragment in social-home.ts.
    // r20 lowers the exact count further, so assert the r19 ceiling (as r17/r18 do)
    // instead of freezing a total that the next retirement slice must reduce.
    expect(total).toBeLessThan(413);
  });
});
