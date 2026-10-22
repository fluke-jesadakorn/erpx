import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";

const navigation = vi.hoisted(() => ({ loaded: vi.fn(), refresh: vi.fn() }));
vi.mock("next/navigation", () => {
  navigation.loaded();
  return { useRouter: () => ({ refresh: navigation.refresh }), usePathname: () => "/", useSearchParams: () => new URLSearchParams() };
});

it("lets the existing Front preference hook load without the unrelated Account controller or navigation", async () => {
  const { useAccountPreferences } = await import("@erpx/feature-identity_access/components/account-preferences-context");
  expect(navigation.loaded).not.toHaveBeenCalled();
  function ExistingConsumer() {
    return createElement("span", null, useAccountPreferences() === null ? "no-account-context" : "unexpected");
  }
  expect(renderToStaticMarkup(createElement(ExistingConsumer))).toBe("<span>no-account-context</span>");
});


it("transports existing absent, saved, pending and failed preference state without adding DOM", async () => {
  const { AccountPreferencesContext, useAccountPreferences } = await import("@erpx/feature-identity_access/components/account-preferences-context");
  const child = createElement("section", { className: "calendar-widget", "data-width": "640" }, "Existing calendar");
  const expected = renderToStaticMarkup(child);
  for (const preference of [undefined, { language: "th" as const, appearance: "dark" as const, version: 7 }]) {
    for (const error of ["", "Keep the captured retry"]) {
      const state = { key: "a".repeat(64), preference, pending: error === "", revision: 3, error };
      let consumed: unknown;
      function ExistingConsumer() { consumed = useAccountPreferences(); return child; }
      expect(renderToStaticMarkup(createElement(AccountPreferencesContext.Provider, { value: state }, createElement(ExistingConsumer)))).toBe(expected);
      expect(consumed).toBe(state);
    }
  }
  expect(navigation.loaded).not.toHaveBeenCalled();
});

it("keeps the actual Account controller's initial content and shared context without an SSR read or refresh", async () => {
  const { AccountPreferencesProvider } = await import("@erpx/feature-identity_access/components/account-preferences-provider");
  const { useAccountPreferences } = await import("@erpx/feature-identity_access/components/account-preferences-context");
  const fetch = vi.fn(() => { throw new Error("Initial Account preferences must not read on construction"); });
  vi.stubGlobal("fetch", fetch);
  try {
    for (const language of ["en", "th"] as const) for (const theme of ["system", "light", "dark"] as const) for (const appearance of ["legacy", "standard"] as const) {
      const snapshot = { key: "b".repeat(64), preference: { language, appearance: theme, version: 4 } };
      function ExistingConsumer() {
        const state = useAccountPreferences();
        return createElement("section", { className: "calendar-widget" }, `${state?.preference?.language}/${state?.preference?.appearance}/${state?.preference?.version}`);
      }
      const expected = renderToStaticMarkup(createElement("section", { className: "calendar-widget" }, `${language}/${theme}/4`));
      const providerProps = { snapshot, appearance, children: createElement(ExistingConsumer) };
      expect(renderToStaticMarkup(createElement(AccountPreferencesProvider, providerProps))).toBe(expected);
    }
    expect(fetch).not.toHaveBeenCalled();
    expect(navigation.refresh).not.toHaveBeenCalled();
  } finally { vi.unstubAllGlobals(); }
});
