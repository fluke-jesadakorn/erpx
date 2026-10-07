import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkActionCard, WorkCardPreferences } from "../../src/lib/work-cards";

// Drive the real provider's state and request callbacks without a DOM or server.
const hooks = vi.hoisted(() => ({
  slots: [] as { value: unknown }[],
  cursor: 0,
  context: null as unknown,
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
  useEffect() {},
}));
vi.mock("next/navigation", () => ({
  usePathname: () => hooks.pathname,
  useSearchParams: () => new URLSearchParams(),
}));
vi.mock("../../src/lib/work-fetch", () => ({ workFetch: vi.fn() }));
vi.mock("../../src/components/quick-access/shelf", () => ({ QuickAccessShelf: () => null }));

import { workFetch } from "../../src/lib/work-fetch";
import { NavigationCountsProvider, useNavigationCounts } from "../../src/components/workplace/navigation-counts";
import { WorkShortcuts } from "../../src/components/workplace/work-shortcuts";

type Navigation = ReturnType<typeof useNavigationCounts>;
type Shelf = {
  loading: boolean;
  error: string;
  initialPreferences: WorkCardPreferences;
  onRetry: () => void;
};
const cards: WorkActionCard[] = ["expenses", "services"].map((id) => ({
  id: id as "expenses" | "services", title: id, icon: id as "expenses" | "services",
  href: `/work?type=${id}`, create: null, toDo: null, waiting: null, blocked: 0, incomplete: false,
}));
const fetchMock = vi.mocked(workFetch);
function render(base = "/holdings/acme") {
  hooks.cursor = 0;
  const provider = NavigationCountsProvider({ base, initialShortcutCards: cards, shortcutStorageKey: base, children: null });
  const navigation = (provider as ReactElement<{ value: Navigation }>).props.value;
  hooks.context = navigation;
  const shelf = (WorkShortcuts({ base }) as ReactElement<Shelf>).props;
  return { navigation, shelf };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
function projection(preferences: WorkCardPreferences = { pinnedIds: ["expenses", "services"], version: 1 }, unavailable = false) {
  return { data: { home: 0, work: 0, quickAccess: { cards, preferences, preferencesUnavailable: unavailable } } };
}
async function load(preferences?: WorkCardPreferences, unavailable = false) {
  fetchMock.mockResolvedValueOnce(Response.json(projection(preferences, unavailable)));
  await render().navigation.refresh();
  return render();
}
beforeEach(() => {
  hooks.slots = [];
  hooks.cursor = 0;
  hooks.context = null;
  hooks.pathname = "/holdings/acme/work";
  fetchMock.mockReset();
});

describe("Work shortcut preference readiness", () => {
  it("keeps Customize gated until the delayed owner preference snapshot has been parsed", async () => {
    const response = deferred<Response>(), body = deferred<unknown>();
    fetchMock.mockReturnValueOnce(response.promise);
    const pending = render().navigation.refresh();
    expect(render().shelf.loading).toBe(true);
    const result = Response.json({});
    vi.spyOn(result, "json").mockReturnValueOnce(body.promise);
    response.resolve(result);
    await Promise.resolve();
    expect(render().shelf.loading).toBe(true);
    body.resolve(projection());
    await pending;
    expect(render().shelf).toMatchObject({ loading: false, error: "", initialPreferences: { pinnedIds: ["expenses", "services"], version: 1 } });
  });

  it("ends initial loading on failure and lets the real Retry callback recover saved pins", async () => {
    fetchMock.mockRejectedValueOnce(Error("network unavailable"));
    await render().navigation.refresh();
    const failed = render().shelf;
    expect(failed).toMatchObject({ loading: false, error: "Shortcuts could not refresh." });
    const retry = deferred<Response>();
    fetchMock.mockReturnValueOnce(retry.promise);
    failed.onRetry();
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/holdings/acme/navigation", expect.objectContaining({ cache: "no-store" }));
    retry.resolve(Response.json(projection()));
    await vi.waitFor(() => expect(render().shelf).toMatchObject({ loading: false, error: "", initialPreferences: { pinnedIds: ["expenses", "services"], version: 1 } }));
  });

  it("shows an unavailable owner preference read with Retry instead of an endless loading shelf", async () => {
    const unavailable = await load({ pinnedIds: null, version: 0 }, true);
    expect(unavailable.shelf).toMatchObject({ loading: false, error: "Your saved shortcuts could not refresh." });
    expect((await load()).shelf).toMatchObject({ loading: false, error: "", initialPreferences: { version: 1 } });
  });

  it("retains the last saved pins while a later refresh failure disables editing with its error", async () => {
    await load();
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 503 }));
    await render().navigation.refresh();
    expect(render().shelf).toMatchObject({ loading: false, error: "Shortcuts could not refresh.", initialPreferences: { pinnedIds: ["expenses", "services"], version: 1 } });
  });

  it("retains newer saved pins without hiding a later unavailable owner preference read", async () => {
    await load({ pinnedIds: ["services"], version: 2 });
    const unavailable = await load({ pinnedIds: null, version: 0 }, true);
    expect(unavailable.shelf).toMatchObject({ loading: false, error: "Your saved shortcuts could not refresh.", initialPreferences: { pinnedIds: ["services"], version: 2 } });
  });

  it("accepts a successful owner save and never regresses it to an older navigation response", async () => {
    await load();
    fetchMock.mockRejectedValueOnce(Error("network unavailable"));
    await render().navigation.refresh();
    render().navigation.saveQuickAccess({ pinnedIds: ["services"], version: 2 });
    expect(render().shelf).toMatchObject({ loading: false, error: "", initialPreferences: { pinnedIds: ["services"], version: 2 } });
    const response = deferred<Response>();
    fetchMock.mockReturnValueOnce(response.promise);
    const pending = render().navigation.refresh();
    render().navigation.saveQuickAccess({ pinnedIds: ["expenses"], version: 3 });
    response.resolve(Response.json(projection()));
    await pending;
    expect(render().shelf.initialPreferences).toEqual({ pinnedIds: ["expenses"], version: 3 });
  });

  it("does not let an aborted older read unlock Customize before the current read succeeds", async () => {
    const first = deferred<Response>(), current = deferred<Response>();
    fetchMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(current.promise);
    const previous = render().navigation.refresh(), latest = render().navigation.refresh();
    expect(fetchMock.mock.calls[0][1]?.signal?.aborted).toBe(true);
    first.resolve(Response.json(projection({ pinnedIds: ["leave"], version: 10 })));
    await previous;
    expect(render().shelf.loading).toBe(true);
    current.resolve(Response.json(projection()));
    await latest;
    expect(render().shelf).toMatchObject({ loading: false, initialPreferences: { pinnedIds: ["expenses", "services"], version: 1 } });
  });

  it("starts unready again when the authority scope remounts the provider", async () => {
    await load({ pinnedIds: ["services"], version: 7 });
    hooks.slots = [];
    hooks.pathname = "/holdings/other/work";
    const fresh = render("/holdings/other");
    expect(fresh.shelf).toMatchObject({ loading: true, error: "", initialPreferences: { pinnedIds: null, version: 0 } });
    fetchMock.mockResolvedValueOnce(Response.json(projection({ pinnedIds: ["leave"], version: 1 })));
    await fresh.navigation.refresh();
    expect(fetchMock).toHaveBeenLastCalledWith("/api/v1/holdings/other/navigation", expect.anything());
    expect(render("/holdings/other").shelf).toMatchObject({ loading: false, initialPreferences: { pinnedIds: ["leave"], version: 1 } });
  });
});
