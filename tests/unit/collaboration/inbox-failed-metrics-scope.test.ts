import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { createInboxApplication, type InboxFailedScope } from '../../../packages/server-collaboration/src/application/inbox';

const scope: InboxFailedScope = {
  holdingId: 'h002',
  holdingSlug: 'northstar',
  companyId: 'company-2',
};

function repository(calls: unknown[]) {
  const pending = { count: 0, oldest: null };
  return new Proxy({} as Record<string, unknown>, {
    get(_target, key: string) {
      if (key === 'failedMetrics')
        return async (value?: InboxFailedScope) => {
          calls.push(value);
          return { count: 0 };
        };
      if (key === 'pendingMetrics') return async () => pending;
      if (key === 'unresolvedMetrics') return async () => ({ count: 0 });
      return async () => undefined;
    },
  });
}

describe('inbox failed-metrics WorkContext threading', () => {
  it('passes the caller scope to failedMetrics and stays closed without one', async () => {
    const calls: unknown[] = [];
    const app = createInboxApplication({
      repository: repository(calls) as never,
      commands: { replay: vi.fn(), commit: vi.fn() },
      clock: { now: () => 0 },
      identifiers: { next: () => 'id' },
      visibleNotifications: vi.fn(),
      candidates: vi.fn(),
    } as never);
    await app.inboxOperationalMetrics(0, scope);
    expect(calls).toEqual([scope]);
    await app.inboxOperationalMetrics(0);
    expect(calls).toEqual([scope, undefined]);
  });

  it('threads the scope from the adapter into owner3 without a first-Holding default', () => {
    const adapter = readFileSync(
      'packages/server-collaboration/src/adapters/inbox.ts', 'utf8',
    );
    expect(adapter).toContain('function application(db: SqlDatabase, failedScope?: InboxFailedScope)');
    expect(adapter).toContain('...owner3(db, failedScope');
    expect(adapter).toContain('? { holdingId: failedScope.holdingId, holdingSlug: failedScope.holdingSlug }');
    expect(adapter).toContain(': undefined)');
    expect(adapter).toContain('export function inboxOperationalMetrics(db: SqlDatabase, failedScope?: InboxFailedScope, now = Date.now())');
    expect(adapter).toContain('application(db, failedScope).inboxOperationalMetrics(now, failedScope)');
    expect(adapter).not.toContain('owner3(db)');
  });
});
