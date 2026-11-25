import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialTaskOriginGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const shared = src('read-tail-sql.ts');
const queued = src('work-task-queued-read-sources.ts');
const policy = src('work-task-read-policy.ts');
const purchase = src('work-task-purchase-source.ts');

// r34 generalizes five byte-identical EXISTS/NOT EXISTS task-origin heads in the
// queued read sources (3), the Task read policy (1) and the purchase source (1).
// Only the quantifier, origin alias and joined tail vary. Every rendered string
// below equals its pre-dedupe inline literal byte for byte; binds, locks and the
// retained r14 socialTaskOriginPostExists helper are unchanged.
describe('Collaboration read-tail r34 parameterized families', () => {
  test('socialTaskOriginGuard renders all five task-origin heads byte-identically', () => {
    expect(socialTaskOriginGuard('NOT EXISTS(', 'queued_origin', ' LEFT JOIN work_queued_social_post_rights rights ON rights.post_id=queued_origin.post_id AND rights.company_id=t.company_id AND rights.assignment_id=work_alert_viewer.assignment_id WHERE queued_origin.task_id=t.id AND COALESCE(rights.readable,false)=false)')).toBe(`NOT EXISTS(SELECT 1 FROM social_task_origins queued_origin LEFT JOIN work_queued_social_post_rights rights ON rights.post_id=queued_origin.post_id AND rights.company_id=t.company_id AND rights.assignment_id=work_alert_viewer.assignment_id WHERE queued_origin.task_id=t.id AND COALESCE(rights.readable,false)=false)`);
    expect(socialTaskOriginGuard('EXISTS(', 'origin', ' JOIN work_queued_social_tasks task ON task.id=origin.task_id AND task.company_id=p.company_id WHERE origin.post_id=p.id)')).toBe(`EXISTS(SELECT 1 FROM social_task_origins origin JOIN work_queued_social_tasks task ON task.id=origin.task_id AND task.company_id=p.company_id WHERE origin.post_id=p.id)`);
    expect(socialTaskOriginGuard('EXISTS(', 'source_origin', ' JOIN work_queued_social_posts source_post ON source_post.id=source_origin.post_id AND source_post.company_id=t.company_id WHERE source_origin.task_id=t.id AND source_post.native_social)')).toBe(`EXISTS(SELECT 1 FROM social_task_origins source_origin JOIN work_queued_social_posts source_post ON source_post.id=source_origin.post_id AND source_post.company_id=t.company_id WHERE source_origin.task_id=t.id AND source_post.native_social)`);
    expect(socialTaskOriginGuard('NOT EXISTS(', 'task_read_origin', `\n LEFT JOIN social_posts p ON p.id=task_read_origin.post_id AND p.company_id=\${taskId===undefined?'t.company_id':'?'}\n WHERE task_read_origin.task_id=\${taskId===undefined?'t.id':'?'} AND (p.id IS NULL OR p.state<>'published'\n OR NOT EXISTS(\${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE \${scope.where}\n AND \${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})})))`)).toBe(`NOT EXISTS(SELECT 1 FROM social_task_origins task_read_origin
 LEFT JOIN social_posts p ON p.id=task_read_origin.post_id AND p.company_id=\${taskId===undefined?'t.company_id':'?'}
 WHERE task_read_origin.task_id=\${taskId===undefined?'t.id':'?'} AND (p.id IS NULL OR p.state<>'published'
 OR NOT EXISTS(\${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE \${scope.where}
 AND \${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})})))`);
    expect(socialTaskOriginGuard('EXISTS(', 'origin', ' JOIN social_posts post ON post.id=origin.post_id AND post.company_id=t.company_id WHERE origin.task_id=t.id AND ${workSocialNativePostPredicate(\'post\')})')).toBe(`EXISTS(SELECT 1 FROM social_task_origins origin JOIN social_posts post ON post.id=origin.post_id AND post.company_id=t.company_id WHERE origin.task_id=t.id AND \${workSocialNativePostPredicate('post')})`);
    expect(shared.match(/export const socialTaskOriginGuard=/g) ?? []).toHaveLength(1);
  });

  test('all five call sites use the shared head and dropped their inline heads', () => {
    expect(queued).toContain("socialTaskOriginGuard('NOT EXISTS(','queued_origin',");
    expect(queued).toContain("socialTaskOriginGuard('EXISTS(','origin',");
    expect(queued).toContain("socialTaskOriginGuard('EXISTS(','source_origin',");
    expect(policy).toContain("socialTaskOriginGuard('NOT EXISTS(','task_read_origin',");
    expect(purchase).toContain("socialTaskOriginGuard('EXISTS(','origin',");
    expect(queued).not.toContain('NOT EXISTS(SELECT 1 FROM social_task_origins queued_origin');
    expect(queued).not.toContain('EXISTS(SELECT 1 FROM social_task_origins origin JOIN');
    expect(queued).not.toContain('EXISTS(SELECT 1 FROM social_task_origins source_origin');
    expect(policy).not.toContain('NOT EXISTS(SELECT 1 FROM social_task_origins task_read_origin');
    expect(purchase).not.toContain('EXISTS(SELECT 1 FROM social_task_origins origin JOIN');
  });

  test('the r14 socialTaskOriginPostExists helper stays retained and unchanged', () => {
    expect(shared).toContain('export const socialTaskOriginPostExists=`EXISTS(SELECT 1 FROM social_task_origins origin WHERE origin.task_id=t.id AND origin.post_id=post.id)`;');
    expect(purchase.match(/socialTaskOriginPostExists/g) ?? []).toHaveLength(5);
  });
});

describe('Collaboration read-tail r34 lock/notification isolation', () => {
  test('locks, binds and notification helpers stay untouched', () => {
    const lockRe = /FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/;
    for (const sql of [
      socialTaskOriginGuard('NOT EXISTS(', 'queued_origin', ' tail)'),
      socialTaskOriginGuard('EXISTS(', 'origin', ' tail)'),
      socialTaskOriginGuard('NOT EXISTS(', 'task_read_origin', ' tail)'),
      socialTaskOriginGuard('EXISTS(', 'source_origin', ' tail)'),
    ]) expect(sql).not.toMatch(lockRe);
    expect(purchase).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(purchase).toContain('LOCK TABLE messaging_teams,messaging_team_members IN SHARE MODE');
    expect(policy).toContain('args:taskId===undefined?args:[actor.companyId,taskId,...args]');
    expect(queued).toContain('args:SqlValue[]=target?[target.id,target.companyId]:[input.deadline]');
    expect(src('staff-notifications.ts')).toContain('legacy_rows AS');
  });
});

describe('Collaboration read-tail r34 KPI pins', () => {
  test('retirement:ts-statements is pinned at 308 after r44, below the r34 BEFORE=375 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Five task-origin existence guards (queued 3, read policy 1, purchase 1)
    // share one parameterized head: 5 -> 1, net -4 SELECT tokens: 375 -> 371.
    // No lock statement, bind or notification helper was modified.
    // r35 refreshed this pin after the 371 -> 370 notification CTE dedupe;
    // r37 refreshed it again after the 370 -> 364 revisions/messaging/PMS dedupe;
    // r38 refreshed it after the 364 -> 360 settlement/AI-draft/post-head dedupe;
    // r39 refreshed it after the 360 -> 356 queued-guard/actor-CTE/requested-select dedupe.
    // r40 refreshed it after the 356 -> 355 source-rows absence guard dedupe.
    // r41 refreshed it after the 355 -> 352 task-origin/by-key, AI-usage row and
    // team/room member-existence dedupe.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(308);
    expect(total).toBeLessThan(375);
  });
});
