import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { releaseChildAggSql } from '../../../packages/server-platform_administration/src/adapters/company-release-attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-dedupe literals copied verbatim from HEAD (r12 baseline, BEFORE=36). Used
// only as byte-identity oracles; the two inline aggregates are no longer present.
const preReviewsAgg = 'SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id';
const preEventsAgg = 'SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id';

describe('Platform Administration attachment-source child-aggregate parameterized-family dedupe (r12)', () => {
  test('the shared aggregate renders byte-identical SQL and stays read-only', () => {
    expect(releaseChildAggSql('company_release_gate_reviews', 'review', 'review.criterion_id')).toBe(preReviewsAgg);
    expect(releaseChildAggSql('company_release_events', 'event', 'event.version,event.id')).toBe(preEventsAgg);
    expect(releaseChildAggSql('t', 'a', 'a.id')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the aggregate body survives once in the helper and every family caller is composed', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text.match(/export function releaseChildAggSql/g)).toHaveLength(1);
    expect(text.split('jsonb_agg(to_jsonb(').length - 1).toBe(1);
    expect(text).toContain("releaseChildAggSql('company_release_gate_reviews','review','review.criterion_id')");
    expect(text).toContain("releaseChildAggSql('company_release_events','event','event.version,event.id')");
    expect(text).not.toContain(preReviewsAgg);
    expect(text).not.toContain(preEventsAgg);
  });

  test('locks and the r10/r11 helpers are untouched', () => {
    const text = source('adapters/company-release-attachment-read-sources.ts');
    expect(text).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/release-predicate.ts')).toContain('export function thbCurrencyProbeSql');
    expect(source('adapters/release-predicate.ts')).toContain('export function releaseAcceptanceExistsSql');
  });
});

describe('Platform Administration r12 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r12 BEFORE=36 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(36);
  });
});
