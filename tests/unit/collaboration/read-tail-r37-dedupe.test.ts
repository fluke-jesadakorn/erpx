import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { messagingTeamMemberGuard, socialPmsSourceFactExists, socialPostRevisionsFileAgg } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const socialPurchases = src('work-social-purchases.ts');
const taskSource = src('work-task-purchase-source.ts');
const ownerProjections = src('messaging-owner-projections.ts');
const sourcePolicy = src('messaging-source-policy.ts');
const messaging = src('messaging.ts');
const workSource = src('work-social-source.ts');
const productEntry = src('product-entry.ts');
const draftPolicy = src('work-social-draft-policy.ts');
const pms = '<pms>';
const origin = '${socialTaskOriginPostExists}';

// r37 generalizes three parameterized families. Each family replaces N inline
// byte-identical SELECT fragments with one parameterized helper: revisions
// aggregate 2->1 (-1), messaging team-member guard 3->1 (-2) and PMS
// source-record guard 4->1 (-3): 370 -> 364. No bind order, lock statement,
// notification target, jsonb_to_recordset or field-outbox predicate changed.
describe('Collaboration read-tail r37 byte oracles', () => {
  test('Rank1 revisions aggregate renders both original JOIN tails byte-identically', () => {
    expect(socialPostRevisionsFileAgg('JOIN posts p ON p.id=revision.post_id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN posts p ON p.id=revision.post_id)');
    expect(socialPostRevisionsFileAgg(`JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${origin}`)).toBe(
      `(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${origin})`);
    // The two pre-dedupe inline fragments shared a 112-byte head through `JOIN `.
    const a = "'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN posts p ON p.id=revision.post_id))::text image,";
    const b = "'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${socialTaskOriginPostExists})) image";
    let lcp = 0;
    while (lcp < a.length && a[lcp] === b[lcp]) lcp += 1;
    expect(a.slice(0, lcp)).toBe("'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN ");
    expect(lcp).toBe(112);
    expect(socialPostRevisionsFileAgg('')).toBe('(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision )');
    expect(shared.match(/export const socialPostRevisionsFileAgg=/g) ?? []).toHaveLength(1);
  });

  test('Rank2 team-member guard renders all three original forms byte-identically, including the empty-alias branch', () => {
    expect(`AND ${messagingTeamMemberGuard('tm', '?', 'a.user_id')}`).toBe(
      'AND EXISTS(SELECT 1 FROM messaging_team_members tm WHERE tm.team_id=? AND tm.user_id=a.user_id)');
    expect(messagingTeamMemberGuard('t', 'r.team_id', '?')).toBe(
      'EXISTS(SELECT 1 FROM messaging_team_members t WHERE t.team_id=r.team_id AND t.user_id=?)');
    expect(`AND ${messagingTeamMemberGuard('', '?', '?')}`).toBe(
      'AND EXISTS(SELECT 1 FROM messaging_team_members WHERE team_id=? AND user_id=?)');
    expect(shared.match(/export const messagingTeamMemberGuard=/g) ?? []).toHaveLength(1);
  });

  test('Rank3 PMS source-record guard renders all four original forms byte-identically', () => {
    expect(socialPmsSourceFactExists(pms, 'included_pms', 'native_source.source_id', '${alias}.company_id')).toBe(
      `EXISTS(SELECT 1 FROM (${pms}) included_pms WHERE included_pms.id=native_source.source_id AND included_pms.company_id=\${alias}.company_id)`);
    expect(socialPmsSourceFactExists(pms, 'source_pms', 'unresolved_source.source_id', 'p.company_id')).toBe(
      `EXISTS(SELECT 1 FROM (${pms}) source_pms WHERE source_pms.id=unresolved_source.source_id AND source_pms.company_id=p.company_id)`);
    expect(socialPmsSourceFactExists(pms, 'native', 'source.source_id', 'p.company_id')).toBe(
      `EXISTS(SELECT 1 FROM (${pms}) native WHERE native.id=source.source_id AND native.company_id=p.company_id)`);
    expect(socialPmsSourceFactExists(pms, 's', 'r.id', '?')).toBe(
      `EXISTS(SELECT 1 FROM (${pms}) s WHERE s.id=r.id AND s.company_id=?)`);
    expect(shared).toContain('WHERE ${alias}.id=${sourceRef} AND ${alias}.company_id=${companyRef})');
    expect(shared.match(/export const socialPmsSourceFactExists=/g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r37 call sites and bind order', () => {
  test('Rank1 both aggregate sites use the helper and dropped their inline heads', () => {
    expect(socialPurchases).toContain("${socialPostRevisionsFileAgg('JOIN posts p ON p.id=revision.post_id')}");
    expect(taskSource).toContain('${socialPostRevisionsFileAgg(`JOIN social_posts post ON post.id=revision.post_id WHERE post.company_id=t.company_id AND ${socialTaskOriginPostExists}`)}');
    expect(socialPurchases).not.toContain('(SELECT jsonb_agg(to_jsonb(revision)');
    expect(taskSource).not.toContain('(SELECT jsonb_agg(to_jsonb(revision)');
    expect(socialPurchases).toContain('.bind(id,companyId,id,companyId,type,id,type,companyId,companyId)');
    expect(taskSource).toContain('.bind(taskId,requestedPostId??null,requestedPostId??null,companyId)');
  });

  test('Rank2 all three team-member sites use the helper and keep their binds', () => {
    expect(ownerProjections).toBe("import {messagingTeamMemberGuard} from './read-tail-sql';\nexport const messagingTeamMemberPredicate=`AND ${messagingTeamMemberGuard('tm','?','a.user_id')}`;\n");
    expect(sourcePolicy).toContain("${messagingTeamMemberGuard('t','r.team_id','?')}");
    expect(messaging).toContain("${teamId ? ` AND ${messagingTeamMemberGuard('','?','?')}` : ''}");
    expect(sourcePolicy).toContain('export const messagingRoomAccessArgs=(actor:WorkContext)=>[actor.userId,actor.userId,actor.userId,actor.userId,actor.userId];');
    expect(messaging).toContain('[companyId, id, ...value2]');
    for (const text of [ownerProjections, sourcePolicy, messaging]) {
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM messaging_team_members(?: t|\s+WHERE)/);
    }
  });

  test('Rank3 all four PMS source sites use the helper and keep kind/source_kind semantics', () => {
    expect(workSource).toContain("${socialPmsSourceFactExists(socialPmsSourceFacts,'included_pms','native_source.source_id',`${alias}.company_id`)}");
    expect(workSource).toContain("${socialPmsSourceFactExists(socialPmsSourceFacts,'source_pms','unresolved_source.source_id','p.company_id')}");
    expect(productEntry).toContain("${socialPmsSourceFactExists(socialPmsSourceFacts,'native','source.source_id','p.company_id')}");
    expect(draftPolicy).toContain("${socialPmsSourceFactExists(socialPmsSourceFacts,'s','r.id','?')}");
    // `kind` on requested/source rows, `source_kind` on source rows: neither was confused with the fact `id` column.
    expect(workSource).toContain("native_source.source_kind='pms_record'");
    expect(workSource).toContain("unresolved_source.source_kind='pms_record'");
    expect(productEntry).toContain("source.source_kind='pms_record'");
    expect(draftPolicy).toContain("r.kind='pms_record'");
    for (const text of [workSource, productEntry, draftPolicy]) {
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(\$\{socialPmsSourceFacts\}\) \w+ WHERE/);
    }
    expect(draftPolicy).toContain('.bind(JSON.stringify(sources),actor.companyId,actor.companyId,actor.companyId)');
    expect(productEntry).toContain('.bind(...purchase.args,target.id,scope.companyId,scope.assignmentVersion)');
  });
});

describe('Collaboration read-tail r37 isolation of protected SQL', () => {
  test('locks, notification targets, jsonb_to_recordset and field-outbox files are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('add8defc4e1a0b3d369362e96d00acb93e34d0d0734cd0a1783b98a7382eb721');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    expect(shared).toContain('export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`');
    expect(draftPolicy).toContain('database.prepare(requestedSourcesSelect(`r.kind,r.id,p.project_id,');
    expect(socialPurchases).toContain('JOIN jsonb_to_recordset(?::jsonb) AS original(id text,company_id text,actor_id text,post_id text,actor_json text,attempt integer,lease_token text)');
    expect(taskSource).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(taskSource).toContain('LOCK TABLE messaging_teams,messaging_team_members IN SHARE MODE');
    expect(socialPurchases).toContain('.bind(JSON.stringify(picked))');
    expect(sourcePolicy).toContain("${messagingMemberExists('messaging_room_members','room_id','r.id')}");
  });

  test('no helper added a lock, write or notification statement', () => {
    const added = [
      'export const messagingTeamMemberGuard=',
      'export const socialPostRevisionsAggBy=',
      'export const socialPmsSourceFactExists=',
    ];
    for (const marker of added) {
      const line = shared.split('\n').find(l => l.startsWith(marker))!;
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      expect(line.match(/SELECT /g) ?? []).toHaveLength(1);
    }
    // r50 folded FileAgg into the shared renderer: the retained wrapper owns no
    // SELECT token and only delegates to socialPostRevisionsAggBy.
    const fileWrapper = shared.split('\n').find(l => l.startsWith('export const socialPostRevisionsFileAgg='))!;
    expect(fileWrapper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
    expect(fileWrapper.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(fileWrapper).toContain('socialPostRevisionsAggBy(');
  });
});

describe('Collaboration read-tail r37 KPI pins', () => {
  test('retirement:ts-statements is pinned at 324 after r44, below the r37 BEFORE=370 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 2->1 revisions (-1), 3->1 team-member (-2), 4->1 PMS source (-3): 370 -> 364.
    // r38 refreshed it after the 364 -> 360 settlement/AI-draft/post-head dedupe;
    // r39 refreshed it after the 360 -> 356 queued-guard/actor-CTE/requested-select dedupe.
    // r40 refreshed it after the 356 -> 355 source-rows absence guard dedupe.
    // r41 refreshed it after the 355 -> 352 task-origin/by-key, AI-usage row and
    // team/room member-existence dedupe.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(324);
    expect(total).toBeLessThan(370);
  });
});
