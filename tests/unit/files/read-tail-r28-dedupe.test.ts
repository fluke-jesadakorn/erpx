import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { PRIVATE_IMAGE_COLUMNS_SQL } from '../../../packages/server-files/src/adapters/read-tail-r28-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');

const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const PREFIX = 'SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes';
const NEEDLE = '${PRIVATE_IMAGE_COLUMNS_SQL}';

/** Render the template literal at `occurrence` by expanding only the shared
 * prefix; all other interpolations stay as source text, as in r21/r27. */
function rendered(text: string, occurrence = 0): string {
  let i = -1;
  for (let n = 0; n <= occurrence; n++) i = text.indexOf(NEEDLE, i + 1);
  const start = i + NEEDLE.length;
  return PREFIX + text.slice(start, text.indexOf('`', start));
}

describe('Files read-tail r28 byte-identical private-image projection family', () => {
  test('shared prefix is the exact byte-identical read-only literal', () => {
    expect(PRIVATE_IMAGE_COLUMNS_SQL).toBe(PREFIX);
    expect(Buffer.byteLength(PRIVATE_IMAGE_COLUMNS_SQL, 'utf8')).toBe(58);
    expect(PRIVATE_IMAGE_COLUMNS_SQL).not.toMatch(
      /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|\bcount\s*\(/i,
    );
  });

  test('helper is defined once and all three call sites compose it', () => {
    expect(src('read-tail-r28-source-sql.ts').split('PRIVATE_IMAGE_COLUMNS_SQL =').length - 1).toBe(1);
    const agent = src('collaboration-agent-sources.ts');
    const social = src('social-agent-image-sources.ts');
    const portal = src('legal-portal-repository.ts');
    expect(agent.match(/\$\{PRIVATE_IMAGE_COLUMNS_SQL\}/g) ?? []).toHaveLength(1);
    expect(social.match(/\$\{PRIVATE_IMAGE_COLUMNS_SQL\}/g) ?? []).toHaveLength(1);
    expect(portal.match(/\$\{PRIVATE_IMAGE_COLUMNS_SQL\}/g) ?? []).toHaveLength(1);
    for (const text of [agent, social, portal]) expect(text).not.toContain(PREFIX);
  });

  test('all three rendered statements are byte-identical to their pre-dedupe literals', () => {
    expect(rendered(src('collaboration-agent-sources.ts'))).toBe(
      `SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes FROM attachments a \${socialDraftAttachmentJoin}
          WHERE a.id=? AND d.company_id=? AND a.owner_type='social_draft'
            AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL) OR (d.comment_parent_post_id=? AND d.published_comment_id=?))
            AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`,
    );
    expect(rendered(src('social-agent-image-sources.ts'))).toBe(
      `SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes,
 jsonb_build_object('attachment',to_jsonb(a),'draft',to_jsonb(d),'intent',to_jsonb(u),
 \${SCANS_WITNESS_SQL}
 FROM attachments a \${socialDraftAttachmentJoin} LEFT JOIN upload_intents u ON u.id=a.upload_id AND u.company_id=a.company_id
 WHERE a.id=? AND d.company_id=? AND a.owner_type='social_draft'
 AND (d.published_post_id=? OR d.edit_post_id=? AND d.edit_saved_version IS NOT NULL OR d.comment_parent_post_id=? AND d.published_comment_id=?)
 AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`,
    );
    expect(rendered(src('legal-portal-repository.ts'))).toBe(
      `SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes,s.expected_target_version,
 CASE WHEN s.state='draft' AND l.state='draft' AND i.status='ready' AND a.source_state='available' AND a.revoked_at IS NULL THEN 1 ELSE 0 END recovery_draft,
 jsonb_build_object('source',to_jsonb(a),'upload',to_jsonb(i),'link',to_jsonb(l),'set',to_jsonb(s))::text image,
 CASE WHEN a.owner_type='legal_item' AND a.owner_id=? AND a.purpose='legal_portal_submission' AND a.created_by=? AND a.object_key=? AND a.original_name=?
  AND i.id=? AND i.company_id=a.company_id AND i.owner_type=a.owner_type AND i.owner_id=a.owner_id AND i.purpose=a.purpose
  AND i.created_by=a.created_by AND i.owner_version=s.expected_target_version AND i.final_key=a.object_key AND i.original_name=a.original_name
  AND i.attachment_set_id=s.id AND i.attachment_link_id=l.id
  AND l.id=? AND l.created_by=a.created_by AND l.attachment_id=a.id
  AND s.id=? AND s.company_id=a.company_id AND s.target_type=a.owner_type AND s.target_id=a.owner_id
  AND s.component_kind='action' AND s.component_ref=? AND s.purpose_key=a.purpose AND s.created_by=a.created_by
  AND s.audience_json::jsonb->>'portalShareId'=? THEN 1 ELSE 0 END scope_matches
 FROM attachments a LEFT JOIN upload_intents i ON i.id=a.upload_id
 LEFT JOIN attachment_links l ON l.id=i.attachment_link_id LEFT JOIN attachment_sets s ON s.id=l.set_id
 WHERE a.id=? AND a.company_id=?`,
    );
  });

  test('locks, advisory statements, native writes and r10-r27 helpers stay untouched', () => {
    expect(src('social-agent-image-sources.ts'))
      .toContain("database.prepare('LOCK TABLE attachments,upload_intents,attachment_scan_attempts IN SHARE ROW EXCLUSIVE MODE')");
    expect(src('collaboration-home-sources.ts').match(/\$\{SELECT_ATTACHMENT_ID_SQL\}/g) ?? []).toHaveLength(5);
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('commerce-delivery-dedupe-sql.ts')).toContain('export const COMMERCE_DELIVERY_GUARD_SQL =');
    expect(src('social-upload-cleanup-rows-sql.ts')).toContain('export const jsonbRowsSql =');
    expect(src('social-draft-image-guard-sql.ts')).toContain('export const socialDraftImageExistsSql =');
    const r26 = src('read-tail-r26-source-sql.ts');
    for (const name of ['preservedObjectExistsSql', 'imageSnapshotSql', 'orphanReconciliationSql', 'attachmentsByIdCompanySql'])
      expect(r26).toContain(`export const ${name} =`);
    expect(src('read-tail-r27-source-sql.ts')).toContain('export const SELECT_ATTACHMENT_ID_SQL =');
    expect(src('collaboration-agent-sources.ts')).toContain('${SELECT_ATTACHMENT_ID_SQL}');
  });

  test('retirement:ts-statements is strictly below the r28 BEFORE=279 baseline', () => {
    let literal = 0;
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) {
          const text = readFileSync(path, 'utf8');
          literal += literalSelectCount(text);
          total += selectCount(text);
        }
      }
    };
    walk(srcRoot);
    expect(literal).toBeLessThan(279);
    expect(total).toBeLessThan(279);
  });
});
