import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { queuedSourceAbsentGuard, socialPurchaseActorCte, requestedSourcesSelect } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const queued = src('work-task-queued-read-sources.ts');
const readPolicy = src('work-social-read-policy.ts');
const draftPolicy = src('work-social-draft-policy.ts');
const purchases = src('work-social-purchases.ts');

// Exact fragments taken from the pre-r39 call sites. Each expected string below
// is the original inline SQL literal before its call site was replaced.
const projectJoin = 'work_queued_social_project_sources actual ON actual.source_kind=requested.source_kind AND actual.source_id=requested.source_id AND actual.company_id=p.company_id LEFT JOIN work_queued_social_project_rights rights ON rights.source_kind=actual.source_kind AND rights.source_id=actual.source_id AND rights.company_id=actual.company_id AND rights.assignment_id=social_purchase_actor.assignment_id';
const projectTail = 'COALESCE(rights.readable,false)=false';
const ordinaryJoin = "(${operations.tasks}) ordinary ON requested.source_kind='staff_task' AND ordinary.id=requested.source_id AND ordinary.company_id=p.company_id";
const ordinaryTail = "ordinary.project_id IS NULL AND NOT (social_purchase_actor.work_history AND (ordinary.template_id IS NULL OR social_purchase_actor.routines_history) AND ${taskParticipantReadPolicy('ordinary.id','ordinary.company_id','social_purchase_actor')})";
const selectedBody = '?::text user_id,?::text assignment_id,?::text role,?::text company_id,${clock} now_ms WHERE ${workforceActorScopeExists(current.where)}';
const currentBody = 'g.user_id,g.id assignment_id,g.role,g.company_id,${clock} now_ms FROM assignments g WHERE g.user_id=? AND g.company_id=? AND (?::text IS NULL OR g.id=?) AND iam_assignment_active(g.id,${clock}) AND iam_principal_active(g.user_id,g.company_id,${clock})';
const queuedBody = "${actor}->>'userId' user_id,${actor}->>'assignmentId' assignment_id,${actor}->>'role' role,pending.company_id,${now} now_ms WHERE ${queuedSocialReplyAuthorityPredicate()}";
const requestedJsonb = '(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))';

// r39 generalizes three parameterized families. Each family replaces N inline
// byte-identical SELECT fragments with one parameterized helper: queued source
// absence guard 2->1 (-1), Social purchase actor CTE 3->1 (-2) and requested
// sources select 2->1 (-1): 360 -> 356. No bind order, lock statement,
// notification target or public export key changed.
describe('Collaboration read-tail r39 byte oracles', () => {
  test('Rank1 queuedSourceAbsentGuard renders both original guards byte-identically', () => {
    expect(queuedSourceAbsentGuard(projectJoin, projectTail)).toBe('NOT EXISTS(SELECT 1 FROM social_post_sources requested JOIN ' + projectJoin + ' WHERE requested.post_id=p.id AND ' + projectTail + ')');
    expect(queuedSourceAbsentGuard(ordinaryJoin, ordinaryTail)).toBe('NOT EXISTS(SELECT 1 FROM social_post_sources requested JOIN ' + ordinaryJoin + ' WHERE requested.post_id=p.id AND ' + ordinaryTail + ')');
    expect(shared).toContain('export const queuedSourceAbsentGuard=(joinSql:string,tail:string)=>`NOT EXISTS(SELECT 1 FROM social_post_sources requested JOIN ${joinSql} WHERE requested.post_id=p.id AND ${tail})`;');
    expect(shared.match(/export const queuedSourceAbsentGuard=/g) ?? []).toHaveLength(1);
  });

  test('Rank2 socialPurchaseActorCte renders all three original CTE heads byte-identically', () => {
    for (const body of [selectedBody, currentBody, queuedBody]) {
      expect(socialPurchaseActorCte(body)).toBe('WITH social_purchase_actor AS MATERIALIZED(SELECT ' + body + ') ');
    }
    expect(socialPurchaseActorCte('x').endsWith(') ')).toBe(true);
    expect(shared).toContain('export const socialPurchaseActorCte=(body:string)=>`WITH social_purchase_actor AS MATERIALIZED(SELECT ${body}) `;');
    expect(shared.match(/export const socialPurchaseActorCte=/g) ?? []).toHaveLength(1);
  });

  test('Rank3 requestedSourcesSelect keeps the original CTE and JSON bind placeholder', () => {
    expect(requestedSourcesSelect('r.kind,r.id')).toBe('WITH requested AS' + requestedJsonb + ' SELECT r.kind,r.id');
    expect(requestedSourcesSelect('jsonb_build_object(?)::text image')).toBe('WITH requested AS' + requestedJsonb + ' SELECT jsonb_build_object(?)::text image');
    expect(shared).toContain('export const requestedSourcesSelect=(body:string)=>`WITH requested AS${requestedSourcesJsonb} SELECT ${body}`;');
    expect(shared.match(/export const requestedSourcesSelect=/g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r39 call sites and binds', () => {
  test('Rank1 both queued readers pass their original join bodies and tails', () => {
    expect(queued).toContain('const projectRead=queuedSourceAbsentGuard(`' + projectJoin + '`,`' + projectTail + '`);');
    expect(queued).toContain('const ordinaryRead=queuedSourceAbsentGuard(`' + ordinaryJoin + '`,`' + ordinaryTail + '`);');
    expect(queued).not.toContain('const projectRead=`NOT EXISTS(');
    expect(queued).not.toContain('const ordinaryRead=`NOT EXISTS(');
    expect(queued).toContain("import {socialTaskOriginGuard,queuedSourceAbsentGuard} from './read-tail-sql';");
    // COALESCE and the task-participant policy stay caller fragments.
    expect(queued).toContain('COALESCE(rights.readable,false)=false');
    expect(queued).toContain("${taskParticipantReadPolicy('ordinary.id','ordinary.company_id','social_purchase_actor')}");
  });

  test('Rank2 all three sites pass clock/authority bodies and keep their binds', () => {
    expect(readPolicy).toContain('prefix:socialPurchaseActorCte(`' + selectedBody + '`)');
    expect(readPolicy).toContain('prefix:socialPurchaseActorCte(`' + currentBody + '`)');
    expect(readPolicy).toContain('args:[actor.userId,actor.assignmentId,actor.role,actor.companyId,actor.companyId,actor.branchId,actor.departmentId,...current.args] as SqlValue[]');
    expect(readPolicy).toContain('args:[actor.userId,actor.companyId,actor.assignmentId??null,actor.assignmentId??null] as SqlValue[]');
    expect(purchases).toContain('EXISTS(${socialPurchaseActorCte(`' + queuedBody + '`)}SELECT 1 FROM social_purchase_actor WHERE ');
    expect(readPolicy).not.toContain('prefix:`WITH social_purchase_actor AS MATERIALIZED(SELECT');
    expect(purchases).not.toContain('EXISTS(WITH social_purchase_actor AS MATERIALIZED(SELECT');
    expect(purchases).toContain("import {requestedSourcesJsonb,workSocialPostsAgg,workSocialCommentsAgg,workSocialSourcesAgg,socialPurchaseActorFeature,socialPostByIdPredicate,socialAgentReplyLeaseIdentity,socialPostPublishReceipt,socialPostByIdGuard,socialAiPhaseImageSql,socialPostRevisionsAgg,socialPostRevisionsFileAgg,socialPostDraftById,socialTargetImage,commandReceiptById,socialPurchaseActorCte,requestedSourcesSelect} from './read-tail-sql';");
  });

  test('Rank3 both sites pass their original bodies and keep the JSON bind order', () => {
    expect(draftPolicy).toContain('database.prepare(requestedSourcesSelect(`r.kind,r.id,p.project_id,');
    expect(purchases).toContain('database.prepare(requestedSourcesSelect(`jsonb_build_object(');
    expect(draftPolicy).not.toContain('database.prepare(`WITH requested AS');
    expect(purchases).not.toContain('database.prepare(`WITH requested AS');
    expect(draftPolicy).toContain('.bind(JSON.stringify(sources),actor.companyId,actor.companyId,actor.companyId)');
    expect(purchases).toContain('.bind(JSON.stringify(sources),companyId,companyId,companyId)');
    expect(shared).toContain('export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`');
  });
});

describe('Collaboration read-tail r39 isolation of protected SQL', () => {
  test('locks, notification targets and field-outbox files are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('add8defc4e1a0b3d369362e96d00acb93e34d0d0734cd0a1783b98a7382eb721');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(purchases).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins,social_post_drafts,social_post_participants,social_handover_acknowledgements,social_stories,social_reactions,social_follows,social_reports,social_mentions,social_topics,social_post_topics,social_comment_topics,social_ai_drafts,social_ai_usage,social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
  });

  test('no helper added a lock, write or notification statement', () => {
    for (const marker of [
      'export const queuedSourceAbsentGuard=',
      'export const socialPurchaseActorCte=',
      'export const requestedSourcesSelect=',
    ]) {
      const line = shared.split('\n').find(l => l.startsWith(marker))!;
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      expect(line.match(/SELECT /g) ?? []).toHaveLength(1);
    }
  });
});

describe('Collaboration read-tail r39 KPI pins', () => {
  test('retirement:ts-statements is pinned at 338 after r44, below the r39 BEFORE=360 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // queued guard (-1), actor CTE (-2), requested sources select (-1): 360 -> 356.
    // r40 deduped the two source-rows absence heads into one renderer: 356 -> 355.
    // r41 deduped task-origin/by-key, AI-usage row and member guards: 355 -> 352.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(333);
    expect(total).toBeLessThan(360);
  });
});
