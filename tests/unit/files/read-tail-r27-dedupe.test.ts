import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { SELECT_ATTACHMENT_ID_SQL } from '../../../packages/server-files/src/adapters/read-tail-r27-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-files/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');

const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const NEEDLE = '${SELECT_ATTACHMENT_ID_SQL}';

/** Render the template literal at `occurrence` by expanding only the shared
 * prefix; all other interpolations stay as source text, as in r21. */
function rendered(text: string, occurrence = 0): string {
  let i = -1;
  for (let n = 0; n <= occurrence; n++) i = text.indexOf(NEEDLE, i + 1);
  const start = i + NEEDLE.length;
  return SELECT_ATTACHMENT_ID_SQL + text.slice(start, text.indexOf('`', start));
}

describe('Files read-tail r27 byte-identical attachment-id read family', () => {
  test('shared prefix is the exact byte-identical read-only literal', () => {
    expect(SELECT_ATTACHMENT_ID_SQL).toBe('SELECT a.id FROM attachments a');
    expect(Buffer.byteLength(SELECT_ATTACHMENT_ID_SQL, 'utf8')).toBe(30);
    expect(SELECT_ATTACHMENT_ID_SQL).not.toMatch(
      /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|\bcount\s*\(/i,
    );
  });

  test('helper is defined once and every one of the seven sites composes it', () => {
    expect(src('read-tail-r27-source-sql.ts').split('SELECT_ATTACHMENT_ID_SQL =').length - 1).toBe(1);
    const home = src('collaboration-home-sources.ts');
    const agent = src('collaboration-agent-sources.ts');
    const sr = src('attachments/service-repository.ts');
    expect(home.match(/\$\{SELECT_ATTACHMENT_ID_SQL\}/g) ?? []).toHaveLength(5);
    expect(agent.match(/\$\{SELECT_ATTACHMENT_ID_SQL\}/g) ?? []).toHaveLength(1);
    expect(sr.match(/\$\{SELECT_ATTACHMENT_ID_SQL\}/g) ?? []).toHaveLength(1);
    for (const text of [home, agent, sr]) expect(text).not.toContain('SELECT a.id FROM attachments a');
  });

  test('all seven rendered statements are byte-identical to their pre-dedupe literals', () => {
    const home = src('collaboration-home-sources.ts');
    expect(rendered(home, 0)).toBe(
      `SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (\${socialDraftFilesReadSql}) d ON d.id=a.owner_id
      WHERE a.id IN (\${ids.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL
      AND a.media_type IN ('image/jpeg','image/png','image/webp') AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`,
    );
    expect(rendered(home, 1)).toBe(
      `SELECT a.id FROM attachments a JOIN (\${socialDraftFilesReadSql}) d ON d.id=a.owner_id
            WHERE d.published_post_id=? AND d.company_id=? AND a.owner_type='social_draft' AND a.revoked_at IS NULL`,
    );
    expect(rendered(home, 2)).toBe(
      `SELECT a.id FROM attachments a JOIN (\${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (\${value.map(() => "?").join(",")}) AND a.owner_type='social_draft'
          AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL))
          AND d.company_id=? AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`,
    );
    expect(rendered(home, 3)).toBe(
      `SELECT a.id FROM attachments a
        JOIN upload_intents u ON u.id=a.upload_id JOIN (\${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (\${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=?
          AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL
          AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')
          AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`,
    );
    expect(rendered(home, 4)).toBe(
      `SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (\${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id IN (\${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL AND u.status='ready' AND a.media_type IN ('image/jpeg','image/png','image/webp')`,
    );
    expect(rendered(src('collaboration-agent-sources.ts'), 0)).toBe(
      `SELECT a.id FROM attachments a \${socialDraftAttachmentJoin}
      WHERE (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL)) AND d.company_id=?
        AND a.owner_type='social_draft' AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp') ORDER BY a.created_at,a.id`,
    );
    expect(rendered(src('attachments/service-repository.ts'), 0)).toContain(
      `SELECT a.id FROM attachments a WHERE a.company_id IS NOT NULL AND a.legal_hold=0 AND a.source_state='available' AND NOT erpx_files_field_attachment_owned(a.id)`,
    );
  });

  test('locks, advisory statements, native writes and r10-r26 helpers stay untouched', () => {
    const home = src('collaboration-home-sources.ts');
    for (const kind of [
      'SF_REVOKE_DRAFT', 'SF_EXPIRE_DRAFT_UPLOADS', 'SF_REVOKE_POST',
      'SF_EXPIRE_POST_UPLOADS', 'SF_REVOKE_COMMENT',
    ]) expect(home).toContain(`nativeWrite('${kind}'`);
    // r45: guardPublishImage composes the r25 helper; its tail is unchanged.
    expect(home).toContain('${socialDraftImageExistsSql}upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id');
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
    expect(sr).toContain("attachmentsByIdCompanySql('checksum_sha256,source_state')");
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
  });

  test('retirement:ts-statements is strictly below the r27 BEFORE=285 baseline', () => {
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
    expect(literal).toBeLessThan(285);
    expect(total).toBeLessThan(285);
  });
});
