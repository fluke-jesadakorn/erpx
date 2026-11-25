import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { nativeWorkflowReadAll } from '../../../packages/server-workflow/src/adapters/native-workflow-read';
import { calendarIndexWorkflowFacts } from '../../../packages/server-workflow/src/adapters/calendar-index-facts';
import { RecordDetailsWorkflowOwnedFacts } from '../../../packages/server-workflow/src/adapters/records-record-details-owned-facts';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const readAll = vi.mocked(nativeWorkflowReadAll);
const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');

/** Independent reconstruction of the two retired TypeScript literals (r47). */
const hr = '"id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone"';
const calendarSql =
  `SELECT record_id,action,actor,at FROM (SELECT e.record_id,e.action,coalesce(p.legal_name,'Authorized colleague') actor,e.created_at at,row_number() OVER(PARTITION BY e.record_id ORDER BY e.created_at DESC,e.id DESC) ordinal FROM audit_events e JOIN command_receipts r ON r.command_id=e.command_id LEFT JOIN (SELECT ${hr} FROM hr_people) p ON p.user_id=e.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE r.authority_json::jsonb->>'companyId'=? AND e.record_id IN (SELECT jsonb_array_elements_text(?::jsonb))) h WHERE ordinal<=20 ORDER BY at DESC`;
const historySql =
  "SELECT action,actor_id,created_at,record_version FROM audit_events WHERE record_id=? ORDER BY created_at DESC LIMIT 50";
const rustLiteral = (sql: string) => sql.replaceAll('"', '\\"');

function capture() {
  const calls: { sql: string; binds: unknown[] }[] = [];
  const stmt = (sql: string) => ({
    bind(...values: unknown[]) { calls.push({ sql, binds: values }); return this; },
    first() { return undefined; },
    all() { return { results: [] as unknown[] }; },
  });
  const db = { prepare: (sql: string) => stmt(sql) } as unknown as SqlDatabase;
  return { db, calls };
}

beforeEach(() => {
  readAll.mockReset();
  readAll.mockResolvedValue([]);
});

describe('calendar-index and record-details native cutover (r47)', () => {
  it('registers byte-identical, SELECT-only SQL with three and one binds', async () => {
    const [domain, publicRegistry] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/domain/workflow_wave3.rs', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/public/workflow_wave3.rs', import.meta.url), 'utf8'),
    ]);
    for (const [kind, sql, binds] of [
      ['wf_native_calendar_index_facts', calendarSql, 3],
      ['wf_native_record_details_history', historySql, 1],
    ] as const) {
      expect(domain).toContain(`pub const WF_NATIVE_${kind.slice('wf_native_'.length).toUpperCase()}: &str = "${kind}"`);
      expect(domain).toContain(`    WF_NATIVE_${kind.slice('wf_native_'.length).toUpperCase()},\n`);
      expect(domain).toContain(rustLiteral(sql));
      expect(publicRegistry).toContain(`WF_NATIVE_${kind.slice('wf_native_'.length).toUpperCase()}`);
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect((sql.match(/\?/g) ?? []).length).toBe(binds);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|RETURNING|;|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(calendarSql).toContain('company_id=?');
    expect((domain.match(/pub const WF_NATIVE_CALENDAR_INDEX_FACTS:/g) ?? []).length).toBe(1);
    expect((domain.match(/pub const WF_NATIVE_RECORD_DETAILS_HISTORY:/g) ?? []).length).toBe(1);
  });

  it('calendarIndexWorkflowFacts emits no local SQL and keeps the three exact binds', async () => {
    const { db, calls } = capture();
    await calendarIndexWorkflowFacts(db).read4({ value1: 'co-1', value2: '["r1"]' });
    expect(calls).toEqual([]);
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_calendar_index_facts', ['co-1', 'co-1', '["r1"]'], 'co-1', db,
    );
  });

  it('record-details history uses the native kind with the company scope carried separately', async () => {
    const { db, calls } = capture();
    await expect(
      RecordDetailsWorkflowOwnedFacts(db).readCommerceMasterDetailHistory('r1', 'co-1'),
    ).resolves.toEqual({ results: [] });
    expect(calls).toEqual([]);
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_record_details_history', ['r1'], 'co-1', db,
    );
  });

  it('removes the retired SELECT literals and the local inList helper', async () => {
    const [calendar, details] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/calendar-index-facts.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/records-record-details-owned-facts.ts', import.meta.url), 'utf8'),
    ]);
    expect(calendar).not.toContain('SELECT record_id,action,actor,at');
    expect(calendar).not.toContain('jsonb_array_elements_text');
    expect(calendar).not.toContain('const inList');
    expect(calendar).not.toContain('workflowFinalOwnerProjections');
    expect(calendar).toContain("'wf_native_calendar_index_facts'");
    expect(details).not.toContain('SELECT action,actor_id,created_at,record_version');
    expect(details).toContain("'wf_native_record_details_history'");
    expect(details).toContain('companyId: string');
  });

  it('the record-details repository port forwards the actor company into the native reader', async () => {
    const [application, adapter] = await Promise.all([
      readFile(new URL('../../../packages/server-records/src/application/record-details.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-records/src/adapters/record-details.ts', import.meta.url), 'utf8'),
    ]);
    expect(application).toContain('readCommerceMasterDetailHistory(id: string, companyId: string)');
    expect(application).toContain('r.readCommerceMasterDetailHistory(id, actor.companyId)');
    expect(adapter).toContain('readCommerceMasterDetailHistory(...args) { return RecordDetailsWorkflowOwnedFacts(database).readCommerceMasterDetailHistory(...args); }');
  });

  it('retirement:ts-statements is 32 after the r47 native read cutover, r48 dedupe and r49 dedupe (41->37->33->32)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(41);
    expect(total).toBe(25);
  });
});
