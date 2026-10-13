import { createElement, type ComponentProps } from 'react';
import { renderToReadableStream } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import { SelfServiceHome } from "../../../src/components/people_payroll/self-service/home";
import type { SelfServiceSeed } from "../../../src/server/modules/records/public/self-service";
vi.mock('next/link', () => ({ default: 'a' }));
it('streams the feed while the day summary loads, then renders the supplied schedule', async () => {
  let resolve!: (value: SelfServiceSeed) => void;
  const initial = new Promise<SelfServiceSeed>(done => { resolve = done; });
  const stream = await renderToReadableStream(createElement(SelfServiceHome, {
    base: '/holdings/acme', enabled: true, initial,
  } as ComponentProps<typeof SelfServiceHome>, createElement('p', null, 'Feed is ready')));
  const reader = stream.getReader(), decoder = new TextDecoder();
  let first = '';
  while (!first.includes('Feed is ready')) { const next = await reader.read(); expect(next.done).toBe(false); first += decoder.decode(next.value); }
  expect(first).toContain('Feed is ready'); expect(first).toContain('Loading your day');
  resolve({ workCount: 0, data: { asOf: Date.now(), companyId: 'test', rosterRequired: true, actions: [], setupBlockers: [], unavailableSources: [], nextShift: null, attention: [], recentRequests: [], leave: null } } as unknown as SelfServiceSeed);
  let rest = '';
  for (;;) { const next = await reader.read(); if (next.done) break; rest += decoder.decode(next.value); }
  expect(rest).toContain('No upcoming published shift.');
  expect(rest).toContain('Plan my day');
});
