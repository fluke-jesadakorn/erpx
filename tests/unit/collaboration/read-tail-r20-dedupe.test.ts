import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialAgentReplyLeaseIdentity } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');
// Byte-identical identity prefix that appeared in both social_agent_replies
// lease guards in work-social-purchases.ts. Only the trailing state predicate
// differs, so the prefix is parameterized and the binds stay in the same order.
const PREFIX = 'SELECT 1 FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND lease_token=? AND actor_json=? AND owner_type=? AND owner_id=?';
const FAILURE_TAIL = " AND state IN ('running','failed')";
const COMPLETE_TAIL = " AND state=? AND (? IN ('reserve','images') OR lease_until=0) AND (?='failure' OR state='completed' OR lease_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";

describe('Collaboration read-tail r20 parameterized lease-identity family', () => {
  test('socialAgentReplyLeaseIdentity stays byte-identical to both pre-dedupe literals', () => {
    expect(socialAgentReplyLeaseIdentity(FAILURE_TAIL)).toBe(PREFIX + FAILURE_TAIL);
    expect(socialAgentReplyLeaseIdentity(COMPLETE_TAIL)).toBe(PREFIX + COMPLETE_TAIL);
    expect(PREFIX.split('SELECT ').length - 1).toBe(1);
  });

  test('the shared fragment is declared exactly once and the inline literals are gone', () => {
    expect(shared.match(/export const socialAgentReplyLeaseIdentity=/g) ?? []).toHaveLength(1);
    // one import + two call sites
    expect(purchases.split('socialAgentReplyLeaseIdentity').length - 1).toBe(3);
    expect(purchases).not.toContain(PREFIX);
  });

  test('keeps the fragment read-only and leaves FOR UPDATE / lock semantics untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(socialAgentReplyLeaseIdentity('')).not.toMatch(lockRe);
    // The two call sites only inject a state predicate; no lock token is added.
    for (const line of purchases.split('\n').filter(l => l.includes('socialAgentReplyLeaseIdentity(') && !l.includes('import'))) {
      expect(line).not.toMatch(lockRe);
    }
    // Existing lock statements in the caller are preserved verbatim.
    expect(purchases).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins,social_post_drafts,social_post_participants,social_handover_acknowledgements,social_stories,social_reactions,social_follows,social_reports,social_mentions,social_topics,social_post_topics,social_comment_topics,social_ai_drafts,social_ai_usage,social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('FOR UPDATE SKIP LOCKED LIMIT 500');
  });
});

describe('Collaboration read-tail r20 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r20 BEFORE=412 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r20 deduped the 2x byte-identical social_agent_replies lease-identity
    // prefix in work-social-purchases.ts into one shared parameterized fragment.
    expect(total).toBe(411);
    expect(total).toBeLessThan(412);
  });
});
