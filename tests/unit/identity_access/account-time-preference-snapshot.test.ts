import { beforeEach, expect, it, vi } from "vitest";
vi.mock("server-only", () => ({}));
const mocks = vi.hoisted(() => ({ session: vi.fn(), read: vi.fn() }));
vi.mock("@/server/modules/identity_access/adapters/authorization", () => ({ currentSession: mocks.session }));
vi.mock("@/server/modules/runtime/public/runtime", () => ({ runtimeEnv: async () => ({ CONTROL_DB: {
  prepare: () => ({ bind: (id: string) => ({ first: () => mocks.read(id) }) }),
} }) }));
import { accountTimePreferenceSnapshot } from "@/server/modules/identity_access/public/account-time-preference";

beforeEach(() => { mocks.session.mockReset(); mocks.read.mockReset(); });
it("seeds only the current authenticated account's preference without serializing identity or session facts", async () => {
  mocks.session.mockResolvedValue({ session: { user: { id: "account-one", email: "private@example.test" }, session: { id: "private-session", token: "private-token" } } });
  mocks.read.mockResolvedValue({ mode: "manual", timeZone: "Asia/Bangkok", hourCycle: "h12", version: 4 });
  const snapshot = await accountTimePreferenceSnapshot();
  expect(mocks.read).toHaveBeenCalledExactlyOnceWith("account-one");
  expect(snapshot).toEqual({ key: expect.stringMatching(/^[a-f0-9]{64}$/), preference: { mode: "manual", timeZone: "Asia/Bangkok", hourCycle: "h12", version: 4 } });
  expect(JSON.stringify(snapshot)).not.toMatch(/account-one|private@example|private-session|private-token/);
});
it("uses an anonymous default without querying account preferences", async () => {
  mocks.session.mockRejectedValue(new Error("UNAUTHENTICATED"));
  expect(await accountTimePreferenceSnapshot()).toEqual({ key: "anonymous", preference: { mode: "system", timeZone: null, hourCycle: "h23", version: 0 } });
  expect(mocks.read).not.toHaveBeenCalled();
});
it("does not seed a revoked or unavailable account", async () => {
  mocks.session.mockRejectedValue(Object.assign(new Error("Revoked"), { status: 401 }));
  expect(await accountTimePreferenceSnapshot()).toBeNull();
  expect(mocks.read).not.toHaveBeenCalled();
});
it("isolates preference state on account switching, even when the previous account had a newer preference version", async () => {
  mocks.session.mockResolvedValueOnce({ session: { user: { id: "account-one" } } })
    .mockResolvedValueOnce({ session: { user: { id: "account-two" } } });
  mocks.read.mockResolvedValueOnce({ mode: "manual", timeZone: "UTC", hourCycle: "h12", version: 10 }).mockResolvedValueOnce(null);
  const first = await accountTimePreferenceSnapshot(), second = await accountTimePreferenceSnapshot();
  expect(first!.key).not.toBe(second!.key);
  expect(second!.preference).toEqual({ mode: "system", timeZone: null, hourCycle: "h23", version: 0 });
});
it("retains the authenticated account identity during a preference outage without seeding a false default", async () => {
  mocks.session.mockResolvedValue({ session: { user: { id: "account-one" } } });
  mocks.read.mockResolvedValueOnce({ mode: "manual", timeZone: "UTC", hourCycle: "h12", version: 10 }).mockRejectedValueOnce(new Error("Database unavailable"));
  const first = await accountTimePreferenceSnapshot(), unavailable = await accountTimePreferenceSnapshot();
  expect(unavailable).toEqual({ key: first!.key, preference: undefined });
});

import {createElement,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {TimeText} from '../../../src/components/people_payroll/time/time-preference-context';
const timeContextProbe=vi.hoisted(()=>({wrap:null as null|((ready:boolean,children:ReactNode)=>ReactNode)}));
vi.mock('react',async importOriginal=>{
 const actual=await importOriginal<typeof import('react')>();
 return {...actual,createContext<T>(initial:T){
  const context=actual.createContext(initial);
  if(initial&&typeof initial==='object'&&'systemZone' in initial&&'ready' in initial&&'setPreference' in initial){
   // Observe the actual private context while delegating every React operation.
   // Its already-mounted readiness is an explicit parent-state port; TimeText's
   // own useSyncExternalStore runs with the real server renderer/dispatcher.
   timeContextProbe.wrap=(ready,children)=>actual.createElement(context.Provider,{value:{...initial,ready},children});
  }
  return context;
 }};
});
function timeTextServerSnapshot(ready:boolean,value:string,appearance:'legacy'|'standard'='standard'){
 if(!timeContextProbe.wrap)throw Error('Actual TimeContext was not observed.');
 return renderToStaticMarkup(timeContextProbe.wrap(ready,createElement(TimeText,{value,appearance,className:'retained-time-widget'})));
}
it.each(['legacy','standard'] as const)('TimeText keeps its own first server snapshot stable under an already-ready parent (%s)',appearance=>{
 const value='2026-09-27T03:56:00Z',original=timeTextServerSnapshot(false,value,appearance),underReadyParent=timeTextServerSnapshot(true,value,appearance);
 expect(original).toContain('>…</time>');expect(underReadyParent).toBe(original);
 expect(underReadyParent).toContain('dateTime="2026-09-27T03:56:00.000Z"');expect(underReadyParent).toContain('class="retained-time-widget"');expect(underReadyParent).toContain(`data-ui-appearance="${appearance}"`);
});
it('TimeText retains its invalid-date placeholder without emitting an invalid dateTime attribute',()=>{
 const html=timeTextServerSnapshot(true,'not-a-timestamp');expect(html).toContain('>…</time>');expect(html).not.toContain('dateTime=');
});
