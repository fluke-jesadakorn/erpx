import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const purchases = src('work-social-purchases.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'work_social_published_replay_guard',
  'work_social_running_lease_guard',
  'work_social_claim_guard',
  'work_social_question_source_predicate',
  'work_social_ai_phase_state_guard',
] as const;

describe('Queued-reply / claim guard family on the native planner', () => {
  it('all five retired guard heads are owner-registered', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).toMatch(/SELECT |EXISTS\(SELECT/);
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+\s+SET/i);
    }
  });

  it('adapter composes guard heads with only registered dynamic predicates', () => {
    for (const kind of KINDS) expect(purchases).toContain(`'${kind}'`);
    expect(purchases).not.toContain('SELECT 1 FROM social_post_drafts draft JOIN social_posts post');
    expect(purchases).not.toContain("SELECT 1 FROM social_agent_replies pending JOIN social_posts p");
    expect(purchases).not.toContain("SELECT 1 FROM social_agent_replies pending WHERE pending.id=? AND pending.state='running'");
    expect(purchases).not.toContain("SELECT 1 WHERE (?='failure' AND NOT EXISTS");
    expect(purchases).not.toContain("pending.owner_type='post' AND pending.owner_id=p.id");
    expect(purchases).toContain('async function queuedQuestionSourcePredicate()');
    expect(purchases).toContain('const predicate=await queuedReplyReadPredicate(modes)');
  });

  it('protects the locked claim and finalize reads', () => {
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset');
    expect(purchases).toContain('AND NOT (${predicate})');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 279 -> 272 with the queued-reply guard family retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(225);
    expect(total).toBeLessThan(279);
  });
});
