import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracle from the retired TypeScript `jsonb_build_object` image, kept as a
// result oracle for the native owner. Rendered with `?` binds before cutover.
const preImage = `SELECT jsonb_build_object('acceptance',to_jsonb(acceptance),
 'reviews',(SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id),
 'revision',(SELECT to_jsonb(revision) FROM company_release_revisions revision WHERE revision.company_id=acceptance.company_id))::text image FROM company_release_acceptances acceptance WHERE acceptance.id=? AND acceptance.company_id=?`;

describe('Platform Administration release child-image native cutover', () => {
  test('the owner renders the byte-identical image SQL with only bind names changed', () => {
    expect(sha(preImage)).toBe('0ebb5e0ec16152ee113377a86f89d1b62045c5a9a985f8e8fc66bbb7de561f81');
    expect(source('adapters/native_lane_33/domain.rs')).toContain('0ebb5e0ec16152ee113377a86f89d1b62045c5a9a985f8e8fc66bbb7de561f81');
    const sql = source('adapters/native_lane_33/postgres.rs');
    expect(sql).toContain("jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id)");
    expect(sql).toContain("jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id)");
    expect(sql).toContain("to_jsonb(revision)");
    expect(sql).toContain("WHERE acceptance.id=$1 AND acceptance.company_id=$2");
  });

  test('the TypeScript source image has no SQL and only the native transport', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text).not.toContain('SELECT ');
    expect(text).not.toContain('releaseChildAggSql');
    expect(text).not.toContain('jsonb_build_object(');
    expect(text).toContain("readNativeReleaseChildImage(actor,id)");
    expect(text).toContain("./native-lane-33/client");
  });

  test('locks and cross-owner organization reads are untouched', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(text).toContain('foundationAttachmentOrganizationSourceLocks(database)');
    expect(text).toContain('foundationAttachmentOrganizationSourceImage(database,actor.companyId)');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
  });

  test('retirement:ts-statements falls from 25 to 23', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(10);
    expect(total).toBeLessThan(25);
  });
});
