import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostDraftFieldGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const home = readFileSync(resolve(adapters, 'social-home.ts'), 'utf8');
// Byte-identical Social draft-field existence guards that appeared at two
// read sites in social-home.ts: the comment-draft and rich-draft readers.
// Only the trailing field predicate varies, so it is parameterized; the fixed
// `id=?` prefix, bind order and lock semantics stay in place.
const COMMENT = 'SELECT 1 FROM social_post_drafts WHERE id=? AND comment_parent_post_id IS NOT NULL';
const RICH = 'SELECT 1 FROM social_post_drafts WHERE id=? AND rich_content_json IS NOT NULL';
const INLINE = 'SELECT 1 FROM social_post_drafts WHERE id=? AND ';

describe('Collaboration read-tail r25 parameterized draft-field guard family', () => {
  test('socialPostDraftFieldGuard stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostDraftFieldGuard('comment_parent_post_id IS NOT NULL')).toBe(COMMENT);
    expect(socialPostDraftFieldGuard('rich_content_json IS NOT NULL')).toBe(RICH);
    // The helper contributes exactly one SELECT; the varying tail adds none.
    expect(COMMENT.split('SELECT ').length - 1).toBe(1);
    expect(socialPostDraftFieldGuard('rich_content_json IS NOT NULL').split('SELECT ').length - 1).toBe(1);
  });

  test('the shared fragment is declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialPostDraftFieldGuard=/g) ?? []).toHaveLength(1);
    expect(home).toContain("socialPostDraftFieldGuard('comment_parent_post_id IS NOT NULL')");
    expect(home).toContain("socialPostDraftFieldGuard('rich_content_json IS NOT NULL')");
    expect(home).not.toContain(INLINE);
    // one import + one call per former inline reader
    expect(home.split('socialPostDraftFieldGuard').length - 1).toBe(3);
  });

  test('keeps the emitted SQL read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const sql of [COMMENT, RICH]) expect(sql).not.toMatch(lockRe);
    for (const line of home.split('\n').filter(l => l.includes('socialPostDraftFieldGuard(') && !l.includes('import'))) {
      expect(line).not.toMatch(lockRe);
    }
  });

  test('bind order at the two former inline call sites is unchanged', () => {
    expect(home).toContain("socialPostDraftFieldGuard('comment_parent_post_id IS NOT NULL')).bind(id).first()");
    expect(home).toContain("socialPostDraftFieldGuard('rich_content_json IS NOT NULL')).bind(id).first()");
  });
});

describe('Collaboration read-tail r25 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=406 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r25 deduped the 2x byte-identical draft-field guard prefix into one shared
    // parameterized fragment in read-tail-sql.ts.
    expect(total).toBe(405);
    expect(total).toBeLessThan(406);
  });
});
