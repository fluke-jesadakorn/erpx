import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { attachmentVersionGuard, messagingOwnerGuard, ownerVersionGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const messaging = src('messaging.ts');
const portalFacts = src('attachments-portal-facts.ts');
const serviceFacts = src('attachments-service-facts.ts');

// Exact pre-r47 rendered SQL at BEFORE=336, pinned by SHA so the byte identity
// of both retired guard families cannot drift silently. The r47 merge only
// generalizes the guarded column; table, tail, binds and order stay fixed.
const SITES = [
  {
    name: 'team owner (r26 TEAM_OWNER)',
    table: 'messaging_teams', col: 'owner_id', tail: '',
    sql: 'EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=?)',
    sqlSha: 'd927523e009dd90970f2540796365887017aecf7e2189aa25a35d0f432000fd2',
    binds: 3, via: 'messagingOwnerGuard',
  },
  {
    name: 'team version (r26 TEAM_VERSION)',
    table: 'messaging_teams', col: 'owner_id', tail: ' AND version=?',
    sql: 'EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=? AND version=?)',
    sqlSha: 'd40ee1f4c8ef8180275f03e40c6c5770f8472312141213762c2141124f406dc2',
    binds: 4, via: 'messagingOwnerGuard',
  },
  {
    name: 'room version (r26 ROOM_VERSION)',
    table: 'messaging_rooms', col: 'owner_id', tail: ' AND version=?',
    sql: 'EXISTS(SELECT 1 FROM messaging_rooms WHERE id=? AND company_id=? AND owner_id=? AND version=?)',
    sqlSha: 'a07b1fa50256cf323296b19a1fbda5e3e0623e245da7d5562ad62c2ec8b24822',
    binds: 4, via: 'messagingOwnerGuard',
  },
  {
    name: 'published post attachment (r33 POST_PUBLISHED)',
    table: 'social_posts', col: 'version', tail: " AND state='published'",
    sql: "EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=? AND state='published')",
    sqlSha: '14b66f50d4943f0e398675c71dfe5c4006370887e44a6634bc4e1f1130715d0b',
    binds: 3, via: 'attachmentVersionGuard',
  },
  {
    name: 'social draft attachment (r33 DRAFT)',
    table: 'social_post_drafts', col: 'version', tail: '',
    sql: 'EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND company_id=? AND version=?)',
    sqlSha: '3dae56f32c9a68520dac6bc62abe1ce934c77c7d9543370d218647f6f248e533',
    binds: 3, via: 'attachmentVersionGuard',
  },
  {
    name: 'messaging room attachment (r33 ROOM)',
    table: 'messaging_rooms', col: 'version', tail: '',
    sql: 'EXISTS(SELECT 1 FROM messaging_rooms WHERE id=? AND company_id=? AND version=?)',
    sqlSha: 'b04502828b5b1c3ba26d9947ba180cf432a18e14b4e95490414c9d9438d7eae1',
    binds: 3, via: 'attachmentVersionGuard',
  },
] as const;

const guard = (site: (typeof SITES)[number]) =>
  site.via === 'messagingOwnerGuard'
    ? messagingOwnerGuard(site.table, site.tail)
    : attachmentVersionGuard(site.table, site.tail);

describe('Collaboration read-tail r47 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired guard renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      expect(ownerVersionGuard(site.table, site.col, site.tail)).toBe(site.sql);
      expect(sha(ownerVersionGuard(site.table, site.col, site.tail))).toBe(site.sqlSha);
      expect(guard(site)).toBe(site.sql);
      expect(sha(guard(site))).toBe(site.sqlSha);
      expect(site.sql.startsWith('EXISTS(SELECT 1 FROM ' + site.table + ' WHERE id=? AND company_id=? AND ' + site.col + '=?')).toBe(true);
      expect(site.sql.endsWith(site.tail + ')')).toBe(true);
      expect(site.sql.match(/SELECT /g) ?? []).toHaveLength(1);
      expect((site.sql.match(/\?/g) ?? []).length).toBe(site.binds);
    });
  }
});

describe('Collaboration read-tail r47 owner/version family', () => {
  test('one renderer plus two retained thin wrappers replace the two guards', () => {
    expect(shared.match(/export const ownerVersionGuard=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingOwnerGuard=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const attachmentVersionGuard=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('(table:string,col:string,tail:string)=>`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${col}=?${tail})`');
    expect(sha(line(shared, 'export const ownerVersionGuard='))).toBe('ed078b70686181c1dd8259d61fd4fe1b414b8906928acf5eedb94919ff132a4e');
    expect(sha(line(shared, 'export const messagingOwnerGuard='))).toBe('644c487a818d528078a1c1cd3f0ed86aeda6bf14f635de1d6a8ef11f35d8037b');
    expect(sha(line(shared, 'export const attachmentVersionGuard='))).toBe('fc1aabaf92fb2f93e1d711fbf8ce37e886fdf425509cd8ee15b492adc57cd975');
    for (const marker of ['export const messagingOwnerGuard=', 'export const attachmentVersionGuard=']) {
      const wrapper = line(shared, marker);
      expect(wrapper).not.toContain('SELECT ');
      expect(wrapper).toContain('ownerVersionGuard(table,');
    }
    expect(line(shared, 'export const messagingOwnerGuard=')).toContain("ownerVersionGuard(table,'owner_id',tail)");
    expect(line(shared, 'export const attachmentVersionGuard=')).toContain("ownerVersionGuard(table,'version',tail)");
  });

  test('consumers keep their original imports, calls and bind arrays', () => {
    expect(messaging).toContain('messagingOwnerGuard,messagingTeamAccessSql');
    expect(messaging).toContain('messagingOwnerGuard("messaging_teams", ""), [teamId, companyId, userId]');
    expect(messaging).toContain('messagingOwnerGuard(isTeam ? "messaging_teams" : "messaging_rooms", " AND version=?"), [id, companyId, userId, expectedVersion]');
    expect(portalFacts).toContain("attachmentVersionGuard('social_posts', \" AND state='published'\")");
    expect(serviceFacts).toContain('attachmentVersionGuard(item.table, item.extra ? ` AND ${item.extra}` : \'\')');
    expect(portalFacts).not.toContain("EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=?");
    expect(serviceFacts).not.toContain('EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?');
    expect(messaging).not.toContain('EXISTS(SELECT 1 FROM messaging_teams WHERE id=? AND company_id=? AND owner_id=?');
  });
});

describe('Collaboration read-tail r47 protected SQL isolation', () => {
  test('the merged renderer adds no lock, write or notification statement', () => {
    const added = line(shared, 'export const ownerVersionGuard=');
    expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('r46 post-id head, agent-reply locks, advisory and inbox pins stay unchanged', () => {
    expect(sha(line(shared, 'export const socialPostIdHead='))).toBe('fcf4c6378e0fa59925b770e2857ca631aadfd9ab8b990bf4eda50a0777e2b66b');
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
    expect(src('social-agent.ts')).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
  });
});

describe('Collaboration read-tail r47 KPI pins', () => {
  test('retirement:ts-statements is pinned at 300, below the r47 BEFORE=336 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical owner/version guards -> one owner-local renderer with
    // two thin column wrappers. Net: 336 -> 335.
    expect(total).toBe(300);
    expect(total).toBeLessThan(336);
  });
});
