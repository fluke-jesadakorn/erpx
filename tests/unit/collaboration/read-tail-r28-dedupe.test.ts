import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostRevisionsAgg } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const purchases = readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8');
const taskSource = readFileSync(resolve(adapters, 'work-task-purchase-source.ts'), 'utf8');

// Byte-identical social-post revisions aggregate head that appeared at two
// image-projection read sites: work-social-purchases.ts (`image`) and
// work-task-purchase-source.ts (`workTaskSocialPurchaseSource`). Only the
// trailing WHERE/JOIN expression varies; the `jsonb_agg(to_jsonb(r) ORDER BY
// r.id) FROM social_post_revisions r` head, bind order and lock semantics stay.
const WHERE_TAIL = 'WHERE r.post_id=?';
const JOIN_TAIL = 'JOIN posts p ON p.id=r.post_id';
const PURCHASE_LITERAL = '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r WHERE r.post_id=?)';
const TASK_LITERAL = '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r JOIN posts p ON p.id=r.post_id)';

describe('Collaboration read-tail r28 parameterized revisions-aggregate family', () => {
  test('socialPostRevisionsAgg stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostRevisionsAgg(WHERE_TAIL)).toBe(PURCHASE_LITERAL);
    expect(socialPostRevisionsAgg(JOIN_TAIL)).toBe(TASK_LITERAL);
    for (const sql of [PURCHASE_LITERAL, TASK_LITERAL]) expect(sql.split('SELECT ').length - 1).toBe(1);
  });

  test('shared fragment is declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialPostRevisionsAgg=/g) ?? []).toHaveLength(1);
    expect(purchases).not.toContain('jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r WHERE r.post_id=?');
    expect(taskSource).not.toContain('jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r JOIN posts p ON p.id=r.post_id');
    expect(purchases).toContain(`\${socialPostRevisionsAgg('${WHERE_TAIL}')}`);
    expect(taskSource).toContain(`\${socialPostRevisionsAgg('${JOIN_TAIL}')}`);
    expect(purchases.split('socialPostRevisionsAgg').length - 1).toBe(2); // one import + one call site
    expect(taskSource.split('socialPostRevisionsAgg').length - 1).toBe(2);
  });

  test('lock statements and notification helpers stay untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(PURCHASE_LITERAL).not.toMatch(lockRe);
    expect(TASK_LITERAL).not.toMatch(lockRe);
    expect(socialPostRevisionsAgg(JOIN_TAIL)).not.toMatch(lockRe);
    // pre-existing lock statements in both files remain byte-for-byte intact
    expect(purchases).toContain('social_agent_replies IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(taskSource).toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    expect(taskSource).toContain('LOCK TABLE messaging_teams,messaging_team_members IN SHARE MODE');
  });
});

describe('Collaboration read-tail r28 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=399 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r28 deduped one new byte-identical family: the social-post revisions
    // aggregate image heads in work-social-purchases.ts and
    // work-task-purchase-source.ts, each into one shared parameterized fragment
    // in read-tail-sql.ts. Net -1 SELECT token; no lock statement or
    // notification helper was modified. r29 deduped a further family below this
    // r28 baseline.
    expect(total).toBeLessThanOrEqual(398);
    expect(total).toBeLessThan(399);
  });
});
