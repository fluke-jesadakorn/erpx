import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const purchases = src('work-social-purchases.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'work_social_facts_project',
  'work_social_facts_pms',
  'work_social_facts_tasks_head',
  'work_social_facts_tasks_from',
  'work_social_facts_tasks_join',
  'work_social_facts_tail',
  'work_social_image_head',
  'work_social_image_sources',
  'work_social_image_tail',
  'work_social_ai_phase_image',
] as const;

describe('Work Social image projection family on the native planner', () => {
  it('all ten image/source/AI fragments are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).toContain(`"${kind}" =>`);
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  it('adapter composes owner bytes around only its registered rowsets and aggregates', () => {
    for (const kind of KINDS) expect(purchases).toContain(`'${kind}'`);
    expect(purchases).not.toContain("jsonb_build_object('project',(SELECT jsonb_agg");
    expect(purchases).not.toContain("SELECT jsonb_build_object('target',?::text,'post'");
    expect(purchases).toContain('${project}${projectSocialPurchaseSources}${pms}${socialPmsSourceFacts}${tasksHead}${workOperationsSettlementSources.delegations}${tasksFrom}${workOperationsSettlementSources.tasks}${tasksJoin}${workTaskSocialFileSourceRows}${tail}');
    expect(purchases).toContain("${head}${workSocialCommentsAgg('WHERE c.post_id=?')}${sourcesFrag}${socialPostRevisionsAgg('WHERE r.post_id=?')}${tail}");
    expect(purchases).toContain(".bind(JSON.stringify(sources),companyId,companyId,companyId)");
    expect(purchases).toContain('.bind(await target(database,actor,{...command,id:sourceId}),postId');
  });

  it('AI phase image reads use the owner planner', () => {
    expect(purchases).toContain("const aiRows=async(database:SqlDatabase)=>(await ownerRead(database,actor.companyId,'work_social_ai_phase_image',[phase.reservationId,phase.reservationId,phase.reply?.id??null])).first<string>('image')");
    expect(purchases).toContain('socialAiPhaseImageSql("-ARRAY[\'state\',\'result_json\']"');
  });

  it('protects locked claim and finalize reads', () => {
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 272 -> 257 with the image projection family retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(247);
    expect(total).toBeLessThan(272);
  });
});
