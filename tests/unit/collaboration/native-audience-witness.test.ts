import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const purchases = src('work-social-purchases.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'work_social_audience_by_post',
  'work_social_audience_by_draft',
  'work_social_witness_drafts',
  'work_social_witness_post_ids',
  'work_social_witness_posts',
  'work_social_witness_native_sources',
] as const;

describe('Audience and source-witness reads on the native planner', () => {
  it('all six retired SQL spans are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
    expect(planner).toContain('"work_social_witness_post_ids" => "post_ids AS(SELECT ?::text id WHERE ?=\'social_post\' UNION SELECT');
  });

  it('adapter uses owner fragments instead of inline audience/witness SQL', () => {
    for (const kind of KINDS) expect(purchases).toContain(`'${kind}'`);
    expect(purchases).not.toContain('"SELECT audience_id id FROM social_posts WHERE id=? AND company_id=?');
    expect(purchases).not.toContain(' UNION SELECT p.audience_id FROM social_post_drafts d');
    expect(purchases).not.toContain('drafts AS(SELECT d.* FROM social_post_drafts d');
    expect(purchases).not.toContain('post_ids AS(SELECT ?::text id WHERE ?=');
    expect(purchases).not.toContain('native_sources AS(SELECT source.source_kind kind');
    expect(purchases).toContain('${socialPostDraftById(\'audience_id id\',\' \'," AND audience_kind=\'project\'")}${await nativeInboxSqlFragment(actor.companyId,\'work_social_audience_by_draft\')}');
    expect(purchases).toContain('.bind(command.id,actor.companyId).all<{id:string}>()');
    expect(purchases).toContain('.bind(id,companyId,id,companyId,type,id,type,companyId,companyId)');
  });

  it('protects locked claim and finalize reads', () => {
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 257 -> 247 with audience/witness reads retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(237);
    expect(total).toBeLessThan(257);
  });
});
