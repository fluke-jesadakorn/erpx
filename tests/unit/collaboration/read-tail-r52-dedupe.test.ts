import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialCommentExistsGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const rendererLine = shared.split('\n').find(l => l.startsWith('export const socialCommentExistsGuard='))!;

// Exact pre-r52 rendered counts frozen from the live HEAD before the fold (SHA,
// byte length, bind count, SELECT count). Both guard assertions shared the
// byte-identical head `EXISTS(SELECT 1 FROM social_comments `; only the WHERE
// tail varied. The r52 fold routes both through one owner-local renderer; the
// nested agent-reply guard in the removal tail, tails, binds, callers and locks
// stay fixed. The inbox whole-file pin, the agent-reply lock/finalize statement
// and the notification files are deliberately not part of this family.
const HEAD = 'EXISTS(SELECT 1 FROM social_comments ';
const AUTHOR_TAIL = `WHERE id=? AND post_id=? AND author_id=? AND author_kind='person' AND state='published' AND version=?)`;
const REMOVAL_TAIL = `c WHERE c.id=? AND c.post_id=? AND c.state='published' AND c.version=? AND (c.author_id=? OR EXISTS(SELECT 1 FROM social_agent_replies r WHERE r.post_id=c.post_id AND r.actor_id=? AND ('reply-' || r.id)=c.id AND r.state='completed')))`;

type Site = {
  name: string;
  tail: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
  selects: number;
};
const SITES: Site[] = [
  {
    name: 'comment author guard (4 binds, 1 SELECT)',
    tail: AUTHOR_TAIL,
    sql: `${HEAD}${AUTHOR_TAIL}`,
    sqlSha: '9b27af458de0e9cba25e6d96aab8691f4ae9f647d441f9d67f4e26db4ae4c458',
    bytes: 139,
    binds: 4,
    selects: 1,
  },
  {
    name: 'comment removal guard (5 binds, nested agent-reply SELECT retained)',
    tail: REMOVAL_TAIL,
    sql: `${HEAD}${REMOVAL_TAIL}`,
    sqlSha: '1924ecb33c99d2a29853f8af0097b7290e0a3545a161c982990e2f110436b86a',
    bytes: 271,
    binds: 5,
    selects: 2,
  },
];

const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i;

describe('Collaboration read-tail r52 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(sha(socialCommentExistsGuard(site.tail))).toBe(site.sqlSha);
      expect(socialCommentExistsGuard(site.tail)).toBe(site.sql);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect((site.sql.match(/SELECT /g) ?? []).length).toBe(site.selects);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.sql).not.toMatch(lockRe);
    });
  }

  test('the two tails are distinct, so the head is the only shared bytes', () => {
    expect(Buffer.byteLength(HEAD)).toBe(37);
    expect(sha(HEAD)).toBe('39572ca6f80641efca73d73422fa0d1218d7d1879adf3ac0fc0e15d3dffae38d');
    expect(AUTHOR_TAIL).not.toBe(REMOVAL_TAIL);
    expect(socialCommentExistsGuard(AUTHOR_TAIL).slice(HEAD.length)).toBe(AUTHOR_TAIL);
    expect(socialCommentExistsGuard(REMOVAL_TAIL).slice(HEAD.length)).toBe(REMOVAL_TAIL);
    expect(REMOVAL_TAIL).toContain("EXISTS(SELECT 1 FROM social_agent_replies r");
  });
});

describe('Collaboration read-tail r52 renderer fold', () => {
  test('one renderer owns the single head and both guard sites route through it', () => {
    expect(shared.match(/export const socialCommentExistsGuard=/g) ?? []).toHaveLength(1);
    expect(sha(rendererLine)).toBe('9a690c9f689c817e2ab3fa95a594654954465cfe9e89fac42c4ef6c1fdc98893');
    expect(rendererLine).toContain('EXISTS(SELECT 1 FROM social_comments ${tail}');
    expect((rendererLine.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendererLine).not.toMatch(lockRe);

    const home = src('social-home.ts');
    expect(home).toContain('socialCommentExistsGuard,socialPostDraftExists');
    expect((home.match(/socialCommentExistsGuard\(/g) ?? []).length).toBe(2);
    expect(home).toContain(`socialCommentExistsGuard(${JSON.stringify(AUTHOR_TAIL)})`);
    expect(home).toContain(`socialCommentExistsGuard(${JSON.stringify(REMOVAL_TAIL)})`);
    expect(home).not.toContain('"EXISTS(SELECT 1 FROM social_comments WHERE');
    expect(home).not.toContain('"EXISTS(SELECT 1 FROM social_comments c WHERE');
  });
});

describe('Collaboration read-tail r52 excluded SQL stays untouched', () => {
  test('inbox whole-file pins, agent-reply locks/finalize and notifications are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('cde03a2e7d9b86710255c0f4411a299b141c91ce00169f79428cfbf5e110116b');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    const purchases = src('work-social-purchases.ts');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-agent.ts')).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
  });
});

describe('Collaboration read-tail r52 KPI pins', () => {
  test('retirement:ts-statements is pinned at 316, below the r52 BEFORE=331 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical guard heads -> one renderer: 331 -> 330.
    // r53 folded the comment-aggregate head family: 330 -> 329.
    expect(total).toBe(316);
    expect(total).toBeLessThan(331);
  });
});
