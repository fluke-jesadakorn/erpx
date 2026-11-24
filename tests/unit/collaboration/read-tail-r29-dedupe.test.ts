import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { socialPostSourcePmsGuard } from '../../../packages/server-collaboration/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-collaboration/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const authz = readFileSync(resolve(adapters, 'pms-source-authorization.ts'), 'utf8');
const policy = readFileSync(resolve(adapters, 'social-source-policy.ts'), 'utf8');

// Byte-identical PMS post-source absence guard that appeared at two read sites:
// pms-source-authorization.ts (`pmsPostSourcesReadableSql`) and
// social-source-policy.ts (`sourceAccessSql`). Only the post alias expression and
// the trailing condition vary; the `ps.post_id=<post>.id AND
// ps.source_kind='pms_record'` head, bind order and lock semantics stay.
const NESTED_TAIL = '\n    AND NOT EXISTS(SELECT 1 FROM (SRC) pr WHERE pr.id=ps.source_id\n      AND pr.company_id=p.company_id AND READABLE)';
const ORIG_AUTHZ = "NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=p.id AND ps.source_kind='pms_record'" + NESTED_TAIL + ')';
const ORIG_POLICY = "AND (NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=p.id AND ps.source_kind='pms_record') OR a.id=ASSIGNMENT)";

describe('Collaboration read-tail r29 parameterized PMS post-source guard family', () => {
  test('socialPostSourcePmsGuard stays byte-identical to both pre-dedupe literals', () => {
    expect(socialPostSourcePmsGuard('p', NESTED_TAIL)).toBe(ORIG_AUTHZ);
    expect(`AND (${socialPostSourcePmsGuard('p', '')} OR a.id=ASSIGNMENT)`).toBe(ORIG_POLICY);
    expect(ORIG_AUTHZ.split('SELECT ').length - 1).toBe(2);
    expect(ORIG_POLICY.split('SELECT ').length - 1).toBe(1);
  });

  test('shared fragment is declared once, inline literals are gone, call sites remain', () => {
    expect(shared.match(/export const socialPostSourcePmsGuard=/g) ?? []).toHaveLength(1);
    expect(authz).not.toContain("NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${post}.id");
    expect(policy).not.toContain("NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${alias}.id");
    expect(authz).toContain('socialPostSourcePmsGuard(post, `');
    expect(policy).toContain("socialPostSourcePmsGuard(alias, '')");
    expect(authz.split('socialPostSourcePmsGuard').length - 1).toBe(2); // one import + one call site
    expect(policy.split('socialPostSourcePmsGuard').length - 1).toBe(2);
  });

  test('stays read-only and leaves LOCK TABLE / FOR UPDATE / notification helpers untouched', () => {
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    expect(ORIG_AUTHZ).not.toMatch(lockRe);
    expect(ORIG_POLICY).not.toMatch(lockRe);
    expect(socialPostSourcePmsGuard('p', NESTED_TAIL)).not.toMatch(lockRe);
    // pre-existing lock statements remain byte-for-byte intact in the owner
    expect(readFileSync(resolve(adapters, 'work-social-purchases.ts'), 'utf8'))
      .toContain('ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500');
    expect(readFileSync(resolve(adapters, 'work-task-purchase-source.ts'), 'utf8'))
      .toContain('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins IN SHARE ROW EXCLUSIVE MODE');
    // notification helpers untouched
    expect(readFileSync(resolve(adapters, 'staff-notifications.ts'), 'utf8')).toContain('legacy_rows AS');
    expect(readFileSync(resolve(adapters, 'web-push.ts'), 'utf8')).toContain('SELECT');
  });
});

describe('Collaboration read-tail r29 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=398 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r29 deduped one new byte-identical family: the PMS post-source absence
    // guard in pms-source-authorization.ts and social-source-policy.ts into a
    // single parameterized fragment in read-tail-sql.ts. Net -1 SELECT token;
    // no lock statement or notification helper was modified.
    expect(total).toBeLessThanOrEqual(397);
    expect(total).toBeLessThan(398);
  });
});
