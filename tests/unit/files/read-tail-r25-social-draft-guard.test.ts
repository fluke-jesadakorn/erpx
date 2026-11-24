import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { socialDraftImageExistsSql } from '../../../packages/server-files/src/adapters/social-draft-image-guard-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

describe('Files read-tail r25 social-draft image guard dedupe', () => {
  test('shared prefix reproduces the original guard literal byte for byte', () => {
    expect(socialDraftImageExistsSql).toBe('EXISTS(SELECT 1 FROM attachments a JOIN ');
  });

  test('helper is defined once and all three collaboration-home guards compose it', () => {
    expect(src('social-draft-image-guard-sql.ts').split('socialDraftImageExistsSql =').length - 1).toBe(1);
    const home = src('collaboration-home-sources.ts');
    expect(home.match(/\$\{socialDraftImageExistsSql\}/g) ?? []).toHaveLength(4);
    // r45 folded guardPublishImage's inline copy into the shared helper; the
    // three other guards were already composing it before r25.
    expect(home.match(/EXISTS\(SELECT 1 FROM attachments a JOIN /g) ?? []).toHaveLength(0);
    expect(home).toContain('import { socialDraftImageExistsSql } from \'./social-draft-image-guard-sql\';');
  });

  test('locks, advisory statements, native write boundaries and r10-r24 helpers remain untouched', () => {
    const home = src('collaboration-home-sources.ts');
    for (const kind of [
      'SF_REVOKE_DRAFT', 'SF_EXPIRE_DRAFT_UPLOADS', 'SF_REVOKE_POST',
      'SF_EXPIRE_POST_UPLOADS', 'SF_REVOKE_COMMENT',
    ]) expect(home).toContain(`nativeWrite('${kind}'`);
    expect(src('social-upload-cleanup.ts'))
      .toContain("db.prepare('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE')");
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('commerce-delivery-dedupe-sql.ts')).toContain('export const COMMERCE_DELIVERY_GUARD_SQL =');
    expect(src('social-upload-cleanup-rows-sql.ts')).toContain('export const jsonbRowsSql =');
  });

  test('retirement:ts-statements stays strictly below the r25 BEFORE=292 baseline', () => {
    let total = 0;
    let literal = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          total += selectCount(text);
          literal += literalSelectCount(text);
        }
      }
    };
    walk(srcRoot);
    expect(literal).toBeLessThan(292);
    expect(total).toBeLessThan(293);
  });
});
