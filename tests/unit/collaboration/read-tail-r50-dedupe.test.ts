import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostRevisionsAgg, socialPostRevisionsAggBy, socialPostRevisionsFileAgg, socialTaskOriginPostExists } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const line = (marker: string) => shared.split('\n').find(l => l.startsWith(marker))!;

// Exact pre-r50 rendered revision aggregates at BEFORE=333, pinned by SHA so the
// byte identity of both retired helper heads cannot drift silently. The r50 merge
// only factors the shared `jsonb_agg(to_jsonb(<alias>) ORDER BY <alias>.id) FROM
// social_post_revisions <alias>` head into one alias-parameterized renderer;
// tails, binds, callers and locks stay fixed.
const TASK_FILE_TAIL = `JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${socialTaskOriginPostExists}`;

type Site = {
  name: string;
  alias: string;
  tail: string;
  sql: string;
  sqlSha: string;
  bytes: number;
  binds: number;
  selects: number;
};
const SITES: Site[] = [
  {
    name: 'purchase image revisions (r / WHERE bind)',
    alias: 'r',
    tail: 'WHERE r.post_id=?',
    sql: '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r WHERE r.post_id=?)',
    sqlSha: '9ba46d92c497a3e673118230ba713a899d806ff45991ccf365480efc1fc35c4f',
    bytes: 92,
    binds: 1,
    selects: 1,
  },
  {
    name: 'task purchase-source revisions (r / JOIN)',
    alias: 'r',
    tail: 'JOIN posts p ON p.id=r.post_id',
    sql: '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r JOIN posts p ON p.id=r.post_id)',
    sqlSha: '40935ffa42be750cd5e682bc29e73b76466aa8020a100571c4337bf4711f18d2',
    bytes: 105,
    binds: 0,
    selects: 1,
  },
  {
    name: 'work social attachment file revisions (revision / JOIN)',
    alias: 'revision',
    tail: 'JOIN posts p ON p.id=revision.post_id',
    sql: '(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN posts p ON p.id=revision.post_id)',
    sqlSha: '156b23c9d1030722002d53f6d84c1090b273a572361f0a457a869a2e656a1f05',
    bytes: 133,
    binds: 0,
    selects: 1,
  },
  {
    name: 'task file revisions (revision / JOIN + origin guard)',
    alias: 'revision',
    tail: TASK_FILE_TAIL,
    sql: `(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision ${TASK_FILE_TAIL})`,
    sqlSha: 'bf86d12c4c5e0c6e4856a87214ece1f9c82919de2afec8327911cf96cf8e6531',
    bytes: 287,
    binds: 0,
    selects: 2,
  },
];

const viaWrapper = (site: Site) => site.alias === 'r' ? socialPostRevisionsAgg(site.tail) : socialPostRevisionsFileAgg(site.tail);
const viaGeneric = (site: Site) => socialPostRevisionsAggBy(site.alias, site.tail);
const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i;

describe('Collaboration read-tail r50 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(Buffer.byteLength(site.sql)).toBe(site.bytes);
      expect(sha(viaWrapper(site))).toBe(site.sqlSha);
      expect(sha(viaGeneric(site))).toBe(site.sqlSha);
      expect(viaWrapper(site)).toBe(site.sql);
      expect(viaGeneric(site)).toBe(site.sql);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
      expect((site.sql.match(/SELECT /g) ?? []).length).toBe(site.selects);
      expect(site.sql.startsWith(`(SELECT jsonb_agg(to_jsonb(${site.alias}) ORDER BY ${site.alias}.id) FROM social_post_revisions ${site.alias} `)).toBe(true);
      expect(site.sql.endsWith(')')).toBe(true);
      expect(site.sql).not.toMatch(lockRe);
    });
  }

  test('the two retired heads were the same template under different aliases', () => {
    const rHead = '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r ';
    const revisionHead = '(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision ';
    const genericHead = '(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM social_post_revisions ${alias} ';
    expect(Buffer.byteLength(rHead)).toBe(74);
    expect(Buffer.byteLength(revisionHead)).toBe(95);
    expect(sha(rHead)).toBe('e9791ad69a8db6c66a242bb57b43868075bf6a0053119e409b1db1d5275e8fa4');
    expect(sha(revisionHead)).toBe('04aac137d9330cda5f5b0c9dd93ad47519f8ae5d42940065f9df0772cc983197');
    expect(revisionHead).toBe(rHead.replace(/\br\b/g, 'revision'));
    expect(Buffer.byteLength(genericHead)).toBe(95);
    expect(sha(genericHead)).toBe('0e15f669f92003ead39fafc26bc5c0da28bd0e7503c3c2a2c5c2f9d617a4d4c2');
    expect(socialPostRevisionsAggBy('r', '')).toBe(`${rHead})`);
    expect(socialPostRevisionsAggBy('revision', '')).toBe(`${revisionHead})`);
  });
});

describe('Collaboration read-tail r50 revisions-aggregate fold', () => {
  test('one alias-parameterized renderer plus two retained thin wrappers replace the two heads', () => {
    expect(shared.match(/export const socialPostRevisionsAggBy=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialPostRevisionsAgg=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialPostRevisionsFileAgg=/g) ?? []).toHaveLength(1);
    expect(sha(line('export const socialPostRevisionsAggBy='))).toBe('3c07a462c17fee49a2fecfa6e3d5fae2929899ecf180f1d4e002bd077554ef64');
    expect(sha(line('export const socialPostRevisionsAgg='))).toBe('69bcdd6271524e9466d33d6d2b0dd21a3dfb9caed2ce81658c8bbb2cd457575d');
    expect(sha(line('export const socialPostRevisionsFileAgg='))).toBe('d649d3932556829deac57895db6435f4f6a1fcf6ba717d8d4c954b68200b4c8c');
    expect(line('export const socialPostRevisionsAggBy=')).toContain('jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM social_post_revisions ${alias}');
    for (const marker of ['export const socialPostRevisionsAgg=', 'export const socialPostRevisionsFileAgg=']) {
      const wrapper = line(marker);
      expect(wrapper).not.toContain('SELECT ');
      expect(wrapper).toContain('socialPostRevisionsAggBy(');
    }
    expect(line('export const socialPostRevisionsAgg=')).toContain("socialPostRevisionsAggBy('r',tail)");
    expect(line('export const socialPostRevisionsFileAgg=')).toContain("socialPostRevisionsAggBy('revision',tail)");
    expect((shared.match(/jsonb_agg\(to_jsonb\(\$\{alias\}\) ORDER BY \$\{alias\}\.id\) FROM social_post_revisions \$\{alias\}/g) ?? []).length).toBe(1);
  });

  test('the new renderer adds no lock, write or notification statement', () => {
    const added = ['export const socialPostRevisionsAggBy=', 'export const socialPostRevisionsAgg=', 'export const socialPostRevisionsFileAgg='];
    for (const marker of added) expect(line(marker)).not.toMatch(lockRe);
    expect((line('export const socialPostRevisionsAggBy=').match(/SELECT /g) ?? []).length).toBe(1);
    expect((line('export const socialPostRevisionsAgg=').match(/SELECT /g) ?? []).length).toBe(0);
    expect((line('export const socialPostRevisionsFileAgg=').match(/SELECT /g) ?? []).length).toBe(0);
  });

  test('consumers keep their imports, call sites and bind arrays', () => {
    const purchases = src('work-social-purchases.ts');
    const task = src('work-task-purchase-source.ts');
    expect(purchases).toContain("socialPostRevisionsAgg('WHERE r.post_id=?')");
    expect(purchases).toContain("socialPostRevisionsFileAgg('JOIN posts p ON p.id=revision.post_id')");
    expect(task).toContain("socialPostRevisionsAgg('JOIN posts p ON p.id=r.post_id')");
    expect(task).toContain('${socialPostRevisionsFileAgg(`JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${socialTaskOriginPostExists}`)}');
    expect(task).toContain('.bind(taskId,requestedPostId??null,requestedPostId??null,companyId)');
    expect(purchases).toContain('.bind(await target(database,actor,{...command,id:sourceId}),postId,actor.companyId');
  });
});

describe('Collaboration read-tail r50 excluded SQL stays untouched', () => {
  test('inbox whole-file pins, agent-reply locks and notifications are unchanged', () => {
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
    expect(sha(shared)).toBe('6f2381ad161f275e79f1dadbc22ee32ef27bbc2f5e6296872973b590d6a85204');
  });
});

describe('Collaboration read-tail r50 KPI pins', () => {
  test('retirement:ts-statements is pinned at 272, below the r50 BEFORE=333 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical revision aggregate heads -> one alias-parameterized
    // renderer with two thin wrappers. Net: 333 -> 332.
    // r51 folded the shared count head: 332 -> 331.
    expect(total).toBe(272);
    expect(total).toBeLessThan(333);
  });
});
