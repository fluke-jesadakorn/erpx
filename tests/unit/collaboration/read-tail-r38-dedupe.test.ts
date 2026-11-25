import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { readablePostHead, settlementSourceRows, socialAiDraftById } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const fixed = (f: string) => readFileSync(resolve(srcRoot, f), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const shared = src('read-tail-sql.ts');
const settlement = src('work-package-obligation-sources.ts');
const socialAi = src('social-ai.ts');
const socialTask = src('social-task.ts');
const socialSource = src('social-source.ts');
const socialHome = src('social-home.ts');
const aiCols = 'actor_json,post_id,kind,source_hash,result_json,state,source_version,comment_ids_json,expires_at,provider_json';

// r38 generalizes three parameterized families. Each family replaces N inline
// byte-identical SELECT fragments with one parameterized helper: settlement
// source rows 3->1 (-2), AI draft-by-id 2->1 (-1) and readable post head 2->1
// (-1): 364 -> 360. No bind order, lock statement, notification target or
// public export key changed. The queued guard and actor CTE were skipped.
describe('Collaboration read-tail r38 byte oracles', () => {
  test('Rank1 settlementSourceRows renders all three original literals byte-identically', () => {
    expect(settlementSourceRows('social_task_origins')).toBe('SELECT * FROM social_task_origins');
    expect(settlementSourceRows('social_posts')).toBe('SELECT * FROM social_posts');
    expect(settlementSourceRows('social_post_sources')).toBe('SELECT * FROM social_post_sources');
    // The three pre-dedupe literals shared a 21-byte head ending in `social_`.
    for (const table of ['social_task_origins', 'social_posts', 'social_post_sources']) {
      expect(`SELECT * FROM ${table}`.startsWith('SELECT * FROM social_')).toBe(true);
    }
    expect(shared).toContain('export const settlementSourceRows=(table:string)=>`SELECT * FROM ${table}`;');
    expect(shared.match(/export const settlementSourceRows=/g) ?? []).toHaveLength(1);
  });

  test('Rank2 socialAiDraftById renders both original drafts reads byte-identically', () => {
    expect(socialAiDraftById(aiCols)).toBe(`SELECT ${aiCols} FROM social_ai_drafts WHERE id=? AND actor_id=?`);
    expect(socialAiDraftById('provider_json')).toBe('SELECT provider_json FROM social_ai_drafts WHERE id=? AND actor_id=?');
    expect(shared).toContain('export const socialAiDraftById=(cols:string)=>`SELECT ${cols} FROM social_ai_drafts WHERE id=? AND actor_id=?`;');
    expect(shared.match(/export const socialAiDraftById=/g) ?? []).toHaveLength(1);
  });

  test('Rank3 readablePostHead renders both original post heads byte-identically', () => {
    const tailA = `p.state='published' AND ACCESS AND WHERE`;
    expect(readablePostHead('*', tailA)).toBe(`SELECT p.* FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${tailA}`);
    const tailB = 'ACCESS AND WHERE';
    expect(readablePostHead('author_id', tailB)).toBe(`SELECT p.author_id FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${tailB}`);
    expect(shared).toContain("export const readablePostHead=(cols:string,tail:string)=>socialPostScopedRead(`p.${cols}`,'p',tail);");
    expect(shared.match(/export const readablePostHead=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialPostScopedRead=/g) ?? []).toHaveLength(1);
  });
});

describe('Collaboration read-tail r38 call sites and binds', () => {
  test('Rank1 keeps the three public export keys and zero binds for server-operations', () => {
    expect(settlement).toContain("export const workSocialSettlementSources={origins:settlementSourceRows('social_task_origins'),posts:settlementSourceRows('social_posts'),postSources:settlementSourceRows('social_post_sources')} as const;");
    expect(settlement).toContain("import {settlementSourceRows} from './read-tail-sql';");
    expect(settlement).not.toContain("'SELECT * FROM social_");
    expect(settlement).toContain("LOCK TABLE social_task_origins,social_posts,social_post_sources IN SHARE MODE");
  });

  test('Rank2 both sites use the helper and keep their id/user binds', () => {
    expect(socialAi).toContain(`.prepare(socialAiDraftById('${aiCols}'))`);
    expect(socialAi).toContain('.bind(id, userId)');
    expect(socialTask).toContain("db.prepare(socialAiDraftById('provider_json')).bind(id, user)");
    expect(socialAi).not.toContain('FROM social_ai_drafts WHERE id=? AND actor_id=?');
    expect(socialTask).not.toContain("'SELECT provider_json FROM social_ai_drafts");
  });

  test('Rank3 both sites use the helper and keep their binds at the call sites', () => {
    expect(socialSource).toContain(`\${purchase.prefix}\${readablePostHead('*', \`p.state='published' AND \${sourceAccessSql("p", value.length ? "?::text" : undefined)} AND \${purchase.where}\`)}`);
    expect(socialHome).toContain(`\${purchase.prefix}\${readablePostHead('author_id', \`\${sourceAccessSql()} AND \${purchase.where}\`)}`);
    expect(socialSource).not.toContain('SELECT p.* FROM social_posts p WHERE');
    expect(socialHome).not.toContain('SELECT p.author_id FROM social_posts p WHERE');
    expect(socialSource).toContain('.bind(...purchase.args,id, companyId, userId, now, now2, ...value)');
    expect(socialHome).toContain('.bind(...purchase.args,id, companyId, userId, now, now2)');
  });
});

describe('Collaboration read-tail r38 isolation of protected SQL', () => {
  test('locks, notification targets, field-outbox and guard files are unchanged', () => {
    expect(sha(fixed('adapters/inbox.ts'))).toBe('8983c717a5753fd3d9bb436150455abde716cec0d1f51b724d0558296ed3109e');
    expect(sha(fixed('adapters/staff-notifications.ts'))).toBe('3ce65abf55973c3a41228531a9d2778739a6a7f859a37591c016bda9f9a2508a');
    expect(sha(fixed('adapters/messaging-purchases.ts'))).toBe('6921fd7a922dccbab407a37ecf8544b26494388c442871f4b084c45b39133c9b');
    expect(sha(fixed('public/field-outbox-predicate.generated.ts'))).toBe('1c1945bec812a3981738b0d36af5483786ff8e372c669453c0bc8683976a6c84');
    // The queued guard and actor CTE were explicitly left untouched.
    expect(socialTask).toContain("EXISTS(SELECT 1 FROM social_ai_drafts WHERE id=? AND actor_id=? AND post_id=? AND source_hash=? AND source_version=? AND state='ready' AND kind='task' AND expires_at>?)");
    expect(shared).toContain('export const requestedSourcesJsonb=`(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text))`');
  });

  test('no helper added a lock, write or notification statement', () => {
    for (const marker of [
      'export const settlementSourceRows=',
      'export const socialAiDraftById=',
      'export const readablePostHead=',
    ]) {
      const line = shared.split('\n').find(l => l.startsWith(marker))!;
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|INSERT\s+INTO|UPDATE\s+\w+\s+SET/);
      // readablePostHead is now a wrapper over the single socialPostScopedRead
      // renderer, which owns the only SELECT for this family.
      expect(line.match(/SELECT /g) ?? []).toHaveLength(marker === 'export const readablePostHead=' ? 0 : 1);
    }
  });
});

describe('Collaboration read-tail r38 KPI pins', () => {
  test('retirement:ts-statements is pinned at 257 after r44, below the r38 BEFORE=364 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 3->1 settlement (-2), 2->1 AI draft (-1), 2->1 post head (-1): 364 -> 360.
    // r39 refreshed this pin after the 360 -> 356 queued-guard/actor-CTE/requested-select dedupe.
    // r40 refreshed it after the 356 -> 355 source-rows absence guard dedupe.
    // r41 refreshed it after the 355 -> 352 task-origin/by-key, AI-usage row and
    // team/room member-existence dedupe.
    // r42 deduped the comment head and UNION projection pair: 352 -> 350.
    // r43 retired draft-by-id reuse, messaging team image and target image: 350 -> 344.
    // r44 retired the latest-message tail, absent-social-post guard and handover
    // ack join: 344 -> 341. Prior pins were refreshed only where 341 replaced 344.
    expect(total).toBe(257);
    expect(total).toBeLessThan(364);
  });
});
