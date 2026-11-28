import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostDraftExists } from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const socialHome = readFileSync(resolve(adapters, 'social-home.ts'), 'utf8');
// Byte-identical draft-existence guards that appeared at three read sites in
// social-home.ts: the version guard constant, the edit-upload assertion and the
// comment-upload assertion. Only the trailing field predicate varies, so it is
// parameterized; the fixed prefix, bind order and lock semantics stay in place.
const DRAFT_VERSION = `EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND version=? AND published_post_id IS NULL)`;
const EDIT_UPLOAD = `EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND company_id=? AND edit_post_id=? AND edit_post_version=? AND edit_saved_version IS NULL AND cancelled_at IS NULL)`;
const COMMENT_UPLOAD = `EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND comment_parent_post_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL)`;
const INLINE = 'EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND author_id=? AND ';

describe('Collaboration read-tail r23 parameterized draft-existence family', () => {
  test('socialPostDraftExists stays byte-identical to all three pre-dedupe literals', () => {
    expect(socialPostDraftExists('version=? AND published_post_id IS NULL')).toBe(DRAFT_VERSION);
    expect(socialPostDraftExists('company_id=? AND edit_post_id=? AND edit_post_version=? AND edit_saved_version IS NULL AND cancelled_at IS NULL')).toBe(EDIT_UPLOAD);
    expect(socialPostDraftExists('comment_parent_post_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL')).toBe(COMMENT_UPLOAD);
    // The helper contributes exactly one SELECT; the varying tail adds none.
    expect(DRAFT_VERSION.split('SELECT ').length - 1).toBe(1);
    expect(socialPostDraftExists('version=? AND published_post_id IS NULL').split('SELECT ').length - 1).toBe(1);
  });

  test('the shared fragment is declared once, all inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialPostDraftExists=/g) ?? []).toHaveLength(1);
    // one import + three call sites in the consumer file
    expect(socialHome.split('socialPostDraftExists').length - 1).toBe(4);
    expect(socialHome).not.toContain(INLINE);
    expect(socialHome).toContain("socialPostDraftExists('version=? AND published_post_id IS NULL')");
    expect(socialHome).toContain("socialPostDraftExists('company_id=? AND edit_post_id=? AND edit_post_version=? AND edit_saved_version IS NULL AND cancelled_at IS NULL')");
    expect(socialHome).toContain("socialPostDraftExists('comment_parent_post_id=? AND published_comment_id IS NULL AND cancelled_at IS NULL')");
  });

  test('keeps the emitted SQL read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const sql of [DRAFT_VERSION, EDIT_UPLOAD, COMMENT_UPLOAD]) expect(sql).not.toMatch(lockRe);
    for (const line of socialHome.split('\n').filter(l => l.includes('socialPostDraftExists(') && !l.includes('import'))) {
      expect(line).not.toMatch(lockRe);
    }
  });

  test('bind order at the three former inline call sites is unchanged', () => {
    expect(socialHome).toContain('[id, userId, expectedVersion]');
    expect(socialHome).toContain('[id, userId, version]');
    expect(socialHome).toContain('[editDraftId, userId, companyId, id, version]');
    expect(socialHome).toContain('[commentDraftId, userId, id]');
  });
});

describe('Collaboration read-tail r23 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=409 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r23 deduped the 3x byte-identical draft-existence prefix in social-home.ts
    // into one shared parameterized fragment in read-tail-sql.ts.
    expect(total).toBeLessThanOrEqual(403);
    expect(total).toBeLessThan(409);
  });
});
