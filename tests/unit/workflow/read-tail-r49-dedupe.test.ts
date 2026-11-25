import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { commandReceiptsProjectionSql } from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
import {
  commandReceiptByCommandSql,
  receiptRecordIdSql,
  exportReceiptResultSql,
} from '../../../packages/server-workflow/src/adapters/receipt-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');

/** Byte oracle captured from HEAD 0146e08219e796645b1706246be7c86eeacc42d6 before the r49 dedupe. */
const exportResultOracle = 'SELECT result_json FROM command_receipts WHERE command_id=? AND operation=?';
const recordIdOracle = 'SELECT record_id FROM command_receipts WHERE command_id=?';

describe('command-receipt read-tail dedupe (r49)', () => {
  it('keeps every rendered receipt source byte-identical to the HEAD literal', () => {
    expect(commandReceiptsProjectionSql(['result_json'], ',', ' WHERE command_id=? AND operation=?')).toBe(exportResultOracle);
    expect(commandReceiptsProjectionSql(['record_id'], ',', ' WHERE command_id=?')).toBe(recordIdOracle);
    expect(commandReceiptByCommandSql('result_json')).toBe('SELECT result_json FROM command_receipts WHERE command_id=?');
    expect(receiptRecordIdSql).toBe(recordIdOracle);
    expect(exportReceiptResultSql).toBe(exportResultOracle);
  });

  it('retires the literal SELECT head and adds no writes, locks or native kinds', async () => {
    const receipt = await src('adapters/receipt-sql.ts');
    expect(receipt.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(receipt.match(/commandReceiptsProjectionSql\(/g) ?? []).toHaveLength(2);
    expect(receipt).toContain("commandReceiptsProjectionSql(['result_json'], ',', ' WHERE command_id=? AND operation=?')");
    expect(exportReceiptResultSql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow/i);
  });

  it('anchors the remaining floor singles (no byte-identical r1-r48 helper partner)', async () => {
    const [flow, inbox, aiRequest, hospitality, inventory, worker, aiRevision] = await Promise.all([
      src('adapters/flow-planning-ports.ts'),
      src('adapters/collaboration-inbox-sources.ts'),
      src('adapters/finance-ai-request-sources.ts'),
      src('adapters/hospitality-owner-projection-sources.ts'),
      src('adapters/inventory-audit-history-source.ts'),
      src('adapters/project-worker-startup-evidence.ts'),
      src('adapters/finance-ai-revision-sources.ts'),
    ]);
    // approvalIndex + expense/payroll CASE subqueries: one literal each, reused in-place.
    expect(flow).toContain("const approvalIndex = `(SELECT current_step FROM approval_instances");
    expect(flow).toContain("EXISTS(SELECT 1 FROM approval_steps s JOIN approval_instances i");
    expect(flow).toContain("COALESCE((SELECT CASE status WHEN 'draft' THEN 'payment_draft'");
    // Collaboration dead-letter count moved to the lane-32 native reader; the
    // switched owner adapter now carries no SQL. Finance request-AI month head,
    // hospitality decision projection remain floor singles.
    expect(inbox).not.toContain('SELECT count(*)::int count FROM outbox_events');
    expect(inbox).toContain("from './native-lane-32/client'");
    expect(aiRequest).toContain('`SELECT company_id,month ${recordsFinalSources.');
    expect(hospitality).toContain('approval_decisions: "SELECT');
    // Inventory history head, project-worker audit/outbox evidence, finance source revision.
    expect(inventory).toContain('SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM audit_events a');
    expect(worker).toContain("audit:'SELECT command_id,action,record_id,record_version FROM audit_events'");
    expect(worker).toContain("outbox:'SELECT id,command_id,event_type,aggregate_id,aggregate_version,payload_json FROM outbox_events'");
    expect(aiRevision).toContain("`(SELECT count(*) FROM command_receipts WHERE authority_json::jsonb->>'companyId'=?");
  });

  it('pins the exact per-file SELECT-head distribution after the r49 cut', async () => {
    const expected: Record<string, number> = {
      'adapters/approval-participant-projection.ts': 3,
      'adapters/approval-planning.ts': 3,
      'adapters/approval-projection-sources.ts': 5,
      'adapters/closed-projection-sources.ts': 2,
      'adapters/finance-ai-request-sources.ts': 1,
      'adapters/finance-ai-revision-sources.ts': 1,
      'adapters/flow-planning-ports.ts': 6,
      'adapters/hospitality-owner-projection-sources.ts': 1,
      'adapters/inventory-audit-history-source.ts': 1,
      'adapters/json-each-sources.ts': 1,
      'adapters/project-worker-startup-evidence.ts': 2,
      'adapters/work-package-obligation-sources.ts': 1,
    };
    const seen: Record<string, number> = {};
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) {
          const rel = p.slice(srcRoot.length + 1);
          const n = ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
          if (n) seen[rel] = n;
        }
      }
    };
    await walk(srcRoot);
    expect(seen).toEqual(expected);
    expect(expected['adapters/receipt-sql.ts'] ?? 0).toBe(0);
  });

  it('retirement:ts-statements is 27 after the lane-32 failed-metrics cut (28->27)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(30);
    expect(total).toBe(27);
  });
});
