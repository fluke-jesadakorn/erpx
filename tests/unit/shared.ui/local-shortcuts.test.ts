import { expect, it } from "vitest";
import { localShortcutKey, parseLocalShortcuts, serializeLocalShortcuts } from "@erpx/feature-work_projection/lib/local-shortcuts";
import { visibleQuickAccessItems } from "@erpx/feature-work_projection/lib/quick-access";

it("isolates local pins by user, company and workspace scope", () => {
  const key = localShortcutKey("user-a", "company-a", "/holdings/acme");
  expect(localShortcutKey("user-b", "company-a", "/holdings/acme")).not.toBe(key);
  expect(localShortcutKey("user-a", "company-b", "/holdings/acme")).not.toBe(key);
  expect(localShortcutKey("user-a", "company-a", "/holdings/acme/branches/bangkok")).not.toBe(key);
  expect(localShortcutKey("a:b", "c", "d")).not.toBe(localShortcutKey("a", "b:c", "d"));
});

it("retains order, intentionally empty pins and restored defaults", () => {
  for (const pinnedIds of [["services", "expenses"], [], null]) {
    const preference = { pinnedIds, version: 3 };
    expect(parseLocalShortcuts(serializeLocalShortcuts(preference))).toEqual(preference);
  }
});

it("ignores corrupt, unsupported or invalid saved data", () => {
  for (const raw of [null, "{", "null", '{"schema":2,"pinnedIds":[],"version":1}',
    '{"schema":1,"pinnedIds":["tasks","tasks"],"version":1}',
    '{"schema":1,"pinnedIds":{},"version":1}',
    '{"schema":1,"pinnedIds":[],"version":-1}',
    JSON.stringify({schema:1, pinnedIds:["a","b","c","d","e","f","g"],version:1})]) {
    expect(parseLocalShortcuts(raw)).toBeNull();
  }
});

it("never renders a locally saved pin absent from the current authorized catalogue", () => {
  const preference = parseLocalShortcuts(serializeLocalShortcuts({
    pinnedIds: ["finance-payments", "expenses"], version: 1,
  }))!;
  expect(visibleQuickAccessItems([{id:"expenses"}], preference, [])).toEqual([{id:"expenses"}]);
});
