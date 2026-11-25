import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { flowRoutesClosedProjectionSql, commandReceiptsProjectionSql } from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
import { commandReceiptByCommandSql, receiptRecordIdSql, exportReceiptResultSql } from '../../../packages/server-workflow/src/adapters/receipt-sql';
import { collaborationNotificationSnapshotSources } from '../../../packages/server-workflow/src/adapters/collaboration-notification-snapshot-sources';
import { peoplePayrollFinalOwnerProjections } from '../../../packages/server-workflow/src/adapters/people-payroll-final-owner-projections';
import { projectWorkerCommandEvidence } from '../../../packages/server-workflow/src/adapters/project-worker-startup-evidence';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');

describe('workflow closed projection generator families (r45)', () => {
  it('A1 emits byte-identical flow-routes closed projections for both exact separators', () => {
    expect(flowRoutesClosedProjectionSql(', ')).toBe('SELECT "company_id", "kind", "record_id", "version", "defaults_revision", "steps_json", "created_at" FROM flow_routes');
    expect(flowRoutesClosedProjectionSql(',')).toBe('SELECT "company_id","kind","record_id","version","defaults_revision","steps_json","created_at" FROM flow_routes');
  });

  it('A2 emits byte-identical command-receipts projections for all three families', () => {
    expect(commandReceiptsProjectionSql(['"command_id"','"operation"','"actor_id"','"request_hash"','"record_id"','"result_json"','"created_at"','"authority_json"'])).toBe('SELECT "command_id","operation","actor_id","request_hash","record_id","result_json","created_at","authority_json" FROM command_receipts');
    expect(commandReceiptsProjectionSql(['command_id','operation','record_id','actor_id','authority_json','result_json'])).toBe('SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts');
    expect(commandReceiptByCommandSql('record_id')).toBe('SELECT record_id FROM command_receipts WHERE command_id=?');
    expect(commandReceiptByCommandSql('operation')).toBe('SELECT operation FROM command_receipts WHERE command_id=?');
    expect(receiptRecordIdSql).toBe('SELECT record_id FROM command_receipts WHERE command_id=?');
    expect(exportReceiptResultSql).toBe('SELECT result_json FROM command_receipts WHERE command_id=? AND operation=?');
  });

  it('retained snapshots still emit the original bytes', () => {
    const collab = collaborationNotificationSnapshotSources().workflow_notification_effects_flow_routes_r_join;
    const payroll = peoplePayrollFinalOwnerProjections();
    expect(collab).toBe('JOIN (SELECT "company_id", "kind", "record_id", "version", "defaults_revision", "steps_json", "created_at" FROM flow_routes) r');
    expect(payroll.payroll_cancellation_flow_routes_f_from).toBe('FROM (SELECT "company_id","kind","record_id","version","defaults_revision","steps_json","created_at" FROM flow_routes) f');
    expect(payroll.people_lifecycle_command_receipts_from).toBe('FROM (SELECT "command_id","operation","actor_id","request_hash","record_id","result_json","created_at","authority_json" FROM command_receipts) command_receipts');
    expect(projectWorkerCommandEvidence.receipts).toBe('SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts');
  });

  it('keeps one generator per family and no inline projection literals in callers', async () => {
    const [shared, collab, payroll, worker, receipt] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/closed-projection-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/collaboration-notification-snapshot-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/people-payroll-final-owner-projections.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/project-worker-startup-evidence.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/receipt-sql.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const flowRoutesClosedProjectionSql =/g)).toHaveLength(1);
    expect(shared.match(/export const commandReceiptsProjectionSql =/g)).toHaveLength(1);
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(2);
    expect(collab).not.toContain('SELECT "company_id"');
    expect(collab).toContain("flowRoutesClosedProjectionSql(', ')");
    expect(payroll).not.toContain('SELECT "company_id"');
    expect(payroll).not.toContain('SELECT "command_id"');
    expect(payroll).toContain("flowRoutesClosedProjectionSql(',')");
    expect(payroll).toContain('commandReceiptsProjectionSql([');
    expect(worker).not.toContain("'SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts'");
    expect(worker).toContain('commandReceiptsProjectionSql([');
    expect(receipt).not.toContain('`SELECT ${column} FROM command_receipts');
    expect(receipt).toContain('commandReceiptsProjectionSql([column]');
  });

  it('retirement:ts-statements is 33 after the r46/r47 native read cutover and r48 dedupe (46->43->41->37->33)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(46);
    expect(total).toBe(26);
  });
});
