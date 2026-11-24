import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingTeamAccessSql, messagingMemberExists } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const messaging = src('messaging.ts');
const messagingPurchases = src('messaging-purchases.ts');

// r31 generalizes the byte-identical managed/team-member access guard shared by
// the single-team and team-list readers in messaging.ts. Only the leading id
// predicate, the managed-team SQL and the trailing ORDER BY vary; the fixed
// company scope, bind order and lock semantics stay unchanged.
const MANAGED = 'MANAGED_SQL';
const TEAM_MEMBER = messagingMemberExists('messaging_team_members','team_id','t.id');
const TEAM_BY_ID = 'SELECT t.* FROM messaging_teams t WHERE t.id=? AND t.company_id=? AND ((NOT t.organization_managed AND ' + TEAM_MEMBER + ') OR (t.organization_managed AND MANAGED_SQL))';
const TEAM_LIST = 'SELECT t.* FROM messaging_teams t WHERE t.company_id=? AND ((NOT t.organization_managed AND ' + TEAM_MEMBER + ') OR (t.organization_managed AND MANAGED_SQL)) ORDER BY t.name,t.id';

describe('Collaboration read-tail r31 parameterized family', () => {
  test('messagingTeamAccessSql renders both readers byte-identically', () => {
    expect(messagingTeamAccessSql('t.id=? AND ', MANAGED, '')).toBe(TEAM_BY_ID);
    expect(messagingTeamAccessSql('', MANAGED, ' ORDER BY t.name,t.id')).toBe(TEAM_LIST);
    expect(shared.match(/export const messagingTeamAccessSql=/g) ?? []).toHaveLength(1);
    expect(messaging).toContain("messagingTeamAccessSql('t.id=? AND ',messagingManagedTeamSql(\"t\", \"?\"),'')");
    expect(messaging).toContain("messagingTeamAccessSql('',messagingManagedTeamSql(\"t\", \"?\"),' ORDER BY t.name,t.id')");
    expect((messaging.match(/messagingTeamAccessSql\(/g) ?? []).length).toBe(2);
    expect(messaging).not.toContain('SELECT t.* FROM messaging_teams t WHERE t.id=?');
    expect(messaging).not.toContain('SELECT t.* FROM messaging_teams t WHERE t.company_id=?');
  });
});

describe('Collaboration read-tail r31 lock/notification isolation', () => {
  test('lock statements and notification helpers stay untouched', () => {
    const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/;
    expect(lockRe.test(messagingTeamAccessSql('', MANAGED, ''))).toBe(false);
    expect(messagingPurchases).toContain('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE');
    expect(src('work-social-purchases.ts')).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(src('work-task-purchase-source.ts')).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(src('staff-notifications.ts')).toContain('legacy_rows AS');
  });
});

describe('Collaboration read-tail r31 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=390 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r31 deduped one family: the managed/team-member team access guard at two
    // inline sites (-2) replaced by one shared helper (+1). Net -1 SELECT token;
    // no lock statement or notification helper was modified.
    expect(total).toBeLessThanOrEqual(389);
    expect(total).toBeLessThan(390);
  });
});
