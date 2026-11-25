import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const purchases = src('work-social-purchases.ts');
const planner = src('notification_statement.rs');

const READS = [
  [
    'work_social_receipts_witness',
    'SELECT jsonb_agg(to_jsonb(receipt) ORDER BY receipt.command_id)::text image FROM command_receipts receipt WHERE receipt.command_id IN(?,?,?)',
  ],
  [
    'work_social_draft_read',
    "SELECT to_jsonb(d)::text draft,(SELECT to_jsonb(c)::text FROM command_receipts c WHERE c.command_id=? AND c.operation=? AND c.record_id=d.id AND c.actor_id=d.author_id AND c.authority_json::jsonb->>'companyId'=d.company_id AND c.result_json::jsonb->>'id'=d.id) creator,EXISTS(SELECT 1 FROM social_comments c WHERE c.comment_upload_draft_id=d.id) consumed FROM social_post_drafts d WHERE d.id=? AND d.company_id=?",
  ],
  [
    'work_social_ai_phase_row',
    'SELECT d.actor_id,d.post_id,d.source_hash,d.actor_json,d.state draft_state,u.company_id,u.actor_id usage_actor,u.state usage_state FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.id=?',
  ],
] as const;

describe('Work Social guard reads on the native planner', () => {
  it('planner owns all three retired SQL literals byte for byte', () => {
    for (const [kind, sql] of READS) {
      expect(planner).toContain(`"${kind}" => "${sql}"`);
      expect((sql.match(/SELECT /g) ?? []).length).toBeGreaterThanOrEqual(1);
    }
  });

  it('adapter sends only kind + ordered values for the retired reads', () => {
    for (const [kind] of READS) expect(purchases).toContain(`ownerRead(database,actor.companyId,'${kind}'`);
    expect(purchases).not.toContain('SELECT jsonb_agg(to_jsonb(receipt) ORDER BY receipt.command_id)');
    expect(purchases).not.toContain('SELECT to_jsonb(d)::text draft,');
    expect(purchases).not.toContain('SELECT d.actor_id,d.post_id,d.source_hash');
    expect(purchases).toContain('nativeSystemNotificationStatement(db, company,');
    expect(purchases).toContain('values_json: JSON.stringify(values)');
  });

  it('leaves queue lock/claim/finalize reads protected', () => {
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(purchases).toContain('SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 284 -> 279 with the guard-read family retired', () => {
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
    expect(total).toBeLessThan(284);
  });
});
