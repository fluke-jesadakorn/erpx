import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { scheduleWorkRefresh } from "@erpx/feature-work_projection/lib/work-refresh";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
let page: EventTarget & { visibilityState: string; querySelector: () => typeof context };
let view: EventTarget;
let context: { isConnected: boolean; dataset: { workRevision: string; workCompany: string; workAssignment: string; workStale?: string } };
beforeEach(() => {
  context = { isConnected: true, dataset: { workRevision: '1', workCompany: 'a', workAssignment: 'grant' } };
  page = Object.assign(new EventTarget(), { visibilityState: 'visible', querySelector: () => context });
  view = Object.assign(new EventTarget(), { location: { origin: 'https://erpx.test' } });
  vi.stubGlobal('window', view); vi.stubGlobal('document', page);
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
it('coalesces focus/visibility/write bursts and removes pending refresh on unmount', () => {
  vi.useFakeTimers(); const refresh = vi.fn(), stop = scheduleWorkRefresh(refresh);
  view.dispatchEvent(new Event('focus')); page.dispatchEvent(new Event('visibilitychange')); view.dispatchEvent(new Event('erpx:work-changed'));
  vi.advanceTimersByTime(80); expect(refresh).toHaveBeenCalledTimes(1);
  view.dispatchEvent(new Event('focus')); stop(); vi.advanceTimersByTime(80); expect(refresh).toHaveBeenCalledTimes(1);
});
it('rejects an old response when the rendered company/role changes, including DOM reuse', async () => {
  let finish!: (response: Response) => void;
  vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
  const pending = workFetch('/api/v1/holdings/acme/work/summary');
  context.dataset.workRevision = '2'; context.dataset.workCompany = 'b';
  finish(Response.json({ data: { secret: 'old scope' } }));
  const response = await pending; expect(response.status).toBe(409);
  expect(JSON.stringify(await response.json())).not.toContain('old scope');
});
it('announces successful writes immediately but not reads or unsuccessful writes', async () => {
  const changed = vi.fn(); view.addEventListener('erpx:work-changed', changed);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ data: {} })));
  await workFetch('/api/v1/holdings/acme/staff-tasks/one', { method: 'POST' }); expect(changed).toHaveBeenCalledTimes(1);
  await workFetch('/api/v1/holdings/acme/work/summary'); expect(changed).toHaveBeenCalledTimes(1);
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ error: {} }, { status: 403 })));
  await workFetch('/api/v1/holdings/acme/staff-tasks/one', { method: 'POST' }); expect(changed).toHaveBeenCalledTimes(1);
});
