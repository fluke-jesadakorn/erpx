import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Drive the real provider's state and request callbacks without a DOM or server.
const hooks = vi.hoisted(() => ({
  slots: [] as { value: unknown }[],
  cursor: 0,
  context: null as unknown,
  effects: [] as (() => void | (() => void))[],
  pathname: "/holdings/acme/work",
}));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState<T>(initial: T | (() => T)) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: typeof initial === "function" ? (initial as () => T)() : initial };
    return [hooks.slots[index].value as T, (update: T | ((value: T) => T)) => {
      hooks.slots[index].value = typeof update === "function"
        ? (update as (value: T) => T)(hooks.slots[index].value as T) : update;
    }];
  },
  useRef<T>(initial: T) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: { current: initial } };
    return hooks.slots[index].value;
  },
  useCallback<T>(callback: T) { return callback; },
  useContext() { return hooks.context; },
  useEffect(effect: () => void | (() => void)) { hooks.effects.push(effect); },
}));
vi.mock("next/navigation", () => ({
  usePathname: () => hooks.pathname,
  useSearchParams: () => new URLSearchParams(),
}));
vi.mock("../../../src/lib/work_projection/work-fetch", () => ({ workFetch: vi.fn() }));
vi.mock("../../../src/lib/work_projection/work-refresh", () => ({scheduleWorkRefresh: () => () => {}}));
vi.mock("../../../src/lib/work_projection/context-switch", () => ({contextIsSwitching: () => false}));
import { workFetch } from "../../../src/lib/work_projection/work-fetch";
import { NavigationCountsProvider, useNavigationCounts } from "../../../src/components/workplace/navigation-counts";

type Navigation = ReturnType<typeof useNavigationCounts>;

const fetchMock = vi.mocked(workFetch);
function render() {
  hooks.cursor = 0;
  const provider = NavigationCountsProvider({ base: "/holdings/acme", shortcutStorageKey: "acme", children: null });
  return (provider as ReactElement<{ value: Navigation }>).props.value;
}
beforeEach(() => {
  hooks.slots = [];
  hooks.effects = [];
  hooks.cursor = 0;
  fetchMock.mockReset();
});

describe("Navigation counts without shortcuts", () => {
  it("loads counts without requiring a shortcut projection", async () => {
    fetchMock.mockResolvedValueOnce(Response.json({ data: { home: 2, work: 3, workViewAvailability: { team: true, routing: false } } }));
    await render().refresh();
    expect(render().counts).toEqual({ home: 2, work: 3 });
    expect(render().workViewAvailability).toEqual({ team: true, routing: false });
  });

  it("preserves a published work count when an older refresh resolves", async () => {
    let resolve!: (response: Response) => void;
    fetchMock.mockReturnValueOnce(new Promise<Response>(done => { resolve = done; }));
    const pending = render().refresh();
    render().publishWork(5);
    resolve(Response.json({ data: { home: 2, work: 3 } }));
    await pending;
    expect(render().counts).toEqual({ home: 2, work: 5 });
  });

  it("retains the published count after a failed refresh", async () => {
    render().publishWork(5);
    fetchMock.mockRejectedValueOnce(Error("network unavailable"));
    await render().refresh();
    expect(render().counts).toEqual({ home: null, work: 5 });
    expect(render().error).toBe(true);
  });
});


const attentionData = {home:2,work:7,attention:{items:[],total:7,waiting:4,incomplete:false},snapshotAt:100};
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

it("coalesces simultaneous Home/menu refreshes into one attention request", async () => {
  let resolve!: (response:Response) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>(done => {resolve=done;}));
  const navigation = render();
  const pending = Promise.all([navigation.refresh(),navigation.refresh(),navigation.refresh()]);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(fetchMock.mock.calls[0][0]).toBe("/api/v1/holdings/acme/navigation?include=attention");
  resolve(Response.json({data:attentionData})); await pending;
  expect(render().attention).toEqual(attentionData.attention);
  expect(render().counts.work).toBe(7);
});

it("retains attention and its timestamp during refresh and failure, then recovers on retry", async () => {
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData}));
  await render().refresh();
  let reject!: (error:Error) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>((_,fail) => {reject=fail;}));
  const pending = render().refresh();
  expect(render().updating).toBe(true);
  expect(render().attention).toEqual(attentionData.attention);
  reject(Error("offline")); await pending;
  expect(render().snapshotAt).toBe(100);
  expect(render().attention.total).toBe(7);
  expect(render().error).toBe(true);
  fetchMock.mockResolvedValueOnce(Response.json({data:{...attentionData,work:8,snapshotAt:200}}));
  await render().refresh();
  expect(render().error).toBe(false);
  expect(render().counts.work).toBe(8);
});

it.each([401,403,404,409])("clears private attention after access response %s", async status => {
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData})); await render().refresh();
  fetchMock.mockResolvedValueOnce(Response.json({error:{}},{status})); await render().refresh();
  expect(render().counts).toEqual({home:null,work:null});
  expect(render().attention.items).toEqual([]);
  expect(render().snapshotAt).toBe(0);
});

it.each(["erpx:context-switch-started","erpx:work-context-changed"])("discards responses immediately after %s", async event => {
  vi.useFakeTimers(); vi.stubGlobal("window",new EventTarget());
  const navigation = render(); const cleanup = hooks.effects[0]();
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData})); await navigation.refresh();
  let resolve!: (response:Response) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>(done => {resolve=done;}));
  const pending = render().refresh();
  window.dispatchEvent(new Event(event));
  expect(render().counts.work).toBeNull();
  expect(render().snapshotAt).toBe(0);
  resolve(Response.json({data:attentionData})); await pending;
  render().publishWork(99);
  expect(render().counts.work).toBeNull();
  expect(render().attention.total).toBe(0);
  if (cleanup) cleanup();
});
