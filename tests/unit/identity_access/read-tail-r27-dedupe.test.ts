import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  activeWorkContextAssignmentSql, assignmentValidUntilSql, auditFeedSql,
  iamConnectionByIdSql, newestPageSql, principalDisplaySql, memberByCompanyUserSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r27';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const helper = read('adapters/read-tail-family-r27.ts');
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

describe('Identity access read-tail r27 seven parameterized families', () => {
  test('each rendered head is byte-identical to its pre-dedupe literal', () => {
    expect(iamConnectionByIdSql('company_id=? AND id=?')).toBe('SELECT * FROM iam_connections WHERE company_id=? AND id=?');
    expect(iamConnectionByIdSql('id=$1 AND company_id=$2 AND status=\'active\' AND last_sync_at>$3'))
      .toBe('SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status=\'active\' AND last_sync_at>$3');
    expect(`${auditFeedSql(' AND (action ILIKE ? OR actor_id ILIKE ? OR target_id ILIKE ?) AND (?::bigint IS NULL OR created_at<? OR (created_at=? AND id<?))')} ORDER BY created_at DESC,id DESC LIMIT 101`)
      .toBe('SELECT * FROM iam_audit WHERE company_id=? AND (action ILIKE ? OR actor_id ILIKE ? OR target_id ILIKE ?) AND (?::bigint IS NULL OR created_at<? OR (created_at=? AND id<?)) ORDER BY created_at DESC,id DESC LIMIT 101');
    expect(`${auditFeedSql('')} ORDER BY created_at DESC,id DESC LIMIT 100`)
      .toBe('SELECT * FROM iam_audit WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 100');
    expect(`${activeWorkContextAssignmentSql('user_id=? AND expires_at>? AND assignment_id IS NOT NULL')} ORDER BY updated_at DESC`)
      .toBe('SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND user_id=? AND expires_at>? AND assignment_id IS NOT NULL ORDER BY updated_at DESC');
    expect(newestPageSql('iam_migrations', 100)).toBe('SELECT * FROM iam_migrations WHERE company_id=? ORDER BY created_at DESC LIMIT 100');
    expect(newestPageSql('iam_access_review_campaigns', 50)).toBe('SELECT * FROM iam_access_review_campaigns WHERE company_id=? ORDER BY created_at DESC LIMIT 50');
    expect(`${principalDisplaySql('lower(display_name) LIKE ?')} ORDER BY display_name LIMIT 8`)
      .toBe('SELECT user_id,display_name FROM principals WHERE status=\'active\' AND lower(display_name) LIKE ? ORDER BY display_name LIMIT 8');
    expect(memberByCompanyUserSql('status,login_policy', '$')).toBe('SELECT status,login_policy FROM iam_members WHERE company_id=$1 AND user_id=$2');
    expect(memberByCompanyUserSql('person_id')).toBe('SELECT person_id FROM iam_members WHERE company_id=? AND user_id=?');
    expect(assignmentValidUntilSql()).toBe('SELECT valid_until FROM assignments WHERE id=$1');
    expect(assignmentValidUntilSql('platform_assignments')).toBe('SELECT valid_until FROM platform_assignments WHERE id=$1');
  });

  test('one owner helper defines exactly seven heads; every call site uses it and no old literal remains', () => {
    expect((helper.match(/SELECT /g) ?? []).length).toBe(7);
    for (const name of [
      'iamConnectionByIdSql', 'auditFeedSql', 'activeWorkContextAssignmentSql',
      'newestPageSql', 'principalDisplaySql', 'memberByCompanyUserSql', 'assignmentValidUntilSql',
    ]) expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);

    for (const rel of [
      'adapters/directory-lookup.ts', 'adapters/directory-operation-preview.ts', 'adapters/iam-migrations.ts',
    ]) expect(calls(rel, 'iamConnectionByIdSql'), rel).toBe(2);
    expect(calls('adapters/iam-service-reads.ts', 'auditFeedSql')).toBe(3);
    expect(calls('adapters/iam-service-reads.ts', 'newestPageSql')).toBe(3);
    expect(calls('adapters/executive-report-sources.ts', 'activeWorkContextAssignmentSql')).toBe(3);
    expect(calls('adapters/records-records-owned-facts.ts', 'principalDisplaySql')).toBe(2);
    expect(calls('adapters/collaboration-mention-principals.ts', 'principalDisplaySql')).toBe(2);
    expect(calls('adapters/iam-grant-transaction.ts', 'memberByCompanyUserSql')).toBe(2);
    expect(calls('adapters/iam-inspector.ts', 'memberByCompanyUserSql')).toBe(2);
    expect(calls('adapters/iam-grant-transaction.ts', 'assignmentValidUntilSql')).toBe(2);
    expect(calls('adapters/technology.ts', 'assignmentValidUntilSql')).toBe(2);

    const inline = [
      'SELECT * FROM iam_connections WHERE company_id=? AND id=?',
      'SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2',
      'SELECT * FROM iam_audit WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 100',
      'SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND user_id=? AND expires_at>? AND assignment_id IS NOT NULL',
      'SELECT * FROM iam_migrations WHERE company_id=? ORDER BY created_at DESC LIMIT 100',
      'SELECT * FROM iam_access_review_campaigns WHERE company_id=? ORDER BY created_at DESC LIMIT 50',
      'SELECT user_id,display_name FROM principals WHERE status=\'active\' AND lower(display_name) LIKE ?',
      'SELECT status,login_policy FROM iam_members WHERE company_id=$1 AND user_id=$2',
      'SELECT person_id FROM iam_members WHERE company_id=? AND user_id=?',
      'SELECT valid_until FROM assignments WHERE id=$1',
    ];
    for (const rel of [
      'adapters/directory-lookup.ts', 'adapters/directory-operation-preview.ts', 'adapters/iam-migrations.ts',
      'adapters/iam-service-reads.ts', 'adapters/executive-report-sources.ts',
      'adapters/records-records-owned-facts.ts', 'adapters/collaboration-mention-principals.ts',
      'adapters/iam-grant-transaction.ts', 'adapters/iam-inspector.ts', 'adapters/technology.ts',
    ]) {
      const src = read(rel);
      // A quoted literal ending marks the old inline call; prefixes of the
      // deliberately retained FOR SHARE lock head do not count.
      for (const literal of inline) expect(new RegExp(`${esc(literal)}['"\`]`).test(src), `${rel} still inlines ${literal}`).toBe(false);
    }
  });

  test('locks, session authority, native kinds and earlier helpers are untouched', () => {
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // Lock statements keep their byte identity, including the deliberately excluded FOR SHARE head.
    expect(hash('adapters/product-sso-store.ts')).toBe('967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441');
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/workflow-command-authority.ts')).toBe('e90999b7b107adf75838dea69b2f95807624799f6c3519d705566632b0d04275');
    expect(read('adapters/iam-migrations.ts')).toContain('SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status=\'active\' AND last_sync_at>$3 FOR SHARE');
    // Session authority families keep their byte identity.
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e9966b74188ce0763c43a50ddc0f98c6c113646d21296c75188dd158d742a172');
    expect(hash('adapters/native-session.ts')).toBe('1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1');
    // Native (Rust) kinds and earlier-round helpers are not edited.
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
    expect(hash('adapters/pms-recipient-projection-family-r26.ts')).toBe('60917ed0d495413680ce96a5d97dcfff89ef31eaa6d8bef480c0e65161f1e7c4');
    expect(hash('adapters/active-role-probe-family-r25.ts')).toBe('aea7ee49d4c93ac3c904b5f8190119a277ad43921a21a70903cfdf5c0455d448');
  });
});

describe('Identity access read-tail r27 retirement check', () => {
  test('retirement:ts-statements is strictly lower than BEFORE=516 (expected 508)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(516);
    expect(total).toBeLessThanOrEqual(508);
  });
});
