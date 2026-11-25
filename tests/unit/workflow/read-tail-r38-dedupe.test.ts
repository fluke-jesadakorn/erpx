import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');

/** Byte oracles captured from the retired r38 generator before the native cut. */
const SNAPSHOT_ORACLE = 'SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? ORDER BY created_at,id';
const RECORD_ORACLE = 'SELECT action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? AND kind=? AND record_id=? ORDER BY created_at,id';

describe('company-scoped flow-event history family (r38 generator, retired by the native read cut)', () => {
  it('keeps both retired literals byte-identical in the owner Rust read registry', async () => {
    const rust = await src('domain/workflow_wave3.rs');
    expect(rust).toContain('pub const WF_NATIVE_FLOW_EVENTS_HISTORY: &str = "wf_native_flow_events_history";');
    expect(rust).toContain('pub const WF_NATIVE_FLOW_EVENTS_RECORD_HISTORY: &str = "wf_native_flow_events_record_history";');
    expect(rust).toContain(`"${SNAPSHOT_ORACLE}"`);
    expect(rust).toContain(`"${RECORD_ORACLE}"`);
    expect((SNAPSHOT_ORACLE.match(/\?/g) ?? []).length).toBe(1);
    expect((RECORD_ORACLE.match(/\?/g) ?? []).length).toBe(3);
  });

  it('deletes the TS generator and leaves no flow_events SQL in the adapter', async () => {
    const flow = await src('adapters/flow-planning-ports.ts');
    expect(flow).not.toContain('flowEventsHistorySql');
    expect(flow).not.toContain('FROM flow_events WHERE company_id=?');
    expect(flow).not.toMatch(/SELECT [^`]*FROM flow_events/);
    expect(flow).toContain("'wf_native_flow_events_history'");
    expect(flow).toContain("'wf_native_flow_events_record_history'");
  });

  it('keeps both retired reads read-only (no writes or row locks)', () => {
    for (const sql of [SNAPSHOT_ORACLE, RECORD_ORACLE]) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements is 25 after the r40 native flow-event cut (26->25)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(53);
    // r46 moved the approval-step/expense-batch reads onto native kinds (43->41);
    // r47 moved calendar-index-facts and record-details history onto native kinds (41->37);
    // r48 deduped the approver union, flow version guard, and settlement sources (37->33);
    // r49 deduped the export-receipt result read onto the closed command-receipt projection (33->32).
    // r53 folded the approver-union bare head onto selectHeadSql (29->28).
    // r40 moved the flow-event history generator onto native read kinds (26->25).
    expect(total).toBe(25);
  });
});
