import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { LINK_ATTACHMENT_SELECT_SQL } from '../../../modules/files/server/src/adapters/read-tail-dedupe-sql';

const srcRoot = resolve(__dirname, '../../../modules/files/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;

const PREFIX = 'SELECT l.id link_id,l.attachment_id';
const NEEDLE = '${LINK_ATTACHMENT_SELECT_SQL}';

/** Render the first SQL template literal in `text` that composes the constant. */
function rendered(text: string, occurrence = 0): string {
  let i = -1;
  for (let n = 0; n <= occurrence; n++) i = text.indexOf(NEEDLE, i + 1);
  const start = i + NEEDLE.length;
  return PREFIX + text.slice(start, text.indexOf('`', start));
}

describe('Files read-tail r21 byte-identical prefix dedupe', () => {
  test('canonical constant is the exact shared byte-identical prefix', () => {
    expect(LINK_ATTACHMENT_SELECT_SQL).toBe(PREFIX);
    expect(Buffer.byteLength(LINK_ATTACHMENT_SELECT_SQL, 'utf8')).toBe(35);
    expect(LINK_ATTACHMENT_SELECT_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|\bcount\s*\(/i);
  });

  test('constant is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-dedupe-sql.ts').split('LINK_ATTACHMENT_SELECT_SQL =')).toHaveLength(2);
  });

  test('finance-review and service-repository compose the prefix instead of inlining it', () => {
    const fr = src('finance-review-file-sources.ts');
    const sr = src('attachments/service-repository.ts');
    expect(fr).not.toContain(PREFIX);
    expect(sr).not.toContain(PREFIX);
    expect(fr.match(/\$\{LINK_ATTACHMENT_SELECT_SQL\}/g) ?? []).toHaveLength(1);
    expect(sr.match(/\$\{LINK_ATTACHMENT_SELECT_SQL\}/g) ?? []).toHaveLength(2);
    // Rendered statements stay byte-identical to the pre-dedupe literals.
    expect(rendered(fr)).toBe(
      `${PREFIX} source_id,l.state link_state,s.state set_state,s.purpose_key,a.checksum_sha256,a.media_type,a.bytes,a.source_state\n FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id\n WHERE s.company_id=? AND s.target_type=? AND s.target_id=? AND l.state IN ('draft','bound') ORDER BY s.purpose_key,l.id`,
    );
    expect(rendered(sr, 0)).toBe(
      `${PREFIX},a.checksum_sha256,a.media_type,a.bytes,a.source_state\n      FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id\n      WHERE l.set_id=? AND s.company_id=? AND l.state='draft' ORDER BY l.created_at,l.id`,
    );
    expect(rendered(sr, 1)).toBe(
      `${PREFIX},s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key,s.company_id,\n    a.preview_object_key,a.preview_media_type,a.preview_checksum_sha256,a.preview_bytes,a.source_state\n    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id\n    WHERE l.id=? AND s.company_id=? AND l.state='bound'`,
    );
  });

  test('advisory locks and count(*) pairs remain untouched', () => {
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    expect(sr).toContain('SELECT COALESCE(max(revision),0)+1 AS next_revision');
    expect(src('finance-review-file-sources.ts')).toContain("'(SELECT count(*) FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id");
  });

  test('r10-r20 shared helpers remain defined and unmodified', () => {
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('finance-review-file-sources.ts')).toContain('${LINK_SET_ATTACHMENT_EXISTS_SQL}');
    expect(src('attachments/service-repository.ts')).toContain('LINK_SET_ID_SELECT_SQL');
  });

  test('retirement:ts-statements stays strictly below the r21 BEFORE=316 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(316);
  });
});
