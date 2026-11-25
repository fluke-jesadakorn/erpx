import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const repo = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(repo, path), 'utf8');
const exists = (path: string) => existsSync(resolve(repo, path));

const PROJECTION = 'packages/server-field_service/src/adapters/final_root_projection.rs';
const CONSUMER = 'packages/server-field_service/src/adapters/final_root_consumer.rs';
const CONSUMER_TEST = 'tests/unit/field_service/rust-final-root-consumer.test.rs';
const PROJECTION_TEST = 'tests/unit/field_service/rust-final-root-projection.test.rs';
// Cross-owner retained consumer; read-only here, cutover belongs to its owner.
const TIMELINE = 'packages/server-customer_360/src/adapters/timeline.ts';

const SCOPE = 'g.company_id=? AND (?::text IS NULL OR g.branch_id=?) AND (?::text IS NULL OR g.department_id=?)';

describe('field_service shared projection sources are native-owned', () => {
  test('the retired in-path TS projection authority stays deleted', () => {
    expect(exists('packages/server-field_service/src/adapters/final-root-owner-projection-sources.ts')).toBe(false);
    expect(exists('packages/server-field_service/src/public/final-root-owner-projection-sources.ts')).toBe(false);
    expect(exists('packages/server-field_service/src/adapters/executive-metric-projections.ts')).toBe(false);
    expect(exists('packages/server-field_service/src/public/executive-metric-projections.ts')).toBe(false);
    for (const file of [
      'packages/server-field_service/src/adapters/record-purchases.ts',
      'packages/server-field_service/src/adapters/finance-record-sources.ts',
      'packages/server-field_service/src/public/record-purchases.ts',
    ]) expect(read(file)).not.toContain('finalRootOwnerProjectionSources');
  });

  test('all five projection sources are owner-fixed Rust statements, not TS authority', () => {
    const rust = read(PROJECTION);
    expect(rust).toContain('pub enum FinalRootProjectionSource');
    expect(rust).toContain('pub const ALL: [Self; 5]');
    for (const variant of ['Agreements', 'AgreementEvents', 'BillingReleases', 'OrderEvents', 'Orders']) {
      expect(rust, variant).toContain(`Self::${variant} =>`);
    }
    expect(rust).not.toContain('include_str!');
    const test = read(PROJECTION_TEST);
    expect(test).toContain('const NATIVE_SQL: [(FinalRootProjectionSource, &str); 5]');
    expect(test).toContain('native_sql_matches_the_native_owned_oracle_statements');
    expect(test).not.toContain('TS_AUTHORITY');
    expect(test).not.toContain('include_str!');
  });

  test('native scoped_service composition matches the retained cross-owner binding shape', () => {
    const consumer = read(CONSUMER);
    expect(consumer).toContain('SELECT g.id,g.customer_id,g.branch_id,g.department_id FROM ({}) g WHERE ' + SCOPE);
    expect(consumer).toContain('FinalRootProjectionSource::Agreements.sql()');
    expect(consumer).toContain('A caller supplies a scope,');
    const test = read(CONSUMER_TEST);
    expect(test).toContain('CONSUMER_SCOPED_SERVICE_SUFFIX: &str =\n    ") g WHERE ' + SCOPE + '"');
    expect(test).toContain('scope_binding_order_matches_the_ts_consumer');
    const tokens = test.replace(/\s+/g, '');
    expect(tokens).toContain('[Some("company"),Some("branch-1"),Some("branch-1"),Some("dept-1"),Some("dept-1"),]');
    expect(tokens).toContain('[Some("company"),None,None,None,None]');
    // The retained Customer 360 TS copy still uses the same predicate and five
    // binds; this is the exact cross-owner tail awaiting its owner cutover.
    const timeline = read(TIMELINE);
    expect(timeline).toContain(SCOPE);
    expect(timeline).toContain("scopedProjection('g.id,g.customer_id,g.branch_id,g.department_id','field_service_agreements g'");
  });

  test('finance lineage and single-order source remain owner-owned Rust ports', () => {
    expect(read('packages/server-field_service/src/adapters/finance_lineage.rs')).toContain('include_str!("finance-lineage-sources.json")');
    expect(read('packages/server-field_service/src/public/finance_lineage.rs')).toContain('field_finance_source');
    expect(read('packages/server-field_service/src/adapters/source_record.rs')).toContain('pub async fn read_source_record');
    expect(read('packages/server-field_service/src/public/source_record.rs')).toContain('read_source_record');
  });

  test('retirement:ts-statements stays 0 in both in-path TS surfaces (BEFORE=0)', () => {
    for (const root of ['packages/server-field_service/src', 'packages/feature-field_service/src']) {
      const files: string[] = [];
      const walk = (dir: string) => {
        for (const entry of readdirSync(resolve(repo, dir), { withFileTypes: true })) {
          const path = `${dir}/${entry.name}`;
          if (entry.isDirectory()) walk(path);
          else if (entry.name.endsWith('.ts')) files.push(path);
        }
      };
      walk(root);
      let total = 0;
      for (const file of files) total += (read(file).match(/SELECT /g) ?? []).length;
      expect(total, root).toBe(0);
    }
  });
});
