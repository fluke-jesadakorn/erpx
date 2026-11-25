import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: Array<{ company: string; input: Record<string, unknown> }> = [];
const statement = { tag: 'native-statement' };
vi.mock('../../../packages/server-collaboration/src/adapters/native-system-notification', () => ({
  nativeSystemNotificationStatement: vi.fn(async (
    _db: unknown,
    company: string,
    input: Record<string, unknown>,
  ) => {
    calls.push({ company, input });
    return statement;
  }),
}));
import { nativeInboxStatements } from '../../../packages/server-collaboration/src/adapters/native-inbox-statements';

const src = (f: string) => readFileSync(resolve(process.cwd(), 'packages/server-collaboration/src/adapters', f), 'utf8');
const db = {} as never;

const BY_KEY = 'SELECT id FROM inbox_events WHERE company_id=? AND event_key=?';
const NOTICE_EXISTS = 'SELECT id FROM inbox_notices WHERE id=? AND company_id=?';
const RECENT_COUNT = 'SELECT count(*)::int n FROM inbox_notices WHERE company_id=? AND sender_user_id=? AND created_at>?';

beforeEach(() => { calls.length = 0; });

describe('native inbox read family', () => {
  it('sends the three registered read kinds with the retired ordered binds', async () => {
    const reads = nativeInboxStatements(db, 'company-1');
    expect(await reads.eventByKey('company-1', 'key-1')).toBe(statement);
    expect(await reads.noticeExists('company-1', 'notice-1')).toBe(statement);
    expect(await reads.recentNoticeCount('company-1', 'user-1', 42)).toBe(statement);
    expect(calls.map(c => c.company)).toEqual(['company-1', 'company-1', 'company-1']);
    expect(calls.map(c => c.input.kind)).toEqual([
      'inbox_event_by_key', 'inbox_notice_exists', 'inbox_recent_notice_count',
    ]);
    expect(calls.map(c => JSON.parse(c.input.values_json as string))).toEqual([
      ['company-1', 'key-1'],
      ['notice-1', 'company-1'],
      ['company-1', 'user-1', 42],
    ]);
    for (const c of calls) {
      expect(c.input.userId).toBe('');
      expect(c.input.recordId).toBe('');
      expect(c.input.recordVersion).toBe(0);
      expect(c.input.message).toBe('');
    }
  });

  it('deletes the replaced TS SQL and keeps the owner statement bridge as the only reader', () => {
    const inbox = src('inbox.ts');
    const bridge = src('native-inbox-statements.ts');
    for (const sql of [BY_KEY, NOTICE_EXISTS, RECENT_COUNT]) expect(inbox).not.toContain(sql);
    expect(inbox).not.toContain('inboxEventByKeySql');
    expect(inbox).toContain('nativeInboxStatements(db, companyId).eventByKey(companyId, key)');
    expect(inbox).toContain('nativeInboxStatements(db, company_id).eventByKey(company_id, value)');
    expect(inbox).toContain('nativeInboxStatements(db, companyId).noticeExists(companyId, source_id)');
    expect(inbox).toContain('nativeInboxStatements(db, companyId).recentNoticeCount(companyId, userId, value)');
    for (const kind of ['inbox_event_by_key', 'inbox_notice_exists', 'inbox_recent_notice_count'])
      expect(bridge).toContain(`'${kind}'`);
    expect(bridge).not.toMatch(/SELECT /);
  });

  it('leaves the page/summary, lock and operational-metric SQL untouched', () => {
    const inbox = src('inbox.ts');
    expect(inbox).toContain('WITH native_rows AS (SELECT * FROM jsonb_to_recordset');
    expect(inbox).toContain('SELECT e.source_type,CASE WHEN e.source_type IN');
    expect(inbox).toContain('SELECT notice_id,company_id,user_id FROM inbox_notice_targets');
    expect(inbox).toContain('SELECT count(*)::int count,min(n.created_at) oldest');
    expect(inbox).toContain('SELECT count(*)::int count FROM inbox_events WHERE source_type NOT IN');
  });
});

describe('retirement:ts-statements', () => {
  it('drops 319 -> 316 for the three Company-scoped inbox reads', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(process.cwd(), 'packages/server-collaboration/src'));
    expect(total).toBe(316);
    expect(total).toBeLessThan(319);
  });
});
