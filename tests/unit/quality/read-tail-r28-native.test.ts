import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(process.cwd());
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const norm = (value: string) => value.replace(/\s+/g, '');

const KIND = 'quality_rd_alert_hold_id';
const SQL = 'SELECT hold_id FROM inventory_quality_alerts WHERE id=?';

const plans = read('packages/server-quality/src/adapters/native_quality_read_sql.rs');
const http = read('packages/server-quality/src/adapters/quality_http.rs');
const purchases = read('packages/server-quality/src/adapters/purchases.ts');
const records = read('packages/server-quality/src/adapters/record-purchases.ts');
const client = read('packages/server-quality/src/adapters/native-quality-read.ts');

/** G4 / retire.quality.native-read-r28: the alert-to-hold point read moves from
 * a TypeScript SELECT to a closed native kind planned by the existing Quality
 * router. The purchase gates keep their unscoped, single-bind semantics and
 * both cross-owner callers stay intact. */
describe('quality read-tail r28 native alert-to-hold point read', () => {
  it('registers one closed kind with byte-exact unscoped SQL and one bind', () => {
    expect(plans).toContain(`"${KIND}"`);
    expect(plans).toContain(`"${SQL}"`);
    expect(plans).toContain('bind_count: 1,');
    expect(SQL.match(/\?/g)).toHaveLength(1);
    expect(SQL).toMatch(/^SELECT /);
    expect(SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
    expect(SQL).not.toContain('company_id');
    expect(SQL).not.toContain('branch_id');
  });

  it('mounts the read-plan handler on the existing Quality router', () => {
    expect(http).toContain('"/api/v1/holdings/{holding_slug}/quality/read-plan"');
    expect(http).toContain('post(read_plan)');
    expect(http).toContain('async fn read_plan(');
    expect(http).toContain('native_quality_read_sql::read_statement(kind)');
    expect(http).toContain('values.len() != plan.bind_count');
  });

  it('keeps byte parity with the retired literal and pins the native client', () => {
    expect(norm(plans)).toContain(norm(SQL));
    expect(purchases).not.toContain(SQL);
    expect(records).not.toContain(SQL);
    expect(client).toContain(`export const RD_ALERT_HOLD_ID = "${KIND}"`);
    expect(purchases).toContain('nativeQualityReadFirst<{hold_id:string}>(RD_ALERT_HOLD_ID,[id],database)');
  });

  it('preserves both cross-owner purchase-gate callers', () => {
    expect(purchases).toContain('target.kind==="alert"?await qualityAlertHoldId(database,target.id)');
    expect(records).toContain('const holdId=await qualityAlertHoldId(database,id);');
  });

  it('registers the five-bind receipt replay probe byte-identically and cuts the TS literal', () => {
    const sql = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
    expect(plans).toContain(`pub const RD_RECEIPT_REPLAY: &str = "quality_rd_receipt_replay"`);
    expect(plans).toContain(`"${sql}"`);
    expect(plans).toContain('RD_RECEIPT_REPLAY => Some(&READ_RECEIPT_REPLAY)');
    expect(plans).toContain('bind_count: 5,');
    expect(sql.match(/\?/g)).toHaveLength(5);
    expect(client).toContain('export const RD_RECEIPT_REPLAY = "quality_rd_receipt_replay"');
    expect(purchases).toContain('nativeQualityReadFirst(RD_RECEIPT_REPLAY,[command.key,command.operation,command.target.id,actor.userId,actor.companyId],database)');
    expect(purchases).not.toContain('FROM command_receipts WHERE command_id=?');
    expect(purchases.match(/evidenceHead\(/g)).toHaveLength(1);
    expect(sql).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
  });

  it('pins the owner TypeScript SELECT KPI at 33 (before 34)', () => {
    const src = resolve(root, 'packages/server-quality/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(15);
  });
});
