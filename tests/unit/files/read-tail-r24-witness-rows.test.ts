import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { jsonbRowsSql } from '../../../modules/files/server/src/adapters/social-upload-cleanup-rows-sql';

const srcRoot = resolve(__dirname, '../../../modules/files/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const readOnly = (sql: string) => {
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|\bcount\s*\(/i);
};

const selectCount = (text: string) => (text.match(/SELECT\s/g) ?? []).length;
const literalSelectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

const STEPS_WHERE = "s.company_id=? AND s.target_type='social_draft' AND s.target_id=?";
const EVENTS_WHERE = 'e.attachment_id IN(SELECT id FROM files) OR e.link_id IN(SELECT id FROM links) OR e.set_id IN(SELECT id FROM sets)';

describe('Files read-tail r24 witness-array dedupe', () => {
  test('canonical helper reproduces every original witness-array literal byte for byte', () => {
    expect(jsonbRowsSql('a', 'files', 'a.id'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM files a),'[]'::jsonb)");
    expect(jsonbRowsSql('u', 'uploads', 'u.id'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM uploads u),'[]'::jsonb)");
    expect(jsonbRowsSql('s', 'sets', 's.id'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM sets s),'[]'::jsonb)");
    expect(jsonbRowsSql('l', 'links', 'l.id'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM links l),'[]'::jsonb)");
    expect(jsonbRowsSql('e', 'attachment_events', 'e.id', EVENTS_WHERE))
      .toBe(`COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM attachment_events e WHERE ${EVENTS_WHERE}),'[]'::jsonb)`);
    expect(jsonbRowsSql('c', 'command_receipts', 'c.command_id', 'c.command_id IN(SELECT bound_command_id FROM sets)'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.command_id) FROM command_receipts c WHERE c.command_id IN(SELECT bound_command_id FROM sets)),'[]'::jsonb)");
    expect(jsonbRowsSql('a', 'audit_events', 'a.id', 'a.command_id IN(SELECT bound_command_id FROM sets)'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a WHERE a.command_id IN(SELECT bound_command_id FROM sets)),'[]'::jsonb)");
    expect(jsonbRowsSql('s', '(STEP_SOURCE)', 's.next_version,s.command_id', STEPS_WHERE))
      .toBe(`COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.next_version,s.command_id) FROM (STEP_SOURCE) s WHERE ${STEPS_WHERE}),'[]'::jsonb)`);
    expect(jsonbRowsSql('p', 'attachment_publications', 'p.id', 'p.source_link_id IN(SELECT id FROM links)'))
      .toBe("COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM attachment_publications p WHERE p.source_link_id IN(SELECT id FROM links)),'[]'::jsonb)");
    for (const rendered of [jsonbRowsSql('a', 'files', 'a.id'), jsonbRowsSql('e', 'attachment_events', 'e.id', EVENTS_WHERE)])
      readOnly(rendered);
  });

  test('helper is defined exactly once and all nine witnesses compose it', () => {
    expect(src('social-upload-cleanup-rows-sql.ts').split('jsonbRowsSql =').length - 1).toBe(1);
    const cleanup = src('social-upload-cleanup.ts');
    expect(cleanup.match(/\$\{jsonbRowsSql\(/g) ?? []).toHaveLength(9);
    expect(cleanup).not.toContain('COALESCE((SELECT jsonb_agg(');
    expect(cleanup).toContain("jsonbRowsSql('s',`(${recordAttachmentVersionSteps})`");
  });

  test('locks, advisory statements and r10-r23 helpers remain untouched', () => {
    expect(src('social-upload-cleanup.ts'))
      .toContain("db.prepare('LOCK TABLE attachments,upload_intents,attachment_links,attachment_sets,attachment_events,attachment_publications,audit_events IN SHARE ROW EXCLUSIVE MODE')");
    const sr = src('attachments/service-repository.ts');
    expect(sr).toContain('const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(sr).toContain('const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE"');
    expect(sr).toContain('SELECT count(DISTINCT candidate.id)');
    const tail = src('read-tail-dedupe-sql.ts');
    for (const name of [
      'attachmentSetPurposeSql', 'workforceActorScopeSql', 'scansAggSql', 'SCANS_WITNESS_SQL',
      'NO_ATTACHMENT_LINK_SQL', 'LINK_SET_ATTACHMENT_EXISTS_SQL', 'LINK_SET_ID_SELECT_SQL',
      'LINK_ATTACHMENT_SELECT_SQL', 'LINK_SET_WHERE_EXISTS_SQL', 'PACKAGE_OBLIGATION_SCOPE_SQL',
    ]) expect(tail).toContain(`export const ${name} =`);
    expect(src('commerce-delivery-dedupe-sql.ts')).toContain('export const COMMERCE_DELIVERY_GUARD_SQL =');
  });

  test('retirement:ts-statements stays strictly below the r24 BEFORE=300 baseline', () => {
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
    expect(literal).toBeLessThan(300);
    expect(total).toBeLessThan(301);
  });
});
