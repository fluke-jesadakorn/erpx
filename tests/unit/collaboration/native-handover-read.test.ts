import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const facts = src('operations-handover-facts.ts');
const planner = src('notification_statement.rs');

describe('Social-post handover read family on the native planner', () => {
  it('planner owns the three retired SQL heads byte for byte', () => {
    expect(planner).toContain('"social_handover_recipients" => "SELECT user_id FROM social_post_participants WHERE post_id=? AND user_id<>?"');
    expect(planner).toContain('"social_handover_task_origin_absent" => "AND NOT EXISTS(SELECT 1 FROM social_task_origins o WHERE o.task_id=t.id)"');
    expect(planner).toContain('"social_handover_acknowledgements"');
    expect(planner).toContain('"SELECT user_id FROM social_handover_acknowledgements\\n    WHERE post_id=? AND post_version=? AND user_id IN ({placeholders})"');
  });

  it('adapter sends only kind + ordered values and composes the owner fragment', () => {
    expect(facts).toContain("ownerRead(database, 'system', 'social_handover_recipients', [postId, author_id])");
    expect(facts).toContain("ownerRead(database, 'system', 'social_handover_acknowledgements', [postId, version, ...ids])");
    expect(facts).toContain("nativeInboxSqlFragment(companyId, 'social_handover_task_origin_absent')");
    expect(facts).toContain('${originAbsent}`)');
    expect(facts).not.toContain('SELECT user_id FROM social_post_participants WHERE post_id=?');
    expect(facts).not.toContain('SELECT user_id FROM social_handover_acknowledgements\n');
    expect(facts).not.toContain('AND NOT EXISTS(SELECT 1 FROM social_task_origins o WHERE o.task_id=t.id)');
  });

  it('leaves the guard, draft reader and owner write untouched', () => {
    expect(facts).toContain("socialPostDraftById('body,participants_json,sources_json,published_post_id'");
    expect(facts).toContain("EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?)");
    expect(facts).toContain("nativeOwnerWriteSystemUnscoped(database, 'handover_ack_insert'");
  });
});

describe('retirement:ts-statements', () => {
  it('drops 300 -> 297 with the handover read family retired', () => {
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
    expect(total).toBeLessThan(300);
  });
});
