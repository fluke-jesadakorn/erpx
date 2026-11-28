import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingTeamMembersAgg, socialFollowPostExists, socialPostByIdPredicate, socialPostRecipientExists } from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const productEntry = src('product-entry.ts');
const socialSource = src('social-source.ts');
const socialHome = src('social-home.ts');
const readPolicy = src('work-social-read-policy.ts');
const receipts = src('handover-receipts.ts');
const obligations = src('work-package-obligations.ts');
const messaging = src('messaging-purchases.ts');
const social = src('social.ts');

// r30 generalizes two existing fragments and adds one, then reuses the existing
// post-by-id predicate head. Every rendering is byte-identical to the pre-dedupe
// inline literal; only the varying tail/predicate is parameterized.
const HEAD = 'SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ';
const ACK_TAIL = 'NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements acknowledgement WHERE acknowledgement.post_id=p.id AND acknowledgement.user_id=recipient.user_id AND acknowledgement.post_version=p.version)';

describe('Collaboration read-tail r30 parameterized families', () => {
  test('socialPostByIdPredicate is reused at all four inline call sites', () => {
    const predicate = "p.version=? AND p.state='published' AND sourceAccess()";
    expect(socialPostByIdPredicate(predicate)).toBe(HEAD + predicate);
    expect(socialPostByIdPredicate('scope.where')).toBe(HEAD + 'scope.where');
    for (const file of [productEntry, socialSource, socialHome, readPolicy]) {
      expect(file).toContain('${socialPostByIdPredicate(');
      expect(file).not.toContain(HEAD);
    }
  });

  test('socialPostRecipientExists stays byte-identical to both handover literals', () => {
    expect(socialPostRecipientExists('recipient.user_id=?')).toBe('EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND recipient.user_id=?)');
    expect(socialPostRecipientExists(ACK_TAIL)).toBe(`EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND ${ACK_TAIL})`);
    expect(shared.match(/export const socialPostRecipientExists=/g) ?? []).toHaveLength(1);
    expect(receipts).toContain("${socialPostRecipientExists('recipient.user_id=?')}");
    expect(obligations).toContain('${socialPostRecipientExists(\'NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements');
    expect(receipts).not.toContain('EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND recipient.user_id=?)');
    expect(obligations).not.toContain('EXISTS(SELECT 1 FROM social_post_participants recipient WHERE recipient.post_id=p.id AND recipient.user_id<>p.author_id AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements');
  });

  test('messagingTeamMembersAgg is generalized without changing renderings', () => {
    expect(messagingTeamMembersAgg('r.team_id')).toBe('(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=r.team_id)');
    expect(messagingTeamMembersAgg('?')).toBe('(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=?)');
    expect(shared.match(/export const messagingTeamMembersAgg=/g) ?? []).toHaveLength(1);
    expect(messaging).toContain("${messagingTeamMembersAgg(team?'?':messagingRoomTeamId)}");
    expect(messaging).not.toContain('(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=${team?');
  });

  test('socialFollowPostExists is generalized without changing renderings', () => {
    expect(socialFollowPostExists("'post'", 'p.id')).toBe("EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='post' AND f.target_id=p.id)");
    expect(socialFollowPostExists("'person'", 'p.author_id')).toBe("EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='person' AND f.target_id=p.author_id)");
    expect(shared.match(/export const socialFollowPostExists=/g) ?? []).toHaveLength(1);
    expect(social).toContain('${socialFollowPostExists("\'person\'",\'p.author_id\')}');
    expect(socialHome).toContain('${socialFollowPostExists("\'post\'",\'p.id\')}');
    expect(social).not.toContain("EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=? AND f.target_type='person'");
  });
});

describe('Collaboration read-tail r30 lock/notification isolation', () => {
  test('lock statements and notification helpers stay untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const sql of [
      socialPostByIdPredicate('scope.where'),
      socialPostRecipientExists(ACK_TAIL),
      messagingTeamMembersAgg('?'),
      socialFollowPostExists("'person'", 'p.author_id'),
    ]) expect(sql).not.toMatch(lockRe);
    expect(src('work-social-purchases.ts')).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(src('work-task-purchase-source.ts')).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(messaging).toContain('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE');
    expect(src('staff-notifications.ts')).toContain('legacy_rows AS');
    expect(src('web-push.ts')).not.toContain('SELECT ');
    expect(src('notification_statement.rs')).toContain('"web_push_current_delivery" => "SELECT j.*,s.user_id');
  });
});

describe('Collaboration read-tail r30 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=397 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r30 deduped four families: the post-by-id head at four inline sites (-4),
    // the handover recipient guard at two sites (-1), the messaging team-member
    // aggregate (-1) and the social-follow post guard (-1). Net -7 SELECT tokens;
    // no lock statement or notification helper was modified.
    expect(total).toBeLessThanOrEqual(390);
    expect(total).toBeLessThan(397);
  });
});
