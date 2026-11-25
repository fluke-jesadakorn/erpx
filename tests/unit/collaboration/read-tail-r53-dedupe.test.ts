import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { workSocialCommentsAgg } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const rendererLine = shared.split('\n').find(l => l.startsWith('export const workSocialCommentsAgg='))!;

// Exact pre-r53 rendered counts frozen from the live HEAD before the fold (SHA,
// byte length, bind count, SELECT count). Three sites shared the byte-identical
// head `(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c `;
// only the closed JOIN/WHERE tail varied. Two sites already used the constant
// renderer; the third stayed inline at the Work social purchase image. The r53
// fold parameterizes that renderer by tail and routes all three through it. The
// tails, binds (the `WHERE` tail keeps its single `c.post_id=?` bind), callers,
// operation literals and locks stay fixed. The inbox whole-file pin, the
// agent-reply lock/claim/finalize statements and the notification files are
// deliberately not part of this family.
const HEAD = '(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c ';
const JOIN_TAIL = 'JOIN posts p ON p.id=c.post_id';
const WHERE_TAIL = 'WHERE c.post_id=?';

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
    name: 'purchase-source comments image (0 binds, 1 SELECT)',
    tail: JOIN_TAIL,
    sql: `${HEAD}${JOIN_TAIL})`,
    sqlSha: '564b83cde598fed9d85775602153f0bcadaf79c08e06874c7f3b6bfc1b66bfa8',
    bytes: 99,
    binds: 0,
    selects: 1,
  },
  {
    name: 'work social purchase image (1 bind, 1 SELECT)',
    tail: WHERE_TAIL,
    sql: `${HEAD}${WHERE_TAIL})`,
    sqlSha: 'a277953e9fc8bf07ebb17b704464f77720fd99d098818970bfe8e46fabd6b35b',
    bytes: 86,
    binds: 1,
    selects: 1,
  },
];

const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i;

describe('Collaboration read-tail r53 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(sha(workSocialCommentsAgg(site.tail))).toBe(site.sqlSha);
      expect(workSocialCommentsAgg(site.tail)).toBe(site.sql);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect((site.sql.match(/SELECT /g) ?? []).length).toBe(site.selects);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.sql).not.toMatch(lockRe);
    });
  }

  test('the two tails are distinct, so the head is the only shared bytes', () => {
    expect(Buffer.byteLength(HEAD)).toBe(68);
    expect(sha(HEAD)).toBe('af49c3a57f3e6964ce1e4db55257ad248a9453b92cc831d788b4e7fdd93cd0ed');
    expect(JOIN_TAIL).not.toBe(WHERE_TAIL);
    expect(workSocialCommentsAgg(JOIN_TAIL).slice(HEAD.length)).toBe(`${JOIN_TAIL})`);
    expect(workSocialCommentsAgg(WHERE_TAIL).slice(HEAD.length)).toBe(`${WHERE_TAIL})`);
  });
});

describe('Collaboration read-tail r53 renderer fold', () => {
  test('one renderer owns the single head and all three sites route through it', () => {
    expect(shared.match(/export const workSocialCommentsAgg=/g) ?? []).toHaveLength(1);
    expect(sha(rendererLine)).toBe('f8cfc41c37cdff30db6fa221ff31a7cf25a7fe3f10ad2a5feca36db41be79289');
    expect(rendererLine).toContain('ORDER BY c.id) FROM social_comments c ${tail})');
    expect((rendererLine.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendererLine).not.toMatch(lockRe);

    const purchases = src('work-social-purchases.ts');
    const taskSource = src('work-task-purchase-source.ts');
    expect(purchases).toContain(`workSocialCommentsAgg('${WHERE_TAIL}')`);
    expect(purchases).toContain(`workSocialCommentsAgg('${JOIN_TAIL}')`);
    expect(taskSource).toContain(`workSocialCommentsAgg('${JOIN_TAIL}')`);
    expect(purchases).not.toContain('jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c WHERE c.post_id=?');
    expect(purchases).not.toContain('jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts');
    expect(taskSource).not.toContain('jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts');
    expect(purchases.split('workSocialCommentsAgg').length - 1).toBe(3); // import + 2 call sites
    expect(taskSource.split('workSocialCommentsAgg').length - 1).toBe(2); // import + 1 call site
  });
});

describe('Collaboration read-tail r53 excluded SQL stays untouched', () => {
  test('inbox whole-file pins, agent-reply locks/claim/finalize and notifications are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
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

describe('Collaboration read-tail r53 KPI pins', () => {
  test('retirement:ts-statements is pinned at 279, below the r53 BEFORE=330 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // One byte-identical comment-aggregate head family -> one parameterized
    // renderer: 330 -> 329. Tails, binds and locks stay fixed.
    expect(total).toBe(279);
    expect(total).toBeLessThan(330);
  });
});
