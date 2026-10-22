import * as React from "react";
import { renderToReadableStream } from "react-dom/server.browser";
import { expect, it, vi } from "vitest";
import { NavigationCountsProvider, useNavigationCounts, useNavigationState } from "@erpx/workplace/components/navigation-counts";
import { NavigationBadge } from "@erpx/workplace/components/navigation-badge";
import type { NavigationSnapshot } from "@erpx/server-work_projection/public/navigation-snapshot";
import { workAttention } from "@erpx/feature-work_projection/lib/work-attention";
import { SocialHomeShell } from "@erpx/feature-collaboration/components/social-home-shell";

vi.mock("next/navigation", () => ({ usePathname: () => "/holdings/acme/home" }));
vi.mock("next/link", () => ({ default: ({ children, ...props }: React.ComponentProps<"a">) => React.createElement("a", props, children) }));
vi.mock("@erpx/feature-work_projection/components/home/home-visual-prototype", () => ({ HomeVisualPrototype: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("@erpx/feature-people_payroll/components/self-service/home", () => ({ MyDayCard: () => null }));
vi.mock("@erpx/feature-work_projection/components/today/start-actions", () => ({ StartActions: () => null }));
vi.mock("@erpx/feature-work_projection/components/today/daily-plan", () => ({ DailyPlan: () => null }));
vi.mock("@erpx/feature-people_payroll/components/time/time-preference-context", () => ({ TimeText: () => null }));
vi.stubGlobal("React", React);
// createElement's overload requires children in the props type even when they
// are supplied through its child arguments.
const Provider = NavigationCountsProvider as React.ComponentType<Omit<React.ComponentProps<typeof NavigationCountsProvider>, "children">>;
const HomeShell = SocialHomeShell as React.ComponentType<Omit<React.ComponentProps<typeof SocialHomeShell>, "children">>;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
const snapshot: NavigationSnapshot = { home: 0, work: 7, attention: workAttention([]), snapshotAt: 123,
  workViewAvailability: { team: true, routing: false } };
function MenuShell() {
  const { readHome } = useNavigationState();
  return React.createElement("button", { onClick: readHome }, "Usable menu");
}
function Priorities() {
  const { counts } = useNavigationCounts();
  return React.createElement("strong", null, `Priorities: ${counts.work}`);
}
function Feed({ data }: { data: Promise<string> }) {
  return React.createElement("article", null, React.use(data));
}
async function chunk(reader: ReadableStreamDefaultReader<Uint8Array>) {
  const result = await reader.read();
  return result.done ? "" : new TextDecoder().decode(result.value);
}

it("streams usable menus before badges, and priorities before a delayed feed", async () => {
  const initial = deferred<NavigationSnapshot | null>(), feed = deferred<string>();
  const tree = React.createElement(Provider, { base: "/holdings/acme", shortcutStorageKey: "test", initial: initial.promise },
    React.createElement(MenuShell),
    React.createElement(React.Suspense, { fallback: "Badge pending" }, React.createElement(NavigationBadge, { kind: "work" })),
    React.createElement(HomeShell, { base: "/holdings/acme", name: "Test", single: false, calendarEnabled: false, canRequestLegal: false },
      React.createElement(React.Suspense, { fallback: "Feed pending" }, React.createElement(Feed, { data: feed.promise }))),
    React.createElement(React.Suspense, { fallback: "Priorities pending" }, React.createElement(Priorities)));
  const stream = await renderToReadableStream(React.createElement("html", null, React.createElement("head"), React.createElement("body", null, tree)), { bootstrapScriptContent: "/* streaming test */" }), reader = stream.getReader();
  let shell = "";
  while (!shell.includes("Feed pending")) shell += await chunk(reader);
  expect(shell).toContain("Usable menu");
  expect(shell).toContain("Today");
  expect(shell).toContain("Badge pending");
  expect(shell).toContain("Feed pending");
  initial.resolve(snapshot);
  let priorities = "";
  while (!priorities.includes("Priorities: 7")) priorities += await chunk(reader);
  expect(priorities).toContain("Priorities: 7");
  expect(priorities).toContain("Needs you");
  expect(priorities).not.toContain("Loaded feed");
  feed.resolve("Loaded feed");
  let rest = "", next: string;
  while ((next = await chunk(reader))) rest += next;
  expect(rest).toContain("Loaded feed");
});

it("keeps the shell and retry UI renderable when the server seed fails", async () => {
  const stream = await renderToReadableStream(React.createElement(Provider,
    { base: "/holdings/acme", shortcutStorageKey: "test", initial: Promise.resolve(null) },
    React.createElement(MenuShell), React.createElement(React.Suspense, { fallback: "Loading" }, React.createElement(Priorities))));
  await stream.allReady;
  const text = await new Response(stream).text();
  expect(text).toContain("Usable menu");
  expect(text).toContain("Priorities: null");
});
