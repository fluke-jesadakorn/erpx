import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingMemberExists, messagingTeamAccessSql, socialAiUsageProjection, socialTaskOriginBy } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';
import { agentSocialMonthlyUsage, socialAiUsageSql } from '../../../packages/server-collaboration/src/adapters/agent-ai-usage-projection';
import { financeAiSocialUsageSql } from '../../../packages/server-collaboration/src/adapters/finance-ai-quota-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-collaboration/src/adapters/hospitality-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const people = src('people-app-entrypoint-facts.ts');
const betaSources = src('beta-management-sources.ts');
const betaFacts = src('beta-management-facts.ts');
const taskSource = src('social-task.ts');
const agentUsage = src('agent-ai-usage-projection.ts');
const hospitality = src('hospitality-owner-projection-sources.ts');
const sourcePolicy = src('messaging-source-policy.ts');
const home = src('social-home.ts');
const purchases = src('work-social-purchases.ts');

// Exact pre-r41 strings from HEAD. Each retired pair rendered one of these
// byte-for-byte; binds, locks and notification targets were never changed.
const TASK_BY_TASK = 'SELECT post_id FROM social_task_origins WHERE task_id=?';
const TASK_BY_POST = 'SELECT task_id FROM social_task_origins WHERE post_id=?';
const AGENT_USAGE = 'SELECT company_id,month FROM social_ai_usage';
const HOSPITALITY_USAGE = 'SELECT "id","company_id","actor_id","month","state","input_tokens","output_tokens","lease_until","created_at" FROM social_ai_usage';
const TEAM_MEMBER = 'EXISTS(SELECT 1 FROM messaging_team_members m WHERE m.team_id=t.id AND m.user_id=?)';
const ROOM_MEMBER = 'EXISTS(SELECT 1 FROM messaging_room_members m WHERE m.room_id=r.id AND m.user_id=?)';

// r41 retires three pairs into three closed renderers (one SELECT head each):
// task-origin by key (-1), AI-usage row projection (-1) and team/room member
// existence (-1): 355 -> 352. Only these three families were touched.
describe('Collaboration read-tail r41 task-origin by-key family', () => {
  test('one closed renderer reproduces both retired heads byte-identically with one bind', () => {
    expect(socialTaskOriginBy('post_id', 'task_id')).toBe(TASK_BY_TASK);
    expect(socialTaskOriginBy('task_id', 'post_id')).toBe(TASK_BY_POST);
    expect(socialTaskOriginBy('post_id', 'task_id').split('?').length - 1).toBe(1);
    expect(socialTaskOriginBy('task_id', 'post_id').split('?').length - 1).toBe(1);
    expect(shared.match(/export const socialTaskOriginBy=/g) ?? []).toHaveLength(1);
    expect(shared).not.toContain('socialTaskOriginByTask');
    expect(shared).not.toContain('socialTaskOriginByPost');
    expect(sha(line(shared, 'export const socialTaskOriginBy='))).toBe('3fc84d696b83a92bcb18db27ee2645ac91db2684c2ac91887b99964f59baf701');
  });

  test('native cutover retires the people and beta call sites; the other three pass the original column/key pair verbatim', () => {
    expect(people).toContain('taskSocialOrigin as nativeTaskOrigin');
    expect(people).toContain('nativeTaskOrigin(db, taskId, actor)');
    expect(betaSources).toContain("db.prepare(socialTaskOriginBy('task_id','post_id')).bind(id)");
    expect(betaFacts).toContain('nativeBetaPostTask');
    expect(betaFacts).toContain('nativeBetaRecoverPostTask');
    expect(taskSource).toContain("db.prepare(socialTaskOriginBy('post_id','task_id')).bind(taskId)");
    expect(people.match(/socialTaskOriginBy\('/g) ?? []).toHaveLength(0);
    expect(betaSources.match(/socialTaskOriginBy\('/g) ?? []).toHaveLength(1);
    expect(betaFacts.match(/socialTaskOriginBy\('/g) ?? []).toHaveLength(0);
    expect(taskSource.match(/socialTaskOriginBy\('/g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r41 AI-usage row projection family', () => {
  test('one closed renderer reproduces both zero-bind heads byte-identically', () => {
    expect(socialAiUsageProjection('company_id,month')).toBe(AGENT_USAGE);
    expect(socialAiUsageProjection('"id","company_id","actor_id","month","state","input_tokens","output_tokens","lease_until","created_at"')).toBe(HOSPITALITY_USAGE);
    expect(socialAiUsageProjection('company_id,month')).not.toContain('?');
    expect(socialAiUsageProjection('"id","company_id","actor_id","month","state","input_tokens","output_tokens","lease_until","created_at"')).not.toContain('?');
    expect(shared.match(/export const socialAiUsageProjection=/g) ?? []).toHaveLength(1);
    expect(sha(line(shared, 'export const socialAiUsageProjection='))).toBe('871b54166f9ae07c4d255f7b746aa2509aaaaf151da98d07d6de1a9ab716726d');
    expect(sha(line(shared, 'export const socialAiUsageRead='))).toBe('cc14834e954b46ffa6f07a81d97ccf47964eba4e7aea19a00ce829f540c0ec8b');
  });

  test('agent, downstream quota and hospitality consumers keep their exact values', () => {
    expect(socialAiUsageSql).toBe(AGENT_USAGE);
    expect(agentSocialMonthlyUsage()).toBe(AGENT_USAGE);
    expect(financeAiSocialUsageSql).toBe(AGENT_USAGE);
    expect(hospitalityOwnerProjectionSources.social_ai_usage).toBe(HOSPITALITY_USAGE);
    expect(agentUsage).toContain("export const socialAiUsageSql=socialAiUsageProjection('company_id,month');");
    expect(hospitality).toContain("social_ai_usage: socialAiUsageProjection('\"id\",\"company_id\",\"actor_id\",\"month\",\"state\",\"input_tokens\",\"output_tokens\",\"lease_until\",\"created_at\"'),");
    expect(agentUsage).not.toContain('"SELECT company_id,month FROM social_ai_usage"');
    expect(hospitality).not.toContain('"SELECT \\"id\\"');
    expect(sha(line(agentUsage, 'import {socialAiMonthlyUsageCount'))).toBe('333c5fb04ce0a78169e350c30fa54f8b36b2c29367f357e83b92ff5915d1a485');
    expect(sha(line(hospitality, '  social_ai_usage:'))).toBe('de361a16b1bc8db6f1711fe76a070079ec3b9048c7cb089585ca3c68ee65c84e');
  });
});

describe('Collaboration read-tail r41 team/room member-existence family', () => {
  test('one closed team/room renderer reproduces both guards with one bind each', () => {
    expect(messagingMemberExists('messaging_team_members', 'team_id', 't.id')).toBe(TEAM_MEMBER);
    expect(messagingMemberExists('messaging_room_members', 'room_id', 'r.id')).toBe(ROOM_MEMBER);
    expect(messagingMemberExists('messaging_team_members', 'team_id', 't.id').split('?').length - 1).toBe(1);
    expect(messagingMemberExists('messaging_room_members', 'room_id', 'r.id').split('?').length - 1).toBe(1);
    expect(shared.match(/export const messagingMemberExists=/g) ?? []).toHaveLength(1);
    expect(shared).not.toContain('messagingTeamMemberExists');
    expect(shared).not.toContain('messagingRoomMemberExists');
    expect(shared).toContain("table:'messaging_team_members'|'messaging_room_members'");
    expect(shared).toContain("groupCol:'team_id'|'room_id'");
    expect(sha(line(shared, 'export const messagingMemberExists='))).toBe('edd4a779a4f3aa96f13a7d0e1cc22118e29661db5cc27774a9fb8d0a808e7b10');
  });

  test('all four call sites and both rendered policies stay byte-identical', () => {
    expect(messagingTeamAccessSql('t.id=? AND ', 'MANAGED_SQL', '')).toBe(
      'SELECT t.* FROM messaging_teams t WHERE t.id=? AND t.company_id=? AND ((NOT t.organization_managed AND ' + TEAM_MEMBER + ') OR (t.organization_managed AND MANAGED_SQL))',
    );
    expect(messagingTeamAccessSql('', 'MANAGED_SQL', ' ORDER BY t.name,t.id')).toBe(
      'SELECT t.* FROM messaging_teams t WHERE t.company_id=? AND ((NOT t.organization_managed AND ' + TEAM_MEMBER + ') OR (t.organization_managed AND MANAGED_SQL)) ORDER BY t.name,t.id',
    );
    expect(shared).toContain("${messagingMemberExists('messaging_team_members','team_id','t.id')}");
    expect(shared).not.toContain('${messagingTeamMemberExists}');
    expect(sourcePolicy.match(/messagingMemberExists\('messaging_room_members','room_id','r.id'\)/g) ?? []).toHaveLength(2);
    expect(sourcePolicy).not.toContain('${messagingRoomMemberExists}');
    expect(home).toContain("${messagingMemberExists('messaging_team_members','team_id','t.id')} ORDER BY t.name,t.id LIMIT 100");
    expect(sha(line(shared, 'export const messagingTeamAccessSql='))).toBe('d89b76e6b0632df460040fe917d87a5f68e3a12d32665808eb49a543d6d3d984');
    expect(sha(line(sourcePolicy, 'export function messagingRoomAccessSql'))).toBe('80fc3d08db9023a32af2ed4751a467f3a5f6310dae1f257ca34d9b58c5ed5b2f');
    expect(sha(line(home, '      AND ${messagingMemberExists('))).toBe('cb280b0fa7cf916d987b096b95e56c3e8862c35da301b9cb1c98cd14d7472a5c');
  });
});

describe('Collaboration read-tail r41 protected SQL isolation', () => {
  test('new renderers add no lock, write or notification statement', () => {
    for (const marker of [
      'export const socialTaskOriginBy=',
      'export const socialAiUsageRead=',
      'export const messagingMemberExists=',
    ]) {
      const added = line(shared, marker);
      expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      expect(added.match(/SELECT /g) ?? []).toHaveLength(1);
    }
  });

  test('agent-replies FOR UPDATE pair, locks, notification targets and field-outbox stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(shared).toContain('export const socialAiMonthlyUsageCount=');
    expect(shared).toContain('export const socialAgentReplyLeaseIdentity=');
  });
});

describe('Collaboration read-tail r41 KPI pins', () => {
  test('retirement:ts-statements is pinned at 302 after r44, below the r41 BEFORE=355 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Three 2->1 pair retirements, one SELECT head each: 355 -> 352. Prior
    // exact pins (r32-r40) were refreshed only where this value replaced 355.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(302);
    expect(total).toBeLessThan(355);
  });
});
