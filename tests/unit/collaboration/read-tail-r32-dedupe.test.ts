import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { publishedCommentRead, socialPostDraftById, socialAiMonthlyUsageCount } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const agent = src('social-agent.ts');
const socialHome = src('social-home.ts');
const homeRead = src('social-home-read.ts');
const purchases = src('work-social-purchases.ts');
const policy = src('work-social-draft-policy.ts');
const beta = src('beta-management-sources.ts');
const recordsCount = src('records-ai-usage-projection.ts');
const budget = src('social-ai-budget-projections.ts');
const agentUsage = src('agent-ai-usage-projection.ts');

// r32 generalizes three byte-identical heads already present in read-tail-sql.ts.
// Only the projected column list varies; the WHERE prefix, `?` bind order, lock
// statements and notification helpers are unchanged. Each rendered string below
// equals the pre-dedupe inline literal byte for byte.
const COMMENT_HEAD = " FROM social_comments WHERE id=? AND post_id=? AND state='published'";
const DRAFT_HEAD = ' FROM social_post_drafts WHERE id=? AND company_id=?';
const AI_HEAD = ' FROM social_ai_usage WHERE company_id=? AND month=?';

describe('Collaboration read-tail r32 parameterized families', () => {
  test('publishedCommentRead renders the four published-comment reads byte-identically', () => {
    expect(publishedCommentRead('1')).toBe('SELECT 1' + COMMENT_HEAD);
    expect(publishedCommentRead('id')).toBe('SELECT id' + COMMENT_HEAD);
    expect(publishedCommentRead('rich_content_json')).toBe('SELECT rich_content_json' + COMMENT_HEAD);
    expect(publishedCommentRead('id,author_id,parent_comment_id')).toBe('SELECT id,author_id,parent_comment_id' + COMMENT_HEAD);
    expect(shared.match(/export const publishedCommentRead=/g) ?? []).toHaveLength(1);
    expect(agent).toContain("publishedCommentRead('id')");
    expect(agent).toContain("publishedCommentRead('rich_content_json')");
    expect(socialHome).toContain("publishedCommentRead('id,author_id,parent_comment_id')");
    expect(socialHome).toContain("EXISTS(${publishedCommentRead('1')})");
    expect(homeRead).toContain("publishedCommentRead('1')");
    expect(agent).not.toContain('SELECT id FROM social_comments WHERE id=? AND post_id=?');
    expect(agent).not.toContain('SELECT rich_content_json FROM social_comments WHERE id=? AND post_id=?');
    expect(socialHome).not.toContain("SELECT id,author_id,parent_comment_id FROM social_comments WHERE id=? AND post_id=?");
  });

  test('socialPostDraftById renders the four draft-identity reads byte-identically', () => {
    expect(socialPostDraftById('*')).toBe('SELECT *' + DRAFT_HEAD);
    expect(socialPostDraftById('COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id')).toBe('SELECT COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id' + DRAFT_HEAD);
    expect(socialPostDraftById('sources_json,COALESCE(comment_parent_post_id,edit_post_id,published_post_id) post_id')).toBe('SELECT sources_json,COALESCE(comment_parent_post_id,edit_post_id,published_post_id) post_id' + DRAFT_HEAD);
    expect(socialPostDraftById('comment_parent_post_id,edit_post_id,published_post_id')).toBe('SELECT comment_parent_post_id,edit_post_id,published_post_id' + DRAFT_HEAD);
    expect(shared.match(/export const socialPostDraftById=/g) ?? []).toHaveLength(1);
    expect(socialHome).toContain("socialPostDraftById('*')");
    expect(homeRead).toContain("socialPostDraftById('*')");
    expect(purchases).toContain("socialPostDraftById('COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id')");
    expect(policy).toContain("socialPostDraftById('sources_json,COALESCE(comment_parent_post_id,edit_post_id,published_post_id) post_id')");
    expect(policy).toContain("socialPostDraftById('comment_parent_post_id,edit_post_id,published_post_id')");
    expect(purchases).not.toContain('SELECT COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id FROM social_post_drafts WHERE id=?');
    expect(policy).not.toContain('SELECT comment_parent_post_id,edit_post_id,published_post_id FROM social_post_drafts WHERE id=?');
  });

  test('socialAiMonthlyUsageCount renders the three monthly-usage reads byte-identically', () => {
    expect(socialAiMonthlyUsageCount('count(*)')).toBe('SELECT count(*)' + AI_HEAD);
    expect(socialAiMonthlyUsageCount('COUNT(*)')).toBe('SELECT COUNT(*)' + AI_HEAD);
    expect(socialAiMonthlyUsageCount('input_tokens,output_tokens')).toBe('SELECT input_tokens,output_tokens' + AI_HEAD);
    expect(shared.match(/export const socialAiMonthlyUsageCount=/g) ?? []).toHaveLength(1);
    expect(beta).toContain("socialAiMonthlyUsageCount('input_tokens,output_tokens')");
    expect(recordsCount).toContain("socialAiMonthlyUsageCount('COUNT(*)')");
    expect(budget).toContain("socialAiMonthlyUsageCount('count(*)')");
    expect(agentUsage).toContain("socialAiMonthlyUsageCount('count(*)')");
    expect(beta).not.toContain('SELECT input_tokens,output_tokens FROM social_ai_usage WHERE company_id=?');
    expect(recordsCount).not.toContain('SELECT COUNT(*) FROM social_ai_usage WHERE company_id=?');
  });
});

describe('Collaboration read-tail r32 lock/notification isolation', () => {
  test('lock statements and notification helpers stay untouched', () => {
    const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/;
    for (const sql of [
      publishedCommentRead('id'),
      publishedCommentRead('1'),
      socialPostDraftById('*'),
      socialPostDraftById('comment_parent_post_id,edit_post_id,published_post_id'),
      socialAiMonthlyUsageCount('count(*)'),
      socialAiMonthlyUsageCount('input_tokens,output_tokens'),
    ]) expect(sql).not.toMatch(lockRe);
    expect(agent).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(src('work-task-purchase-source.ts')).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(src('messaging-purchases.ts')).toContain('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE');
    expect(src('staff-notifications.ts')).toContain('legacy_rows AS');
  });
});

describe('Collaboration read-tail r32 KPI pins', () => {
  test('retirement:ts-statements is pinned at 344 after r43, below the r32 BEFORE=389 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r32 deduped three families (published-comment read 4->1, draft identity
    // 4->1, monthly AI usage 3->1) for -8 tokens: 389 -> 381. r33 dedupes four
    // more heads (room-by-id, agent-reply id, attachment version guard,
    // command-receipt id) for -6 more: 381 -> 375. r34 dedupes five task-origin
    // existence guards into one head for -4 more: 375 -> 371.
    // No lock statement or notification helper was modified.
    // r35 refreshed this pin after the 371 -> 370 notification CTE dedupe;
    // r37 refreshed it again after the 370 -> 364 revisions/messaging/PMS dedupe;
    // r38 refreshed it after the 364 -> 360 settlement/AI-draft/post-head dedupe;
    // r39 refreshed it after the 360 -> 356 queued-guard/actor-CTE/requested-select dedupe.
    // r40 refreshed it after the 356 -> 355 source-rows absence guard dedupe.
    // r41 refreshed it after the 355 -> 352 task-origin/by-key, AI-usage row and
    // team/room member-existence dedupe.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    expect(total).toBeLessThanOrEqual(344);
    expect(total).toBeLessThan(375);
  });
});
