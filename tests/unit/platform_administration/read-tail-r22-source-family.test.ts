import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { platformReadHeads } from '../../../packages/server-platform_administration/src/adapters/platform-read-heads';
import { NATIVE_OWNER_WRITE_NOOP_SQL, selectOneSql } from '../../../packages/server-platform_administration/src/adapters/native-owner-write';
import {
  projectionSourceHeadSql,
  releaseAcceptanceRowSql,
  selectOneFromWhereSql,
} from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { recordsExecutiveUsageSql } from '../../../packages/server-platform_administration/src/adapters/records-ai-usage-projection';
import { platformOwnerProjectionSources } from '@erpx/server-runtime/public/platform-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

describe('Platform Administration native read-head contract family (r22)', () => {
  test('the Rust-owned JSON contract keeps the retired literal bytes', () => {
    const contract = source('adapters/platform-read-head-sources.json');
    expect(sha(contract)).toBe('f87d53d53108140606ff23ed019c3b6944a1213d89a35a5956422f03ba7010aa');
    expect(contract).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|FOR UPDATE|FOR SHARE|LOCK TABLE|pg_advisory/);
    expect(platformReadHeads.selectOne).toBe('SELECT 1');
    expect(platformReadHeads.selectOneFromWhere).toBe('SELECT 1 FROM {from}{gap}WHERE {where}');
    expect(platformReadHeads.selectProjectionFrom).toBe('SELECT {columns} FROM ({projection}) {alias} WHERE {predicate}');
    expect(platformReadHeads.selectRowByPredicate).toBe('SELECT {columns} FROM {table} WHERE {predicate}{suffix}');
    expect(platformReadHeads.recordsUsageProjection).toBe("SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (");
    expect(platformReadHeads.workforceScopeProbe).toBe('SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ');
    expect(platformReadHeads.databaseIdentityRead).toBe('SELECT current_database() name,current_user owner,');
  });

  test('every renderer keeps the pre-fold statement bytes and bind shape', () => {
    expect(selectOneSql()).toBe(NATIVE_OWNER_WRITE_NOOP_SQL);
    expect(selectOneSql()).toBe('SELECT 1');
    expect(selectOneSql(' WHERE probe')).toBe('SELECT 1 WHERE probe');
    expect(selectOneSql(' WHERE a=? AND b=?').match(/\?/g)).toHaveLength(2);

    expect(selectOneFromWhereSql('t', 'x=1')).toBe('SELECT 1 FROM t WHERE x=1');
    expect(selectOneFromWhereSql('t', 'x=1', '\n  ')).toBe('SELECT 1 FROM t\n  WHERE x=1');
    expect(selectOneFromWhereSql('t', 'x=1', ' ')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    expect(projectionSourceHeadSql('columns', 'SRC', 'c', 'c.x=1')).toBe('SELECT columns FROM (SRC) c WHERE c.x=1');
    expect(projectionSourceHeadSql('1', 'SRC', 'c', 'c.company_id=scope_company.id')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    expect(releaseAcceptanceRowSql('version,revoked_at', 'company_id=? AND id=?')).toBe('SELECT version,revoked_at FROM company_release_acceptances WHERE company_id=? AND id=?');
    expect(releaseAcceptanceRowSql('revision', 'company_id=?', ' FOR SHARE', 'company_release_revisions')).toBe('SELECT revision FROM company_release_revisions WHERE company_id=? FOR SHARE');

    expect(recordsExecutiveUsageSql).toBe(`SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (${platformOwnerProjectionSources.executive_generation_jobs}) executive_generation_jobs`);
  });

  test('owner-controlled fragments are inserted literally, never as replacement patterns', () => {
    expect(selectOneFromWhereSql('t.$&', 'x.$1', ' $2')).toBe('SELECT 1 FROM t.$& $2WHERE x.$1');
    expect(projectionSourceHeadSql('c.$&', 'S.$1', 'a.$2', 'p.$3')).toBe('SELECT c.$& FROM (S.$1) a.$2 WHERE p.$3');
    expect(releaseAcceptanceRowSql('c.$&', 'p.$1', ' s.$2', 't.$3')).toBe('SELECT c.$& FROM t.$3 WHERE p.$1 s.$2');
    for (const sql of [
      selectOneFromWhereSql('t.$&', 'x.$1'),
      projectionSourceHeadSql('c.$&', 'S.$1', 'a.$2', 'p.$3'),
      releaseAcceptanceRowSql('c.$&', 'p.$1', '', 't.$3'),
    ]) {
      expect(sql).not.toContain('{from}');
      expect(sql).not.toContain('{where}');
      expect(sql).not.toContain('{columns}');
      expect(sql).not.toContain('{projection}');
      expect(sql).not.toContain('{alias}');
      expect(sql).not.toContain('{predicate}');
      expect(sql).not.toContain('{suffix}');
      expect(sql).not.toContain('{table}');
    }
  });

  test('the replaced TypeScript literals are deleted and bound to the native contract', () => {
    const helper = source('adapters/platform-read-heads.ts');
    expect(helper).toContain('platform-read-head-sources.json');
    expect(helper).not.toContain('SELECT ');
    expect(helper).not.toContain('INSERT ');

    const ownerWrite = source('adapters/native-owner-write.ts');
    expect(ownerWrite).toContain('platformReadHeads.selectOne');
    expect(ownerWrite).not.toContain('SELECT 1');

    const records = source('adapters/records-ai-usage-projection.ts');
    expect(records).toContain('platformReadHeads.recordsUsageProjection');
    expect(records).not.toContain('SELECT company_id');

    const attachment = source('adapters/company-release-attachment-read.ts');
    expect(attachment).toContain('platformReadHeads.workforceScopeProbe');
    expect(attachment).not.toContain('SELECT 1 FROM(SELECT ?::text');

    const engineering = source('adapters/engineering-verification.ts');
    expect(engineering).toContain('platformReadHeads.databaseIdentityRead');
    expect(engineering).not.toContain('SELECT current_database() name');

    const predicate = source('adapters/release-predicate.ts');
    expect(predicate).toContain('platformReadHeads.selectOneFromWhere');
    expect(predicate).toContain('platformReadHeads.selectProjectionFrom');
    expect(predicate).toContain('platformReadHeads.selectRowByPredicate');
    expect(predicate).not.toContain('`SELECT 1 FROM ${from}${gap}WHERE ${where}`');
    expect(predicate).not.toContain('`SELECT ${columns} FROM (${projection}) ${alias} WHERE ${predicate}`');
    expect(predicate).not.toContain('`SELECT ${columns} FROM ${table} WHERE ${predicate}${suffix}`');
  });

  test('locks, the pg_catalog fence, native-write placeholders and native routes are untouched', () => {
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain("releaseAcceptanceRowSql('company_id','company_id=?',' FOR SHARE','engineering_verification_contexts')");
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/engineering-verification.ts')).toContain('pg_catalog.pg_database');
    expect(source('adapters/native-owner-write.ts')).toContain('export const NATIVE_OWNER_WRITE_NOOP_SQL');
    expect(source('adapters/native-owner-write.ts')).toContain('export function selectOneSql');
    expect(source('public/release-predicate.ts')).toContain('releaseRevisionShareLockSql');
  });

  test('retirement:ts-statements drops from the 18-token owner tail to 10', () => {
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
    expect(total).toBeLessThan(18);
  });
});
