import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');

/** Byte oracles captured from the retired TypeScript generator before the r40 cut. */
const SNAPSHOT_ORACLE = 'SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? ORDER BY created_at,id';
const RECORD_ORACLE = 'SELECT action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? AND kind=? AND record_id=? ORDER BY created_at,id';

describe('workflow flow-event history read tail (r40 native read cutover)', () => {
  it('keeps both retired literals byte-identical in the owner Rust read registry', async () => {
    const rust = await readFile(resolve(srcRoot, 'domain/workflow_wave3.rs'), 'utf8');
    expect(rust).toContain(`"${SNAPSHOT_ORACLE}"`);
    expect(rust).toContain(`"${RECORD_ORACLE}"`);
    expect(rust).toContain('pub const WF_NATIVE_FLOW_EVENTS_HISTORY: &str = "wf_native_flow_events_history";');
    expect(rust).toContain('pub const WF_NATIVE_FLOW_EVENTS_RECORD_HISTORY: &str = "wf_native_flow_events_record_history";');
    expect((SNAPSHOT_ORACLE.match(/\?/g) ?? []).length).toBe(1);
    expect((RECORD_ORACLE.match(/\?/g) ?? []).length).toBe(3);
  });

  it('plans the company snapshot through the native read with the exact company bind', async () => {
    const rows = [{ kind: 'expense', record_id: 'record-1' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: vi.fn(),
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../packages/server-workflow/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const result = await nativeFlowPlanningPorts(db).repository.readSharedFlowReferencesRoutesdefaultsevents3('company');
      expect(result).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_flow_events_history',
        ['company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../packages/server-workflow/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });

  it('plans the per-record history through the native read with the exact company, kind and record binds', async () => {
    const rows = [{ action: 'submit', actor_name: 'Actor', created_at: 1, reason: '', step_key: 'draft' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: vi.fn(),
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../packages/server-workflow/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const result = await nativeFlowPlanningPorts(db).repository.readReadFlowHistoryRows('company', 'expense', 'record-2');
      expect(result).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_flow_events_record_history',
        ['company', 'expense', 'record-2'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../packages/server-workflow/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});
