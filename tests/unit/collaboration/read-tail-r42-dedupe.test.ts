import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialCommentAncestorStep, socialCommentHead, socialCommentProjection } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const home = src('social-home.ts');
const purchases = src('work-social-purchases.ts');

// Exact pre-r42 column lists and rendered fragments from HEAD. The comments
// CTE and comments-page reader share the 138-byte `SELECT <cols>,` head; the
// two UNION arms share the unqualified projection. Tails, tables, binds,
// locks and notification targets were never changed.
const COLS = 'id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id';
const COLSQ = 'c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id';
const HEAD = 'SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,';
const SELECTED = 'SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM selected';
const ANCESTORS = 'SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM ancestors';
const COMMENTS = `WITH RECURSIVE ranked AS (
        SELECT c.*,row_number() OVER(PARTITION BY c.post_id ORDER BY c.created_at DESC,c.id DESC) ordinal
        FROM social_comments c WHERE c.post_id IN (?,?) AND c.state='published'
      ), selected AS (
        SELECT * FROM ranked WHERE ordinal<=?
      ), ancestors AS (
        SELECT parent.* FROM selected child JOIN social_comments parent
          ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
        WHERE parent.state='published'
        UNION
        ${socialCommentAncestorStep}
      ), thread_comments AS (
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM selected
        UNION
        SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM ancestors
      )
      SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,
        parent.author_name AS parent_author_name FROM thread_comments c
      LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id
      ORDER BY c.created_at,c.id`;
const PAGE = "SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name,c.created_at::text AS page_key FROM social_comments c LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id WHERE c.post_id=? AND c.state='published'";

// Re-resolve the source's own interpolations with the production renderers,
// so the byte oracle covers the rendered SQL and not just the helper output.
const resolveInterpolations = (body: string) => {
  const substitutions: Array<[string, string]> = [
    ["${allowed.map(() => '?').join(',')}", '?,?'],
    ['${socialCommentAncestorStep}', socialCommentAncestorStep],
    ["${socialCommentProjection(commentCols,'selected')}", socialCommentProjection(COLS, 'selected')],
    ["${socialCommentProjection(commentCols,'ancestors')}", socialCommentProjection(COLS, 'ancestors')],
    ['${socialCommentHead(commentColsQ)}', socialCommentHead(COLSQ)],
  ];
  for (const [from, to] of substitutions) body = body.split(from).join(to);
  return body;
};

describe('Collaboration read-tail r42 comment head family', () => {
  test('one shared 138-byte head reproduces both retired column heads', () => {
    expect(HEAD).toHaveLength(138);
    expect(socialCommentHead(COLSQ)).toBe(HEAD);
    expect(sha(socialCommentHead(COLSQ))).toBe('a7dd1818c537288f70fbf6c424c84166fb5a1134106b279f6d7f7d5205f3d35a');
    expect(socialCommentHead(COLSQ).match(/SELECT /g) ?? []).toHaveLength(1);
    expect(socialCommentHead(COLSQ)).not.toContain('?');
    expect(shared.match(/export const socialCommentHead=/g) ?? []).toHaveLength(1);
    expect(shared).not.toContain('export const socialCommentHeadSql=');
    expect(sha(line(shared, 'export const socialCommentHead='))).toBe('2ad63e64db338d62db117ee74155b7ea3281d741f48b4b6811ba884cc24b0d92');
  });

  test('both call sites keep their own tails and tables', () => {
    expect(home.match(/\$\{socialCommentHead\(commentColsQ\)\}/g) ?? []).toHaveLength(2);
    expect(sha(line(home, '      ${socialCommentHead(commentColsQ)}'))).toBe('944addbb198bb9f976947f2d9579ef71e9175aeee7ad412ca01f62e078eeb4a1');
    expect(sha(line(home, '            sql: `'))).toBe('db5f2eaae6d98b8420c2707f635676bf1e5c5c998ac3ca5a313d6c879178bc9d');
    expect(home).toContain("author_name AS parent_author_name FROM thread_comments c");
    expect(home).toContain("c.created_at::text AS page_key FROM social_comments c LEFT JOIN social_comments parent");
    expect(home).not.toContain('"SELECT c.id,c.post_id');
    expect(sha(line(home, 'const commentColsQ='))).toBe('257a708a35255bef9eb3b5ecca4c33251d40cd2512da940429b8942c57e8dfbe');
    expect(sha(line(home, 'const commentCols='))).toBe('564279a73da9af68c85f0a2d813ab4cb94b7cb6fb270356f1dd84a0c4609919d');
  });
});

describe('Collaboration read-tail r42 UNION projection family', () => {
  test('one closed renderer reproduces both UNION arms byte-identically', () => {
    expect(socialCommentProjection(COLS, 'selected')).toBe(SELECTED);
    expect(socialCommentProjection(COLS, 'ancestors')).toBe(ANCESTORS);
    expect(socialCommentProjection(COLS, 'selected').match(/SELECT /g) ?? []).toHaveLength(1);
    expect(socialCommentProjection(COLS, 'ancestors').match(/SELECT /g) ?? []).toHaveLength(1);
    expect(shared).toContain("table:'selected'|'ancestors'");
    expect(shared.match(/export const socialCommentProjection=/g) ?? []).toHaveLength(1);
    expect(sha(line(shared, 'export const socialCommentProjection='))).toBe('e81afcc981c3c2bf41540a33fba9728e1870f57ec62b9fc4ffc2bb6771cc65c4');
  });

  test('both arms pass the same column list and their closed source tables', () => {
    expect(home).toContain("${socialCommentProjection(commentCols,'selected')}");
    expect(home).toContain("${socialCommentProjection(commentCols,'ancestors')}");
    expect(home.match(/socialCommentProjection\(commentCols,/g) ?? []).toHaveLength(2);
    expect(sha(line(home, "        ${socialCommentProjection(commentCols,'selected')}"))).toBe('0ef8400e3deece6c3d95cbd516d60f3fac5229087b6c9b7b8adec6bbc9da4349');
    expect(sha(line(home, "        ${socialCommentProjection(commentCols,'ancestors')}"))).toBe('59ab42508a920bd61d20e89598e63d63532151f37b3bf3df34e1b2e9f199bb20');
    expect(home).not.toContain(`SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM selected`);
    expect(home).not.toContain(`SELECT id,post_id,body,author_id,author_name,created_at,mentions_json,rich_content_json,author_kind,parent_comment_id FROM ancestors`);
  });
});

describe('Collaboration read-tail r42 byte oracles', () => {
  test('the rendered comments CTE is byte-identical to pre-r42 HEAD', () => {
    const start = home.indexOf('db.prepare(`WITH RECURSIVE ranked AS (');
    const endMark = 'ORDER BY c.created_at,c.id`)';
    const end = home.indexOf(endMark, start);
    const body = home.slice(start + 'db.prepare(`'.length, end + 'ORDER BY c.created_at,c.id'.length);
    const rendered = resolveInterpolations(body);
    expect(rendered).toBe(COMMENTS);
    expect(sha(rendered)).toBe('cf28ef9f83d242918b941b1ff5d7ee4413dca1170c293247a7faf12d2cc3efbb');
    expect(rendered).toContain(HEAD);
    expect(rendered).toContain(SELECTED);
    expect(rendered).toContain(ANCESTORS);
  });

  test('the rendered comments-page SQL is byte-identical to pre-r42 HEAD', () => {
    const pageLine = line(home, '            sql: `');
    const body = pageLine.slice(pageLine.indexOf('`') + 1, pageLine.lastIndexOf('`'));
    const rendered = resolveInterpolations(body);
    expect(rendered).toBe(PAGE);
    expect(sha(rendered)).toBe('61e2649d02b28b587ac96b1c0e2e7a9b2f3cd40ae78606cc748478ac19249031');
    expect(rendered.startsWith(HEAD)).toBe(true);
    expect(rendered.split('?').length - 1).toBe(1);
  });
});

describe('Collaboration read-tail r42 protected SQL isolation', () => {
  test('new renderers add no lock, write or notification statement', () => {
    for (const marker of ['export const socialCommentHead=', 'export const socialCommentProjection=']) {
      const added = line(shared, marker);
      expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
    }
  });

  test('agent-replies lock pair, notification targets and locks stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Collaboration read-tail r42 KPI pins', () => {
  test('retirement:ts-statements is pinned at 297, below the r43 BEFORE=350 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two 2->1 comment-projection retirements, one SELECT head each: 352 -> 350.
    // Prior exact pins were refreshed only where 350 replaced 352.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(297);
    expect(total).toBeLessThan(350);
  });
});
