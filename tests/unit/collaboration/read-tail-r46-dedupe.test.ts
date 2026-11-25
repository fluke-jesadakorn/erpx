import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostIdHead } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const head = 'SELECT p.id FROM social_posts p WHERE ';
const headSha = 'c3da7edb04fe72dc9065e3ba946bb8f4245ec66edcb10783b66c781bc0d47cdd';
const call = (tail: string) => '${socialPostIdHead(`' + tail + '`)}';

// Exact pre-r46 source-text tails at BEFORE=338. The head, quantifiers, CTE
// prefix, bind array/order and ORDER/LIMIT are pinned by SHA so the expectation
// cannot drift silently. Tails are source text: `${...}` stays literal here.
const SITES = [
  {
    name: 'profile-posts author feed',
    file: 'profile-posts.ts',
    importMarker: "import {socialPostIdHead} from './read-tail-sql';",
    prefix: '',
    tail: "p.author_id=? AND ${postScope.sql}\n      AND (?::bigint IS NULL OR p.created_at<? OR (p.created_at=? AND (?::text IS NULL OR p.id<?)))\n      ORDER BY p.created_at DESC,p.id DESC LIMIT 9",
    tailSha: '4cd9c200800e9f139536ee9bfdffb32d24e9fe7814ae9d691d9e67ae8074a1c4',
    selectCount: 1,
    orderLimit: 'ORDER BY p.created_at DESC,p.id DESC LIMIT 9',
    headTailSha: 'a7986b1a8d9c92306086d71067689000f930ee8a6c4eebda5e136b0b430f3dcf',
    beforeTplSha: 'a7986b1a8d9c92306086d71067689000f930ee8a6c4eebda5e136b0b430f3dcf',
    afterTplSha: '784b32b64977b55cfed25c0e1fe1d9590327162d1c398a7e4bb7f7351cc2af6a',
  },
  {
    name: 'pms-home-read-facts published ids',
    file: 'pms-home-read-facts.ts',
    importMarker: "import {socialPostIdHead} from './read-tail-sql';",
    prefix: '',
    tail: "p.id IN (${loadedIds.map(() => \"?\").join(\",\")})\n    AND p.company_id=? AND p.state='published' AND (?::text IS NULL OR p.branch_id IS NULL OR p.branch_id=?) AND ${sourceAccessSql(\"p\", \"?::text\")}",
    tailSha: 'b0b956aa2478fbfe09d532abe7ef3c38df2afe2d584f8cbbc5ef6112a262890a',
    selectCount: 1,
    orderLimit: '',
    headTailSha: '530ee48511accc9a7e08e643278a0923937d043a7ee42b491601c83376fc1893',
    beforeTplSha: '530ee48511accc9a7e08e643278a0923937d043a7ee42b491601c83376fc1893',
    afterTplSha: 'a2dae8e4def7701bb63ef6aa0766e308cb89b746ba2bf15141bb2b29cc5389ee',
  },
  {
    name: 'social-home feed ids',
    file: 'social-home.ts',
    importMarker: "socialPostIdHead} from './read-tail-sql';",
    prefix: '${purchase.prefix}',
    tail: "p.company_id=? AND p.state='published' AND p.linked_module IS NULL AND ${sourceAccessSql(\"p\", \"?::text\")} AND ${purchase.where} AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.branch_id=?) AND (p.created_at<? OR (p.created_at=? AND p.id<?)) AND (?=0 OR ${socialFollowPostExists(\"'post'\",'p.id')}) AND (?::text IS NULL OR ${socialPostTopicExists('?')} OR ${socialCommentTopicExists('?')})\n        AND (?::text IS NULL OR (p.audience_kind='team' AND p.audience_id=?))\n        AND (?::text IS NULL OR p.purpose=?) AND (?::text IS NULL OR NOT EXISTS(SELECT 1 FROM social_post_sources s WHERE s.post_id=p.id AND s.source_kind='pms_record')\n          OR EXISTS(SELECT 1 FROM social_post_sources s JOIN (${pmsSourcesSql}) pr ON pr.id=s.source_id\n            WHERE s.post_id=p.id AND s.source_kind='pms_record' AND pr.company_id=p.company_id AND pr.property_id=?))\n        AND (?::text IS NULL OR (?='milestones' AND p.kind='system') OR (?='conversations' AND p.kind<>'system'))\n        ORDER BY p.created_at DESC,p.id DESC LIMIT ?",
    tailSha: 'd8b5fffa5e458933dca14f227cd9fd7c3fa1537f71756b9e68a558d28201b242',
    selectCount: 3,
    orderLimit: 'ORDER BY p.created_at DESC,p.id DESC LIMIT ?',
    headTailSha: '1cb96300ad1b08aeb63810a2afa564ec1f60838fe09691b13d954e1e5373f492',
    beforeTplSha: '0d576b630954fcfddace95180347e5a4365b50125fa3ea22eb2aa28ed69fd819',
    afterTplSha: '87f1a9d402553947d111a9b9da271cf125c7b4bc9bc56454699af46b29c4c554',
  },
] as const;

const tplOf = (body: string, site: (typeof SITES)[number]) => {
  const at = body.indexOf(site.prefix + '${socialPostIdHead(');
  expect(at).toBeGreaterThanOrEqual(0);
  const open = body.lastIndexOf('`', at);
  const bind = body.indexOf('.bind(', at);
  expect(bind).toBeGreaterThan(open);
  return body.slice(open + 1, body.lastIndexOf('`', bind));
};

describe('Collaboration read-tail r46 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head and tail render byte for byte`, () => {
      expect(sha(head)).toBe(headSha);
      expect(sha(site.tail)).toBe(site.tailSha);
      expect(sha(head + site.tail)).toBe(site.headTailSha);
      expect(socialPostIdHead(site.tail)).toBe(head + site.tail);
      expect(sha(socialPostIdHead(site.tail))).toBe(site.headTailSha);
      expect(socialPostIdHead(site.tail).startsWith(head)).toBe(true);
      expect(socialPostIdHead(site.tail).match(/SELECT /g) ?? []).toHaveLength(site.selectCount);
      expect(site.tail.endsWith(site.orderLimit)).toBe(true);
      if (site.orderLimit) expect(site.tail.match(/ORDER BY/g) ?? []).toHaveLength(1);
      else expect(site.tail).not.toContain('ORDER BY');
    });

    test(`${site.name}: the post-r46 template reconstructs its pre-r46 bytes`, () => {
      const body = src(site.file);
      const tpl = tplOf(body, site);
      expect(sha(tpl)).toBe(site.afterTplSha);
      expect(tpl).toBe(site.prefix + call(site.tail));
      expect(sha(tpl.replace(call(site.tail), head + site.tail))).toBe(site.beforeTplSha);
      expect(tpl.startsWith(site.prefix)).toBe(true);
      expect(tpl).not.toContain(head);
    });
  }
});

describe('Collaboration read-tail r46 post-id family', () => {
  test('one renderer reproduces all three heads with their own tails', () => {
    for (const site of SITES) {
      expect(socialPostIdHead(site.tail)).toBe(head + site.tail);
      expect(socialPostIdHead(site.tail).match(/SELECT /g) ?? []).toHaveLength(site.selectCount);
      expect(site.tail.endsWith(site.orderLimit)).toBe(true);
    }
    expect(shared.match(/export const socialPostIdHead=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('(tail:string)=>`SELECT p.id FROM social_posts p WHERE ${tail}`');
    expect(sha(line(shared, 'export const socialPostIdHead='))).toBe('fcf4c6378e0fa59925b770e2857ca631aadfd9ab8b990bf4eda50a0777e2b66b');
  });

  test('all three adapters import and call the renderer exactly once', () => {
    for (const site of SITES) {
      const body = src(site.file);
      expect(body).toContain(call(site.tail));
      expect(body.match(/socialPostIdHead\(/g) ?? []).toHaveLength(1);
      expect(body).toContain(site.importMarker);
      expect(body).not.toContain(head);
    }
    expect(src('social-home.ts')).toContain('${purchase.prefix}${socialPostIdHead(');
  });
});

describe('Collaboration read-tail r46 protected SQL isolation', () => {
  test('the new renderer adds no lock, write or notification statement', () => {
    const added = line(shared, 'export const socialPostIdHead=');
    expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('r42-r45 helpers, notification targets and agent-reply locks stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    for (const [marker, digest] of [
      ['export const operationsSourceExists=', '100632e4f53c8ee9bbdfd7dd976dfac5b466118c9bce81140e877b58db40933f'],
      ['export const socialCommentHead=', '2ad63e64db338d62db117ee74155b7ea3281d741f48b4b6811ba884cc24b0d92'],
      ['export const socialCommentProjection=', 'e81afcc981c3c2bf41540a33fba9728e1870f57ec62b9fc4ffc2bb6771cc65c4'],
      ['export const socialPostDraftById=', '9962c290a602e5de5310ba0bb33ae9edd12c0f4db89acb0bb80b579f72814989'],
      ['export const messagingTeamImage=', '0b8462fac51b955ac03ce908b9a50e358700589c0aa3a537b0427dec1e3983e9'],
      ['export const socialTargetImage=', '85b5df731c67c23e797568e8ea4cd3c89f6a67a4bbda1769d522b7a730b8f6dc'],
      ['export const messagingLatestMessage=', '03274a5e1842dc1bff51d3b688d8d1fdbdd6acb56202ed8c80a0ef7e7908a422'],
      ['export const absentSocialPostGuard=', '90aa7d128113bdb5f4da43bce4a8acbf77d112970ea83aa6e27e538e3e7c33ce'],
      ['export const socialHandoverAckJoin=', '52afef4c97ea01d247426bab9469eb8166db069dce785966c2445c3acc41e064'],
    ] as Array<[string, string]>) {
      expect(sha(line(shared, marker))).toBe(digest);
    }
    const purchases = src('work-social-purchases.ts');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(shared.match(/export const operationsSourceExists=/g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r46 KPI pins', () => {
  test('retirement:ts-statements is pinned at 257, below the r46 BEFORE=338 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Three byte-identical post-id heads -> one owner-local renderer; the
    // four r45 settlement heads stay retired at 338. Net: 338 -> 336.
    // r47 merged the two owner/version guards into one renderer: 336 -> 335.
    expect(total).toBe(257);
    expect(total).toBeLessThan(338);
  });
});
