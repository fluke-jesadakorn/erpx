import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'modules/collaboration/server/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const agent = src('social-agent.ts');
const planner = src('notification_statement.rs');

const RECENT = "SELECT id FROM social_comments WHERE post_id=? AND state='published' AND created_at<=? ORDER BY created_at DESC,id DESC LIMIT 10";
const CONTENT = "SELECT content_json FROM social_posts WHERE id=? AND company_id=? AND state='published'";

describe('Social agent read oracles on the native planner', () => {
  it('planner owns the two retired reads byte for byte', () => {
    expect(planner).toContain(`"social_agent_recent_comments" => "${RECENT}"`);
    expect(planner).toContain(`"social_agent_post_content" => "${CONTENT}"`);
    expect(RECENT.match(/SELECT /g) ?? []).toHaveLength(1);
    expect(CONTENT.match(/SELECT /g) ?? []).toHaveLength(1);
  });

  it('adapter sends only kind + ordered values, no SQL text', () => {
    expect(agent).not.toContain(RECENT);
    expect(agent).not.toContain(CONTENT);
    expect(agent).toContain("ownerRead(db, 'system', 'social_agent_recent_comments', [post_id, now])");
    expect(agent).toContain("ownerRead(db, companyId, 'social_agent_post_content', [post_id, companyId])");
    expect(agent).toContain('nativeSystemNotificationStatement(db, companyId,');
    expect(agent).toContain('values_json: JSON.stringify(values)');
  });

  it('leaves lock and guard SQL untouched', () => {
    expect(agent).toContain('SELECT pg_advisory_xact_lock(hashtext(?))');
    expect(agent).toContain('socialAgentReplyStateGuard("state=\'failed\'")');
    expect(agent).toContain("db.prepare(socialAgentReplyById('*'))");
  });
});

describe('retirement:ts-statements', () => {
  it('drops 308 -> 297 with the agent, web-push and handover reads retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(220);
    expect(total).toBeLessThan(308);
  });
});
