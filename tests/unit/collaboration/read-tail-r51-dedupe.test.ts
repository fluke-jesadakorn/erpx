import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { countIntNFrom } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const rendererLine = shared.split('\n').find(l => l.startsWith('export const countIntNFrom='))!;

// Exact pre-r51 rendered counts frozen from the live HEAD before the fold (SHA,
// byte length, bind count). Both sites shared the byte-identical head
// `SELECT count(*)::int n FROM `; only the FROM expression varied. The r51 fold
// routes both through one owner-local renderer; tails, binds, callers and locks
// stay fixed. The inbox whole-file pin, the agent-reply lock/finalize statement
// and the notification files are deliberately not part of this family.
const HEAD = 'SELECT count(*)::int n FROM ';
const NOTICE_FROM = `inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
        WHERE e.company_id=? AND e.event_key=?`;
const PUSH_FROM = 'push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?';

type Site = {
  name: string;
  from: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
  selects: number;
};
const SITES: Site[] = [
  {
    name: 'notice delivery count (r21, 2 binds, retained line break)',
    from: NOTICE_FROM,
    sql: `${HEAD}${NOTICE_FROM}`,
    sqlSha: '8265283da494d34215995a25b8abec42b1ae3ec63d27ce1daef3bd49a72852e1',
    bytes: 132,
    binds: 2,
    selects: 1,
  },
  {
    name: 'push subscription session count (r11, 3 binds)',
    from: PUSH_FROM,
    sql: `${HEAD}${PUSH_FROM}`,
    sqlSha: 'f9c7702f9fa7ab486acbddc5a6dab47d31fd74b2d27b5c14a7516acfc9081b0e',
    bytes: 96,
    binds: 3,
    selects: 1,
  },
];

const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i;

describe('Collaboration read-tail r51 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(sha(countIntNFrom(site.from))).toBe(site.sqlSha);
      expect(countIntNFrom(site.from)).toBe(site.sql);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect((site.sql.match(/SELECT /g) ?? []).length).toBe(site.selects);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.sql).not.toMatch(lockRe);
    });
  }

  test('both tails are distinct, so the head is the only shared bytes', () => {
    expect(Buffer.byteLength(HEAD)).toBe(28);
    expect(sha(HEAD)).toBe('9b90d3f44cbd4a527606322db386e2daba3e54d6f574b1b3b5ba858c7e383b58');
    expect(NOTICE_FROM).not.toBe(PUSH_FROM);
    expect(countIntNFrom(NOTICE_FROM).slice(HEAD.length)).toBe(NOTICE_FROM);
    expect(countIntNFrom(PUSH_FROM).slice(HEAD.length)).toBe(PUSH_FROM);
  });
});

describe('Collaboration read-tail r51 renderer fold', () => {
  test('one renderer owns the single SELECT head and both call sites route through it', () => {
    expect(shared.match(/export const countIntNFrom=/g) ?? []).toHaveLength(1);
    expect(sha(rendererLine)).toBe('a663d8e5467a6afe68693f2dff01aefb95f1725c77079df749bf1853d48d90e5');
    expect(rendererLine).toContain('SELECT count(*)::int n FROM ${from}');
    expect((rendererLine.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendererLine).not.toMatch(lockRe);

    const people = src('people-app-entrypoint-facts.ts');
    const push = src('web-push.ts');
    expect(people).toContain("sentNoticeDeliveryCount as nativeDeliveryCount");
    expect(people).toContain("taskSocialOrigin as nativeTaskOrigin");
    expect(push).toContain("pushSessionSubscriptionCount } from './native-lane-16/client'");
    expect(people).toContain('nativeDeliveryCount(db, companyId, eventKey, actor)');
    expect(people).not.toContain('.bind(companyId,eventKey).first<{n:number}>()');
    expect(push).toContain('pushSessionSubscriptionCount(db, companyId, userId, String(value), actor)');
    expect((people.match(/countIntNFrom\(/g) ?? []).length).toBe(0);
    expect((push.match(/countIntNFrom\(/g) ?? []).length).toBe(0);
    expect(people).not.toContain('`SELECT count(*)::int n FROM inbox_deliveries');
    expect(push).not.toContain('"SELECT count(*)::int n FROM push_subscriptions');
  });
});

describe('Collaboration read-tail r51 excluded SQL stays untouched', () => {
  test('inbox whole-file pins, agent-reply locks/finalize and notifications are unchanged', () => {
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

describe('Collaboration read-tail r51 KPI pins', () => {
  test('retirement:ts-statements is pinned at 297, below the r51 BEFORE=332 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical count heads -> one renderer: 332 -> 331.
    expect(total).toBe(297);
    expect(total).toBeLessThan(332);
  });
});
