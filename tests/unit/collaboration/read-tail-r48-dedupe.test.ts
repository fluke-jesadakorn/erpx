import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingMembersAgg, messagingRoomMembersAgg, messagingTeamMembersAgg } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');

// Exact pre-r48 rendered member aggregates at BEFORE=335, pinned by SHA so the
// byte identity of both retired heads cannot drift silently. The r48 merge only
// generalizes the member table and WHERE scope; the aggregate head, table alias,
// bind order and lock semantics stay fixed.
const HEAD = '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM ';
const ROOM = '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id)';
const TEAM_R = '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=r.team_id)';
const TEAM_Q = '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=?)';

const SITES = [
  {
    name: 'room member aggregate (attachment readers)',
    table: 'messaging_room_members', where: 'm.room_id=r.id',
    sql: ROOM, sqlSha: 'f658ab3e07efc0d2e4611611d7fc45e2241e283f5b37389f940f9e2aa1f63963',
    render: () => messagingRoomMembersAgg,
  },
  {
    name: 'team member aggregate (r.team_id)',
    table: 'messaging_team_members', where: 'm.team_id=r.team_id',
    sql: TEAM_R, sqlSha: '7c1ef45ec628f23d69c2ca927ca4ff40f7a75142bbbfbffca372079b77870f5d',
    render: () => messagingTeamMembersAgg('r.team_id'),
  },
  {
    name: 'team member aggregate (?)',
    table: 'messaging_team_members', where: 'm.team_id=?',
    sql: TEAM_Q, sqlSha: 'c9149ce3e5850d06d8b61c95ce06bed72aafd57a5e6d81a6e0d065d6691d5440',
    render: () => messagingTeamMembersAgg('?'),
  },
] as const;

describe('Collaboration read-tail r48 per-site byte oracles', () => {
  for (const site of SITES) {
    test(`${site.name}: the retired head renders byte for byte`, () => {
      expect(sha(site.sql)).toBe(site.sqlSha);
      const merged = messagingMembersAgg(site.table, site.where);
      expect(merged).toBe(site.sql);
      expect(sha(merged)).toBe(site.sqlSha);
      const wrapper = site.render();
      expect(wrapper).toBe(site.sql);
      expect(sha(wrapper)).toBe(site.sqlSha);
      expect(site.sql.startsWith(HEAD)).toBe(true);
      expect(site.sql).toContain(' FROM ' + site.table + ' m WHERE ' + site.where + ')');
      expect(site.sql.match(/SELECT /g) ?? []).toHaveLength(1);
      expect(site.sql.match(/\?/g) ?? []).toHaveLength(site.where === 'm.team_id=?' ? 1 : 0);
      expect(site.render()).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    });
  }
});

describe('Collaboration read-tail r48 member-aggregate family', () => {
  test('one renderer plus two retained thin wrappers replace the two heads', () => {
    expect(shared.match(/export const messagingMembersAgg=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingRoomMembersAgg=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingTeamMembersAgg=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('(table:string,where:string)=>`(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM ${table} m WHERE ${where})`');
    expect(sha(line(shared, 'export const messagingMembersAgg='))).toBe('82b4bbbf2fae9de8253afadd96e54f3407640c031c4123d9e92f814037591f68');
    expect(sha(line(shared, 'export const messagingRoomMembersAgg='))).toBe('40196508ea34905c62413fb56042ab2bdfbdc0b2f688c27a8f864908ec4b33ea');
    expect(sha(line(shared, 'export const messagingTeamMembersAgg='))).toBe('4ad0921ff43f207ca5934cd33e3ab6ad5abac7403403b901e4182f0d27832eaf');
    for (const marker of ['export const messagingRoomMembersAgg=', 'export const messagingTeamMembersAgg=']) {
      const wrapper = line(shared, marker);
      expect(wrapper).not.toContain('SELECT ');
      expect(wrapper).toContain('messagingMembersAgg(');
    }
    expect(line(shared, 'export const messagingRoomMembersAgg=')).toContain("messagingMembersAgg('messaging_room_members','m.room_id=r.id')");
    expect(line(shared, 'export const messagingTeamMembersAgg=')).toContain('messagingMembersAgg(\'messaging_team_members\',`m.team_id=${teamId}`)');
  });

  test('the new renderer adds no lock, write or notification statement', () => {
    const added = line(shared, 'export const messagingMembersAgg=');
    expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  test('consumers keep their original imports, calls and bind arrays', () => {
    const purchase = src('messaging-attachment-purchases.ts');
    const msgPurchase = src('messaging-purchases.ts');
    const read = src('messaging-attachment-read.ts');
    expect(purchase).toContain("import {messagingRoomMembersAgg,messagingTeamImage,messagingTeamMembersAgg,workforceActorScopeExists} from './read-tail-sql';");
    expect(purchase).toContain("'members',${messagingRoomMembersAgg},'team',${messagingTeamImage('r.team_id','r.company_id',true)},'teamMembers',${messagingTeamMembersAgg('r.team_id')}");
    expect(msgPurchase).toContain("'teamMembers',${messagingTeamMembersAgg(team?'?':messagingRoomTeamId)}");
    expect(msgPurchase).toContain("'members',${messagingRoomMembersAgg}");
    expect(read).toContain("'members',${messagingRoomMembersAgg},'team',${messagingTeamImage('r.team_id','r.company_id')},'teamMembers',${messagingTeamMembersAgg('r.team_id')}");
    for (const body of [purchase, msgPurchase, read]) {
      expect(body).not.toContain('jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members');
      expect(body).not.toContain('jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members');
    }
  });
});

describe('Collaboration read-tail r48 protected SQL isolation', () => {
  test('r46/r47 heads, inbox pins, notification targets and agent-reply locks stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    for (const [marker, digest] of [
      ['export const socialPostIdHead=', 'fcf4c6378e0fa59925b770e2857ca631aadfd9ab8b990bf4eda50a0777e2b66b'],
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

describe('Collaboration read-tail r48 KPI pins', () => {
  test('retirement:ts-statements is pinned at 306, below the r48 BEFORE=335 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Two byte-identical member-aggregate heads -> one member-local renderer
    // with two thin table/scope wrappers. Net: 335 -> 334.
    expect(total).toBe(306);
    expect(total).toBeLessThan(335);
  });
});
