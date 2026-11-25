import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const purchases = src('work-social-purchases.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'work_social_witness_image_head',
  'work_social_witness_participants',
  'work_social_witness_teams',
  'work_social_witness_sources',
] as const;

describe('Attachment witness final projection on the native planner', () => {
  it('all four retired projection spans are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
    expect(planner).toContain('"work_social_witness_image_head" => " SELECT jsonb_build_object(\'root\',(SELECT image FROM native_root),\'posts\',"');
  });

  it('adapter composes owner bytes around only its registered aggregates', () => {
    for (const kind of KINDS) expect(purchases).toContain(kind);
    expect(purchases).not.toContain("SELECT jsonb_build_object('root',(SELECT image FROM native_root)");
    expect(purchases).not.toContain(" 'participants',(SELECT jsonb_agg(to_jsonb(member)");
    expect(purchases).not.toContain(" 'teams',(SELECT jsonb_agg(jsonb_build_object('team'");
    expect(purchases).not.toContain(" COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',kind,'id',id)");
    expect(purchases).toContain("${await nativeInboxSqlFragment(companyId,'work_social_witness_image_head')}${workSocialPostsAgg}");
    expect(purchases).toContain("${await nativeInboxSqlFragment(companyId,'work_social_witness_participants')}");
    expect(purchases).toContain("${await nativeInboxSqlFragment(companyId,'work_social_witness_teams')}");
    expect(purchases).toContain("${await nativeInboxSqlFragment(companyId,'work_social_witness_sources')}");
    expect(purchases).toContain('.bind(id,companyId,id,companyId,type,id,type,companyId,companyId)');
  });

  it('protects the locked claim and finalize reads', () => {
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 247 -> 240 with the witness projection retired', () => {
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
    expect(total).toBeLessThan(247);
  });
});
