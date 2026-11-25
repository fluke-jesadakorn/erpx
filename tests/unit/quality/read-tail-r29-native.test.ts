import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd());
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

const plans = read('packages/server-quality/src/adapters/native_quality_read_sql.rs');
const http = read('packages/server-quality/src/adapters/quality_http.rs');
const client = read('packages/server-quality/src/adapters/native-quality-read.ts');
const records = read('packages/server-quality/src/adapters/record-purchases.ts');
const service = read('packages/server-quality/src/adapters/attachments-service-facts.ts');
const operational = read('packages/server-quality/src/adapters/attachments-operational-facts.ts');
const armHead = read('packages/server-quality/src/adapters/quality-source-arm-head.ts');

/** G4 / retire.quality.native-read-r30. The r29 design was measured at 31 but
 * reverted while the tree briefly showed 33. This witness re-pins the closed
 * kinds, the removed TypeScript mirrors, the assertion delegation shape and
 * the owner TypeScript statement KPI. */
const kinds = [
  { kind: 'quality_rd_alert_hold_id', const: 'RD_ALERT_HOLD_ID', sql: 'SELECT hold_id FROM inventory_quality_alerts WHERE id=?', binds: 1 },
  { kind: 'quality_rd_hold_exists', const: 'RD_HOLD_EXISTS', sql: 'SELECT 1 FROM inventory_quality_holds WHERE id=?', binds: 1 },
  { kind: 'quality_rd_spec_exists', const: 'RD_SPEC_EXISTS', sql: 'SELECT 1 FROM inventory_quality_specs WHERE id=?', binds: 1 },
  { kind: 'quality_rd_attach_hold_cas', const: 'RD_ATTACH_HOLD_CAS', sql: 'EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)', binds: 3 },
  { kind: 'quality_rd_attach_spec_cas', const: 'RD_ATTACH_SPEC_CAS', sql: 'EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)', binds: 3 },
] as const;

describe('quality read-tail r29 native hold/spec probes and CAS guards', () => {
  it('registers five closed kinds with exact SQL and bind counts', () => {
    expect(plans.match(/pub const RD_[A-Z_]+: &str = /g)).toHaveLength(5);
    for (const { kind, const: k, sql, binds } of kinds) {
      expect(plans).toContain(`pub const ${k}: &str = "${kind}";`);
      expect(plans).toContain(`"${sql}"`);
      expect(plans).toContain(`${k} => Some(&READ_${k.slice(3)})`);
      expect(sql.match(/\?/g)).toHaveLength(binds);
      expect(plans).toContain(`bind_count: ${binds},`);
      expect(sql).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
    }
    expect(http).toContain('native_quality_read_sql::read_statement(kind)');
    expect(http).toContain('values.len() != plan.bind_count');
    expect(plans).toContain('fn unknown_kinds_fail_closed()');
  });

  it('keeps byte parity with the retired TypeScript probe and guard literals', () => {
    const norm = (value: string) => value.replace(/\s+/g, '');
    for (const { sql } of kinds) expect(norm(plans)).toContain(norm(sql));
    // Former qualitySourceIdExistsSql(table,'', '?') output.
    expect(kinds[1].sql).toBe('SELECT 1 FROM inventory_quality_holds WHERE id=?');
    expect(kinds[2].sql).toBe('SELECT 1 FROM inventory_quality_specs WHERE id=?');
    // Former qualityAttachmentVersionGuardSql(table) output.
    expect(kinds[3].sql).toBe('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)');
    expect(kinds[4].sql).toBe('EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)');
    // Former aliased output is reproduced byte-for-byte by the shared arm head.
    expect(armHead).toContain('=> `SELECT ${columns} FROM ${source}${predicate}`');
  });

  it('exports the kinds and plans guards through the unchanged assertion bridge', () => {
    for (const { const: k, kind } of kinds) expect(client).toContain(`export const ${k} = "${kind}";`);
    expect(client).toContain('export async function nativeQualityReadSql(');
    expect(client).toContain('export async function nativeQualityReadFirst<T>(');
    expect(client).toContain('export function nativeQualityGuardAssertion(');
    expect(client).toContain('const statement=await nativeQualityReadSql(kind,values);');
    expect(client).toContain('const guard=assertion(db,commandId,step,statement.sql,statement.values as (string|number|null)[]);');
    expect(client).toContain('return (context?guard.withContext(context):guard).run();');
    expect(client).toContain('__nativeWrite:true');
    expect(client).toContain('withContext(next:GuardContext){context=next;return descriptor;}');
    expect(client).toContain('import {assertion} from "@erpx/server-workflow/public/command-assertions";');
  });

  it('rewires callers and leaves no TypeScript SQL mirror helper', () => {
    expect(records.match(/qualitySourceArmHead\(/g)).toHaveLength(6);
    expect(records).toContain('nativeQualityReadFirst(RD_HOLD_EXISTS,[id],database)');
    expect(records).toContain('nativeQualityReadFirst(RD_SPEC_EXISTS,[id],database)');
    expect(service).toContain('RD_ATTACH_HOLD_CAS');
    expect(operational).toContain('RD_ATTACH_SPEC_CAS');
    expect(service.match(/nativeQualityGuardAssertion\(/g)).toHaveLength(1);
    expect(operational.match(/nativeQualityGuardAssertion\(/g)).toHaveLength(1);
    for (const text of [records, service, operational, client]) {
      expect(text).not.toContain('quality-record-source-sql');
      expect(text).not.toContain('qualitySourceIdExistsSql');
      expect(text).not.toContain('qualityAttachmentVersionGuardSql');
    }
    expect(() => read('packages/server-quality/src/adapters/quality-record-source-sql.ts')).toThrow();
  });

  it('pins the owner TypeScript SELECT KPI at 31 (before 33)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-quality/src'));
    expect(total).toBe(25);
  });
});
