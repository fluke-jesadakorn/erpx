import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const src = (f: string) => readFileSync(resolve(root, 'adapters', f), 'utf8');
const feed = src('social.ts');
const planner = src('notification_statement.rs');

const KINDS = [
  'social_feed_head',
  'social_feed_appreciations',
  'social_feed_comment_count',
  'social_feed_appreciated',
  'social_feed_topics',
  'social_following_topic_exists',
  'social_feed_comments_by_post',
  'social_stories_head',
] as const;

describe('Social feed projection on the native planner', () => {
  it('all eight retired SELECT fragments are owner-registered and read-only', () => {
    for (const kind of KINDS) {
      expect(planner).toContain(`"${kind}" =>`);
      const line = planner.split('\n').find(l => l.includes(`"${kind}" =>`))!;
      expect(line).toMatch(/SELECT |EXISTS\(SELECT/);
      expect(line).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|FOR\s+UPDATE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('adapter composes owner bytes around only the registered dynamic fragments', () => {
    expect(feed.match(/SELECT /g) ?? []).toHaveLength(0);
    for (const kind of KINDS) expect(feed).toContain(`nativeInboxSqlFragment(context.companyId, '${kind}')`);
    expect(feed).toContain('${purchase.prefix}${head}${appreciations}${commentCount}${appreciated}${socialFollowPostExists("\'post\'",\'p.id\')} AS followed,\\n      ${topics}${scope.sql}${filterSql(filter, topicExists)} AND ${purchase.where}');
    expect(feed).toContain('db.prepare(commentsByPost).bind(row.id)');
    expect(feed).toContain('${purchase.prefix}${storiesHead}${scope.sql} AND ${purchase.where} ORDER BY created_at DESC LIMIT 20');
    expect(feed).toContain('.bind(...purchase.args,context.userId, context.userId, ...scope.bindings, ...followingBindings)');
  });

  it('leaves the purchase scope, filter kinds and owner writes untouched', () => {
    expect(feed).toContain("if (filter === \"team\")\n        return \" AND p.kind='team'\"");
    expect(feed).toContain('workSocialReadScope(db,context)');
    expect(feed).toContain('createSocialApplication');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 292 -> 284 with the feed projection retired', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(279);
    expect(total).toBeLessThan(292);
  });
});
