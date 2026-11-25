import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { companyReleaseAttachmentReadSourceImage } from '../../../packages/server-platform_administration/src/adapters/company-release-attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

// Historical renderings kept only as owner oracles; none remains inline in TS.
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

describe('Platform Administration r19 native release child-image cutover', () => {
  test('historical renderings remain byte-pinned as oracles', () => {
    for (const key of Object.keys(pre) as Array<keyof typeof pre>) {
      expect(sha(pre[key])).toBe(preSha[key]);
    }
    expect(pre.image.match(/\?/g)).toHaveLength(2);
    expect(pre.image.split('SELECT ').length - 1).toBe(4);
  });

  test('the TypeScript image reader prepares no SQL after cutover', async () => {
    let prepared = 0;
    const db = {
      prepare() {
        prepared += 1;
        return { bind: () => ({ first: () => Promise.resolve(null) }) };
      },
    };
    // No native config: the client fails closed before any SQL can be prepared.
    await expect(
      // @ts-expect-error minimal SqlDatabase stand-in
      companyReleaseAttachmentReadSourceImage(db, {
        companyId: 'c',
        holdingId: 'h001',
        holdingSlug: 'acme',
      }, 'company_release_acceptance', 'id'),
    ).rejects.toMatchObject({ status: 503 });
    expect(prepared).toBe(0);
  });

  test('the native owner owns the image SQL and bounded response', () => {
    expect(source('adapters/native_lane_33/domain.rs')).toContain(preSha.image);
    const sql = source('adapters/native_lane_33/postgres.rs');
    const http = source('adapters/native_lane_33/http.rs');
    expect(http).toContain('"image": image.map(|image| image.image)');
    expect(http).toContain('x-erpx-context-company');
    expect(http).toContain('x-erpx-context-release');
    expect(source('adapters/company-release-attachment-read-sources.ts')).not.toContain('releaseChildAggSql');
  });

  test('locks and cross-owner reads are untouched', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(text).toContain('foundationAttachmentOrganizationSourceLocks(database)');
    expect(text).toContain('foundationAttachmentOrganizationSourceImage(database,actor.companyId)');
  });
});

describe('Platform Administration r19 retirement check', () => {
  test('retirement:ts-statements is pinned at 23 (BEFORE=25)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(23);
    expect(total).toBeLessThan(26);
  });
});
