import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import {
  releaseChildAggSql,
  companyReleaseAttachmentReadSourceImage,
} from '../../../packages/server-platform_administration/src/adapters/company-release-attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Byte oracles captured from the BEFORE=27 source (HEAD 6d08598b3c2…). They are
// rendered output only: neither the revision head nor the three subquery heads
// remain inline after the r19 fold.
const pre = {
  reviews: 'SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id',
  events: 'SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id',
  revision: 'SELECT to_jsonb(revision) FROM company_release_revisions revision WHERE revision.company_id=acceptance.company_id',
  image: `SELECT jsonb_build_object('acceptance',to_jsonb(acceptance),
 'reviews',(SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id),
 'revision',(SELECT to_jsonb(revision) FROM company_release_revisions revision WHERE revision.company_id=acceptance.company_id))::text image FROM company_release_acceptances acceptance WHERE acceptance.id=? AND acceptance.company_id=?`,
};

const preSha: Record<keyof typeof pre, string> = {
  reviews: 'cdcd5b91ac9ef016f5b78018bf4f1fa6a4f3d2a77ecac7ed5af58db89665c031',
  events: '489f1812dfbfbfb2aed31d6d63bb28ba4b51e3adb7db58c16c3d1280345373ae',
  revision: '32efe3780e824a1ad54257b03c06544b9594603470a8743d6c3da5d6ae7e1a04',
  image: '0ebb5e0ec16152ee113377a86f89d1b62045c5a9a985f8e8fc66bbb7de561f81',
};

const renderImage = async () => {
  let sql = '';
  const db = {
    prepare(statement: string) {
      sql = statement;
      return { bind: () => ({ first: () => Promise.resolve(null) }) };
    },
  };
  // @ts-expect-error minimal SqlDatabase stand-in captures the prepared statement
  await companyReleaseAttachmentReadSourceImage(db, { companyId: 'c' }, 'company_release_acceptance', 'id');
  return sql;
};

describe('Platform Administration r19 revision-scalar fold', () => {
  test('every rendered statement keeps its exact bytes and SELECT head count', async () => {
    const after = {
      reviews: releaseChildAggSql('company_release_gate_reviews', 'review', 'review.criterion_id'),
      events: releaseChildAggSql('company_release_events', 'event', 'event.version,event.id'),
      revision: releaseChildAggSql('company_release_revisions', 'revision', '', false),
      image: await renderImage(),
    };
    for (const key of Object.keys(pre) as Array<keyof typeof pre>) {
      expect(after[key]).toBe(pre[key]);
      expect(sha(after[key])).toBe(preSha[key]);
    }
    for (const key of ['reviews', 'events', 'revision'] as const) {
      expect(after[key].split('SELECT ').length - 1).toBe(1);
    }
    expect(after.image.split('SELECT ').length - 1).toBe(4);
    expect(pre.image.match(/\?/g)).toHaveLength(2);
  });

  test('the scalar revision renders through the single parameterized family head', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text.match(/export function releaseChildAggSql/g)).toHaveLength(1);
    expect(text.split('jsonb_agg(to_jsonb(').length - 1).toBe(1);
    expect(text.split('`SELECT ${projection} FROM ${table} ${alias} WHERE ${predicate}`').length - 1).toBe(1);
    expect(text).toContain("releaseChildAggSql('company_release_revisions','revision','',false)");
    expect(text).toContain("releaseChildAggSql('company_release_gate_reviews','review','review.criterion_id')");
    expect(text).toContain("releaseChildAggSql('company_release_events','event','event.version,event.id')");
    expect(text).not.toContain(pre.revision);
    expect(text).not.toContain(pre.image);
  });

  test('locks, authority, native reads and cross-owner sources are untouched', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(text).toContain("database.prepare('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE')");
    expect(text).toContain('foundationAttachmentOrganizationSourceLocks(database)');
    expect(text).toContain('foundationAttachmentOrganizationSourceImage(database,actor.companyId)');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE');
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/engineering-verification.ts')).toContain('pg_catalog.pg_database');
    expect(source('adapters/native-owner-write.ts')).toContain('export const NATIVE_OWNER_WRITE_NOOP_SQL');
    expect(source('adapters/release-predicate.ts')).toContain('iam_assignment_active(reviewer.id');
  });
});

describe('Platform Administration r19 retirement check', () => {
  test('retirement:ts-statements is pinned at 26 (BEFORE=27)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(26);
    expect(total).toBeLessThan(27);
  });
});
