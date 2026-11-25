import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: Array<{ company: string; input: Record<string, unknown> }> = [];
const statement = { tag: 'native-statement' };
vi.mock('../../../packages/server-collaboration/src/adapters/native-system-notification', () => ({
  nativeSystemNotificationStatement: vi.fn(async (
    _db: { prepare(sql: string): { bind(): unknown } },
    company: string,
    input: Record<string, unknown>,
  ) => {
    calls.push({ company, input });
    const bound = _db.prepare(`PLANNER:${String(input.kind)}`).bind();
    void bound;
    return statement;
  }),
}));
import {
  nativeInboxStatements,
  nativeInboxSqlFragment,
  SYSTEM_COMPANY,
} from '../../../packages/server-collaboration/src/adapters/native-inbox-statements';

const root = resolve(process.cwd(), 'packages/server-collaboration/src');
const adapters = resolve(root, 'adapters');
const src = (f: string) => readFileSync(resolve(adapters, f), 'utf8');
const db = { prepare: (_sql: string) => ({ bind: () => statement }) } as never;

const SQL = {
  eventByKey: 'SELECT id FROM inbox_events WHERE company_id=? AND event_key=?',
  noticeExists: 'SELECT id FROM inbox_notices WHERE id=? AND company_id=?',
  recentCount: 'SELECT count(*)::int n FROM inbox_notices WHERE company_id=? AND sender_user_id=? AND created_at>?',
  pendingTargets: 'SELECT notice_id,company_id,user_id FROM inbox_notice_targets WHERE delivered_at IS NULL ORDER BY notice_id,user_id LIMIT 50',
  pendingMetrics: 'SELECT count(*)::int count,min(n.created_at) oldest\n    FROM inbox_notice_targets t JOIN inbox_notices n ON n.id=t.notice_id WHERE t.delivered_at IS NULL',
  candidate: 'SELECT d.*,e.source_type,e.source_id,e.source_version,e.category,e.preview FROM inbox_deliveries d\n    JOIN inbox_events e ON e.id=d.event_id WHERE d.id=? AND d.company_id=? AND d.user_id=?',
  nativeOwned: "SELECT e.source_type,CASE WHEN e.source_type IN('field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal') AND erpx_field_push_source(e.source_type,e.source_id,e.company_id) THEN 1 ELSE 0 END native_owned FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND d.user_id=?",
  nativeRows: 'WITH native_rows AS (SELECT * FROM jsonb_to_recordset(?::jsonb) AS n(id text,event_id text,company_id text,user_id text,delivered_at bigint,read_at bigint,archived_at bigint,source_type text,source_id text,source_version bigint,category text,preview text,native_field_item jsonb)),legacy_rows AS (',
  candidateSource: 'SELECT d.id,d.event_id,d.company_id,d.user_id,d.delivered_at,d.read_at,d.archived_at,e.source_type,e.source_id,e.source_version,e.category,e.preview,NULL::jsonb AS native_field_item\n  FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id\n  WHERE d.company_id=? AND d.user_id=? AND d.archived_at IS NULL',
  combinedOrder: 'SELECT * FROM combined ORDER BY delivered_at DESC,id DESC LIMIT 100',
};

beforeEach(() => { calls.length = 0; });

describe('native inbox read family', () => {
  it('sends every retired read kind with the exact ordered binds', async () => {
    const reads = nativeInboxStatements(db, 'company-1');
    expect(await reads.eventByKey('company-1', 'key-1')).toBe(statement);
    expect(await reads.noticeExists('company-1', 'notice-1')).toBe(statement);
    expect(await reads.recentNoticeCount('company-1', 'user-1', 42)).toBe(statement);
    expect(await reads.pendingTargets()).toBe(statement);
    expect(await reads.pendingMetrics()).toBe(statement);
    expect(await reads.unresolvedMetrics(['notice', 'task'])).toBe(statement);
    expect(await reads.candidate('company-1', 'delivery-1', 'user-1')).toBe(statement);
    expect(await reads.deliveryNativeOwned('company-1', 'delivery-1', 'user-1')).toBe(statement);
    expect(calls.map(c => c.input.kind)).toEqual([
      'inbox_event_by_key', 'inbox_notice_exists', 'inbox_recent_notice_count',
      'inbox_pending_targets', 'inbox_pending_metrics', 'inbox_unresolved_metrics',
      'inbox_candidate', 'inbox_delivery_native_owned',
    ]);
    expect(calls.map(c => JSON.parse(c.input.values_json as string))).toEqual([
      ['company-1', 'key-1'],
      ['notice-1', 'company-1'],
      ['company-1', 'user-1', 42],
      [],
      [],
      ['notice', 'task'],
      ['delivery-1', 'company-1', 'user-1'],
      ['delivery-1', 'company-1', 'user-1'],
    ]);
    expect(calls.map(c => c.company)).toEqual([
      'company-1', 'company-1', 'company-1',
      SYSTEM_COMPANY, SYSTEM_COMPANY, SYSTEM_COMPANY,
      'company-1', 'company-1',
    ]);
  });

  it('returns owner SQL text for the fixed page-projection fragments', async () => {
    for (const kind of ['inbox_native_rows_cte', 'inbox_candidate_source', 'inbox_combined_order']) {
      expect(await nativeInboxSqlFragment('company-1', kind, [])).toBe(`PLANNER:${kind}`);
    }
  });

  it('deletes every TS SELECT literal from inbox.ts', () => {
    const inbox = src('inbox.ts');
    expect(inbox.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(inbox).not.toContain('db.prepare("SELECT');
    expect(inbox).not.toContain('db.prepare(`SELECT');
    expect(inbox).toContain('nativeInboxStatements(db, companyId).candidate(companyId, id, userId)');
    expect(inbox).toContain('nativeInboxStatements(db, SYSTEM_COMPANY).pendingTargets()');
    expect(inbox).toContain('nativeInboxStatements(db, SYSTEM_COMPANY).pendingMetrics()');
    expect(inbox).toContain('nativeInboxStatements(db, SYSTEM_COMPANY).unresolvedMetrics(types)');
    expect(inbox).toContain('nativeInboxStatements(db,actor.companyId).deliveryNativeOwned');
    expect(inbox).toContain('nativeInboxSqlFragment(actor.companyId,\'inbox_native_rows_cte\',[native])');
    expect(inbox).toContain('nativeInboxSqlFragment(actor.companyId,\'inbox_candidate_source\')');
    expect(inbox).toContain('nativeInboxSqlFragment(actor.companyId,\'inbox_combined_order\')');
    expect(inbox).toContain('${nativeRows}${scope.clock}${candidateSource}');
    expect(inbox).toContain('${legacyNativeUnion} ${combinedOrder}');
  });

  it('pins the owner planner SQL byte for byte against the retired literals', () => {
    const planner = src('notification_statement.rs');
    for (const [kind, sql] of Object.entries(SQL)) {
      expect(planner).toContain(`"${kind === 'eventByKey' ? 'inbox_event_by_key' : kind === 'noticeExists' ? 'inbox_notice_exists' : kind === 'recentCount' ? 'inbox_recent_notice_count' : kind === 'pendingTargets' ? 'inbox_pending_targets' : kind === 'pendingMetrics' ? 'inbox_pending_metrics' : kind === 'candidate' ? 'inbox_candidate' : kind === 'nativeOwned' ? 'inbox_delivery_native_owned' : kind === 'nativeRows' ? 'inbox_native_rows_cte' : kind === 'candidateSource' ? 'inbox_candidate_source' : 'inbox_combined_order'}"`);
      expect(planner).toContain(sql.replace(/\n/g, '\\n'));
    }
    expect(planner).toContain('source_type NOT IN ({placeholders})');
  });

  it('leaves the dynamic owner predicates, binds and locks at the caller', () => {
    const inbox = src('inbox.ts');
    expect(inbox).toContain('AND NOT (e.source_type IN(');
    expect(inbox).toContain('.bind(JSON.stringify(native),actor.companyId, actor.userId,...scope.args');
    expect(inbox).toContain('ORDER BY d.delivered_at DESC,d.id DESC LIMIT 100), combined AS');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 316 -> 297 as the inbox adapter reaches zero SELECT tokens', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBe(257);
    expect(total).toBeLessThan(316);
  });
});
