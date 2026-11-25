import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingTeamImage, socialPostDraftById, socialTargetImage } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const line = (body: string, marker: string) => body.split('\n').find(l => l.startsWith(marker))!;
const shared = src('read-tail-sql.ts');
const purchases = src('work-social-purchases.ts');
const handover = src('operations-handover-facts.ts');
const attachPurchases = src('messaging-attachment-purchases.ts');
const attachRead = src('messaging-attachment-read.ts');
const messaging = src('messaging-purchases.ts');

// Exact pre-r43 fragments at HEAD. Tails, the handover newline gap, aliases,
// casts, binds, locks and notification targets were never changed. The oracle
// SHAs pin the constants themselves so expectations cannot drift silently.
const DRAFT_ARM = "SELECT audience_id id FROM social_post_drafts WHERE id=? AND company_id=? AND audience_kind='project'";
const HANDOVER_DRAFT = 'SELECT body,participants_json,sources_json,published_post_id FROM social_post_drafts\n    WHERE id=? AND company_id=? AND author_id=?';
const TEAM_REDACTED = "(SELECT to_jsonb(t)-'version' FROM messaging_teams t WHERE t.id=r.team_id AND t.company_id=r.company_id)";
const TEAM_PLAIN = '(SELECT to_jsonb(t) FROM messaging_teams t WHERE t.id=r.team_id AND t.company_id=r.company_id)';
const TEAM_ROOM = '(SELECT to_jsonb(t) FROM messaging_teams t WHERE t.id=messagingRoomTeamId AND t.company_id=?)';
const TARGET_ROW = 'SELECT to_jsonb(row)::text image FROM ${table} row WHERE id=? AND company_id=?';
const TARGET_ROOT = 'SELECT to_jsonb(root) image FROM ${root} root WHERE root.id=? AND root.company_id=?';

describe('Collaboration read-tail r43 byte oracles', () => {
  test('the seven retired fragments are pinned byte for byte', () => {
    for (const [value, digest] of [
      [DRAFT_ARM, '06b7de32a7fc3b116a051070156a5d4ea6d1c20a3743004e2209ce75c85d9791'],
      [HANDOVER_DRAFT, '859ba73c594e8de74f160422d009db1784ade9b8ae9cb107d0e69c26e731be9b'],
      [TEAM_REDACTED, 'e4ce16e2a75a41f3a9e7da0d3a77a29226d3582cb96073803081ad5bb8b598b9'],
      [TEAM_PLAIN, '819ee73e7b4d0bda45b49188e9ecce716a3fc40e956556918b45655fd8aace59'],
      [TEAM_ROOM, '55727b4426637c831d1af231004c5fb98e9ce538b8d04eefd6efce33a92cc0fc'],
      [TARGET_ROW, 'c486a3fdbf0ad5c900df744a4dd707d48c589f89cd7ad02334b529fd956e13af'],
      [TARGET_ROOT, '19ec62ee907ebcc29ccc57dc466d586de9b285cd4f2f85e1bdeb787506c2f837'],
    ] as Array<[string, string]>) {
      expect(sha(value)).toBe(digest);
      expect(value.match(/SELECT /g)).toHaveLength(1);
    }
  });
});

describe('Collaboration read-tail r43 draft-by-id reuse family', () => {
  test('socialPostDraftById renders both retired heads with their own tails', () => {
    expect(socialPostDraftById('audience_id id',' '," AND audience_kind='project'")).toBe(DRAFT_ARM);
    expect(socialPostDraftById('body,participants_json,sources_json,published_post_id','\n    ',' AND author_id=?')).toBe(HANDOVER_DRAFT);
    expect(socialPostDraftById('*')).toBe('SELECT * FROM social_post_drafts WHERE id=? AND company_id=?');
    expect(socialPostDraftById('audience_id id',' '," AND audience_kind='project'").split('?').length - 1).toBe(2);
    expect(shared.match(/export const socialPostDraftById=/g) ?? []).toHaveLength(1);
    expect(sha(line(shared, 'export const socialPostDraftById='))).toBe('9962c290a602e5de5310ba0bb33ae9edd12c0f4db89acb0bb80b579f72814989');
  });

  test('both call sites keep their binds, newline gap and UNION tail', () => {
    expect(purchases).toContain("socialPostDraftById('audience_id id',' ',\" AND audience_kind='project'\")");
    expect(handover).toContain("socialPostDraftById('body,participants_json,sources_json,published_post_id','\\n    ',' AND author_id=?')");
    expect(purchases.match(/socialPostDraftById\('/g) ?? []).toHaveLength(2);
    expect(handover.match(/socialPostDraftById\('/g) ?? []).toHaveLength(1);
    expect(purchases).toContain("${await nativeInboxSqlFragment(actor.companyId,'work_social_audience_by_draft')}");
    expect(handover).toContain('.bind(value, companyId, userId)');
    // The UNION arm is now an owner-planned fragment; the draft-by-id head,
    // bind order and comma gap are unchanged.
    expect(sha(line(purchases, " const rows=command.type==='post'"))).toBe('149fb90cb4e498dc976fdbd88b5b13c40edc0e06c8c0cb426c84fa0a684590b4');
    expect(sha(line(handover, '            return database.prepare(socialPostDraftById('))).toBe('3cb56a561a0185c7b6ffe84ea2dc2f8db0dbac8b8e00654a347985870af9fb10');
    expect(purchases).not.toContain('"SELECT audience_id id FROM social_post_drafts WHERE id=?');
    expect(handover).not.toContain('`SELECT body,participants_json,sources_json,published_post_id FROM social_post_drafts');
  });
});

describe('Collaboration read-tail r43 messaging team image family', () => {
  test('one closed renderer reproduces the redacted and plain team subqueries', () => {
    expect(messagingTeamImage('r.team_id','r.company_id',true)).toBe(TEAM_REDACTED);
    expect(messagingTeamImage('r.team_id','r.company_id')).toBe(TEAM_PLAIN);
    expect(messagingTeamImage('messagingRoomTeamId','?')).toBe(TEAM_ROOM);
    expect(messagingTeamImage('r.team_id','r.company_id',true).match(/SELECT /g) ?? []).toHaveLength(1);
    expect(shared.match(/export const messagingTeamImage=/g) ?? []).toHaveLength(1);
    expect(shared).toContain("teamRef:string,companyRef:string,redact=false");
    expect(sha(line(shared, 'export const messagingTeamImage='))).toBe('0b8462fac51b955ac03ce908b9a50e358700589c0aa3a537b0427dec1e3983e9');
  });

  test('all three attachment/messaging call sites keep their team references', () => {
    expect(attachPurchases).toContain("${messagingTeamImage('r.team_id','r.company_id',true)}");
    expect(attachRead).toContain("${messagingTeamImage('r.team_id','r.company_id')}");
    expect(messaging).toContain("${messagingTeamImage(team?'?':messagingRoomTeamId,'?')}");
    expect(attachPurchases.match(/messagingTeamImage\(/g) ?? []).toHaveLength(1);
    expect(attachRead.match(/messagingTeamImage\(/g) ?? []).toHaveLength(1);
    expect(messaging.match(/messagingTeamImage\(/g) ?? []).toHaveLength(1);
    expect(sha(line(attachPurchases, 'const source='))).toBe('04c305a0264c0843d5f15bb2688aca6889b7123fc3d4325149791039ea59c64b');
    expect(sha(line(attachRead, 'async function image('))).toBe('f386557a513be7c9d7fb7373afbea47de998a067267d6e3f5c45f13c3240ee84');
    expect(sha(line(messaging, 'async function image('))).toBe('8108ac3f43724d472ad398fa8e0e2db5795b252b065a0a0ff4633e1c672d1916');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(attachPurchases).not.toContain("(SELECT to_jsonb(t)-'version' FROM messaging_teams t");
    expect(attachRead).not.toContain('(SELECT to_jsonb(t) FROM messaging_teams t');
  });
});

describe('Collaboration read-tail r43 social target image family', () => {
  test('one by-table renderer reproduces the alias and cast variants', () => {
    expect(socialTargetImage('${table}','to_jsonb(row)::text image','row')).toBe(TARGET_ROW);
    expect(socialTargetImage('${root}','to_jsonb(root) image','root','root')).toBe(TARGET_ROOT);
    expect(socialTargetImage('${table}','to_jsonb(row)::text image','row').match(/SELECT /g) ?? []).toHaveLength(1);
    expect(socialTargetImage('${root}','to_jsonb(root) image','root','root').match(/SELECT /g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialTargetImage=/g) ?? []).toHaveLength(1);
    expect(shared).toContain("table:string,cols:string,alias='',whereAlias='',gap=' ',tail=''");
    expect(sha(line(shared, 'export const socialTargetImage='))).toBe('85b5df731c67c23e797568e8ea4cd3c89f6a67a4bbda1769d522b7a730b8f6dc');
  });

  test('both purchase call sites keep their closed tables and bindings', () => {
    expect(purchases).toContain("socialTargetImage(table,'to_jsonb(row)::text image','row')");
    expect(purchases).toContain("socialTargetImage(root,'to_jsonb(root) image','root','root')");
    expect(purchases.match(/socialTargetImage\(/g) ?? []).toHaveLength(2);
    expect(purchases).toContain("command.type==='draft'?'social_post_drafts':'social_stories'");
    expect(purchases).toContain('WITH native_root AS(');
    expect(sha(line(purchases, 'async function target('))).toBe('73b51eec4998cae9c991e071a92737cfac4f557bf06b6c6f4fedb369e79e670b');
    expect(sha(line(purchases, ' const row=await database.prepare(`WITH native_root'))).toBe('58724bddd0852b547108518d4059c4252572186ef84feac2df320257b941665b');
    expect(purchases).not.toContain('SELECT to_jsonb(row)::text image FROM ${table}');
    expect(purchases).not.toContain('SELECT to_jsonb(root) image FROM ${root}');
  });
});

describe('Collaboration read-tail r43 protected SQL isolation', () => {
  test('new renderers add no lock, write or notification statement', () => {
    for (const marker of [
      'export const socialPostDraftById=',
      'export const messagingTeamImage=',
      'export const socialTargetImage=',
    ]) {
      const added = line(shared, marker);
      expect(added).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      expect(added.match(/SELECT /g) ?? []).toHaveLength(marker.includes('socialPostDraftById') ? 0 : 1);
    }
  });

  test('agent-reply locks, notification targets and r42 comment helpers stay unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(sha(line(shared, 'export const socialCommentHead='))).toBe('2ad63e64db338d62db117ee74155b7ea3281d741f48b4b6811ba884cc24b0d92');
    expect(sha(line(shared, 'export const socialCommentProjection='))).toBe('e81afcc981c3c2bf41540a33fba9728e1870f57ec62b9fc4ffc2bb6771cc65c4');
    const claim = purchases.split('\n').find(l => l.includes("SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending'"))!;
    const finalize = purchases.split('\n').find(l => l.includes('SELECT count(*)::int n FROM social_agent_replies pending JOIN'))!;
    expect(sha(claim)).toBe('a08538190a67e554b1b1f6e725aa69ff9572e64abb28415dfffb2a678af93be1');
    expect(sha(finalize)).toBe('3cd3b4bb4504c1a15857a9ce51e8db1b4ebddde5676c45432bf29ad9a625a0de');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Collaboration read-tail r43 KPI pins', () => {
  test('retirement:ts-statements is pinned at 247, below the r44 BEFORE=344 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Draft-by-id reuse (-2), messaging team image 3->1 (-2) and social target
    // image 2->1 (-2): 350 -> 344. Prior exact pins were refreshed only where
    // 344 replaced 350; locks and notification targets were not touched.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(247);
    expect(total).toBeLessThan(344);
  });
});
