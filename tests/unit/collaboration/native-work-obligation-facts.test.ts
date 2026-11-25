import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const adapter = src('work-package-obligations.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'work_obligation_handbook_source',
  'work_obligation_calendar_source',
  'work_obligation_notification_fact',
  'work_obligation_handbook_fact',
  'work_obligation_social_draft_fact',
  'work_obligation_social_story_fact',
  'work_obligation_social_ai_draft_fact',
  'work_obligation_final',
] as const;

describe('Work package-obligation fact fragments', () => {
  it('all eight static spans are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|LOCK\s+TABLE|FOR\s+UPDATE|pg_advisory/i);
    }
  });

  it('adapter composes owner bytes around its registered rowsets and predicates', () => {
    for (const kind of KINDS) expect(adapter).toContain(`'${kind}'`);
    expect(adapter).not.toContain("OR (n.source_type IN ('handbook','handbook_article')");
    expect(adapter).not.toContain("SELECT 'notification' kind,n.id,n.source_version");
    expect(adapter).not.toContain("UNION ALL SELECT 'handbook',article.id");
    expect(adapter).not.toContain("UNION ALL SELECT 'social-story',story.id");
    expect(adapter).toContain('${handbookSource}');
    expect(adapter).toContain('${calendarSource}');
    expect(adapter).toContain('${notificationFact}');
    expect(adapter).toContain('${handbookFact}');
    expect(adapter).toContain('${socialDraftFact}');
    expect(adapter).toContain('${socialStoryFact}');
    expect(adapter).toContain('${socialAiDraftFact}');
    expect(adapter).toContain('${finalSelect}');
    expect(adapter).toContain('.bind(input.companyId,input.packageId,input.now,input.now)');
  });

  it('leaves advisory and share locks untouched', () => {
    expect(adapter).toContain('LOCK TABLE social_posts,social_post_sources');
    expect(adapter).toContain('WORK_COLLABORATION_OBLIGATION_SOURCE_REQUIRED');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 234 -> 225 with the obligation fact arms retired', () => {
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
    expect(total).toBeLessThan(234);
  });
});
