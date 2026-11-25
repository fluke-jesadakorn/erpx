import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const activity = src('social-activity.ts');
const planner = src('notification_statement.rs');
const metrics = src('executive-metric-projections.ts');
const evidence = src('executive-main-evidence-sources.ts');

const FRAGMENTS = [
  'social_activity_visible_posts',
  'social_activity_visible_comments',
  'social_activity_reply_arm',
  'social_activity_mention_arm',
  'social_activity_final',
] as const;

describe('Social activity fact union on the native planner', () => {
  it('all five owner fragments are registered and read-only', () => {
    for (const kind of FRAGMENTS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line.match(/SELECT /g) ?? []).toHaveLength(1);
    }
  });

  it('adapter composes owner bytes around only the registered scope predicate', () => {
    expect(activity.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(activity).toContain('export async function socialActivityEvents');
    expect(activity).toContain('${visiblePosts}${scope.sql}${visibleComments}${replyArm}${mentionArm}${finalSelect}');
    expect(activity).toContain('values: scope.bindings');
    for (const kind of FRAGMENTS) expect(activity).toContain(`nativeInboxSqlFragment(company, '${kind}')`);
  });

  it('executive callers await the owner-planned source', () => {
    expect(metrics).toContain('await socialActivityEvents(a)');
    expect(evidence.match(/await socialActivityEvents\(actor\)/g) ?? []).toHaveLength(2);
  });
});

describe('retirement:ts-statements', () => {
  it('drops 297 -> 292 with the activity union retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(284);
    expect(total).toBeLessThan(297);
  });
});
