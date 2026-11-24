import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingRoomByIdSql, socialAgentReplyById, attachmentVersionGuard, commandReceiptById } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const messaging = src('messaging.ts');
const messagingRead = src('messaging-read.ts');
const agent = src('social-agent.ts');
const portalFacts = src('attachments-portal-facts.ts');
const serviceFacts = src('attachments-service-facts.ts');
const purchases = src('work-social-purchases.ts');

// r33 generalizes four byte-identical heads: the access-guarded room-by-id read,
// the agent-reply identity read, the attachment owner-version guard and the
// command-receipt id read. Only projected columns / target table / alias vary;
// bind order, lock statements and notification helpers are unchanged. Every
// rendered string below equals the pre-dedupe inline literal byte for byte.
const ACCESS = '<ACCESS>';

describe('Collaboration read-tail r33 parameterized families', () => {
  test('messagingRoomByIdSql renders both access-guarded room reads byte-identically', () => {
    expect(messagingRoomByIdSql('1', ACCESS)).toBe(`SELECT 1 FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${ACCESS}`);
    expect(messagingRoomByIdSql('r.*', ACCESS)).toBe(`SELECT r.* FROM messaging_rooms r WHERE r.id=? AND r.company_id=? AND ${ACCESS}`);
    expect(shared.match(/export const messagingRoomByIdSql=/g) ?? []).toHaveLength(1);
    expect(messaging).toContain("EXISTS(${messagingRoomByIdSql('1',access)})");
    expect(messagingRead).toContain("messagingRoomByIdSql('r.*',access)");
    expect(messaging).not.toContain('EXISTS(SELECT 1 FROM messaging_rooms r WHERE r.id=? AND r.company_id=?');
    expect(messagingRead).not.toContain('SELECT r.* FROM messaging_rooms r WHERE r.id=? AND r.company_id=?');
  });

  test('socialAgentReplyById renders both agent-reply identity reads byte-identically', () => {
    expect(socialAgentReplyById('*')).toBe('SELECT * FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?');
    expect(socialAgentReplyById('post_id')).toBe('SELECT post_id FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?');
    expect(shared.match(/export const socialAgentReplyById=/g) ?? []).toHaveLength(1);
    expect(agent).toContain("db.prepare(socialAgentReplyById('*'))");
    expect(agent).toContain("db.prepare(socialAgentReplyById('post_id'))");
    expect(agent).not.toContain('SELECT * FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?');
    expect(agent).not.toContain('SELECT post_id FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=?');
  });

  test('attachmentVersionGuard renders both owner-version guards byte-identically', () => {
    expect(attachmentVersionGuard('social_posts', " AND state='published'")).toBe("EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=? AND state='published')");
    expect(attachmentVersionGuard('social_post_drafts', '')).toBe('EXISTS(SELECT 1 FROM social_post_drafts WHERE id=? AND company_id=? AND version=?)');
    expect(attachmentVersionGuard('messaging_rooms', '')).toBe('EXISTS(SELECT 1 FROM messaging_rooms WHERE id=? AND company_id=? AND version=?)');
    expect(shared.match(/export const attachmentVersionGuard=/g) ?? []).toHaveLength(1);
    expect(portalFacts).toContain("attachmentVersionGuard('social_posts', \" AND state='published'\")");
    expect(serviceFacts).toContain('attachmentVersionGuard(item.table, item.extra ? ` AND ${item.extra}` : \'\')');
    expect(portalFacts).not.toContain("EXISTS(SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND version=?");
    expect(serviceFacts).not.toContain('EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?');
  });

  test('commandReceiptById renders four receipt-by-command-id reads byte-identically', () => {
    expect(commandReceiptById('operation,actor_id,record_id')).toBe('SELECT operation,actor_id,record_id FROM command_receipts WHERE command_id=?');
    expect(commandReceiptById('to_jsonb(c)::text image', 'c')).toBe('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?');
    expect(commandReceiptById('operation,actor_id,record_id,to_jsonb(c)::text image', 'c')).toBe('SELECT operation,actor_id,record_id,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?');
    expect(commandReceiptById("operation,actor_id,record_id,authority_json::jsonb->>'companyId' company_id,result_json,to_jsonb(c)::text image", 'c')).toBe("SELECT operation,actor_id,record_id,authority_json::jsonb->>'companyId' company_id,result_json,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?");
    expect(shared.match(/export const commandReceiptById=/g) ?? []).toHaveLength(1);
    expect(purchases).toContain("commandReceiptById('operation,actor_id,record_id')");
    expect(purchases).toContain("commandReceiptById('to_jsonb(c)::text image','c')");
    expect(purchases).toContain("commandReceiptById('operation,actor_id,record_id,to_jsonb(c)::text image','c')");
    expect(purchases).toContain("commandReceiptById(\"operation,actor_id,record_id,authority_json::jsonb->>'companyId' company_id,result_json,to_jsonb(c)::text image\",'c')");
    expect(purchases).not.toContain('SELECT operation,actor_id,record_id FROM command_receipts WHERE command_id=?');
    expect(purchases).not.toContain('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?');
    expect(purchases).not.toContain('SELECT operation,actor_id,record_id,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?');
    expect(purchases).not.toContain("SELECT operation,actor_id,record_id,authority_json::jsonb->>'companyId' company_id,result_json,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?");
  });
});

describe('Collaboration read-tail r33 lock/notification isolation', () => {
  test('lock statements and notification helpers stay untouched', () => {
    const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/;
    for (const sql of [
      messagingRoomByIdSql('1', ACCESS),
      messagingRoomByIdSql('r.*', ACCESS),
      socialAgentReplyById('*'),
      socialAgentReplyById('post_id'),
      attachmentVersionGuard('social_posts', " AND state='published'"),
      attachmentVersionGuard('messaging_rooms', ''),
      commandReceiptById('operation,actor_id,record_id'),
      commandReceiptById('to_jsonb(c)::text image', 'c'),
    ]) expect(sql).not.toMatch(lockRe);
    expect(agent).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
    expect(purchases).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(src('work-task-purchase-source.ts')).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(src('messaging-purchases.ts')).toContain('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE');
    expect(src('staff-notifications.ts')).toContain('legacy_rows AS');
  });
});

describe('Collaboration read-tail r33 KPI pins', () => {
  test('retirement:ts-statements is pinned at 350 after r42, below the r33 BEFORE=381 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r33 deduped four heads: access-guarded room-by-id 2->1 (-1), agent-reply
    // identity 2->1 (-1), attachment owner-version guard 2->1 (-1) and
    // command-receipt id 4->1 (-3). Net -6 SELECT tokens: 381 -> 375. r34
    // dedupes five task-origin guards into one head for -4 more: 375 -> 371.
    // No lock statement or notification helper was modified.
    // r35 refreshed this pin after the 371 -> 370 notification CTE dedupe;
    // r37 refreshed it again after the 370 -> 364 revisions/messaging/PMS dedupe;
    // r38 refreshed it after the 364 -> 360 settlement/AI-draft/post-head dedupe;
    // r39 refreshed it after the 360 -> 356 queued-guard/actor-CTE/requested-select dedupe.
    // r40 refreshed it after the 356 -> 355 source-rows absence guard dedupe.
    // r41 refreshed it after the 355 -> 352 task-origin/by-key, AI-usage row and
    // team/room member-existence dedupe.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    expect(total).toBeLessThanOrEqual(350);
    expect(total).toBeLessThan(375);
  });
});
