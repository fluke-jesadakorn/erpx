import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  readQuickAccessPreferences,
  saveQuickAccessPreferences,
} from "../src/server/quick-access-preferences";

vi.mock("../src/server/runtime/env", () => ({ getRuntime: () => env }));

beforeAll(() => applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS));

function scope() {
  return { userId: crypto.randomUUID(), surface: "front-pos", contextKey: "acme:company-1" };
}

it("distinguishes defaults, explicit empty pins, and saved order across reads", async () => {
  const owner = scope();
  expect(await readQuickAccessPreferences(owner.userId, owner.surface, owner.contextKey))
    .toEqual({ pinnedIds: null, version: 0 });
  const first = await saveQuickAccessPreferences({
    ...owner, pinnedIds: [], expectedVersion: 0,
    availableIds: ["sale", "history"], requestKey: crypto.randomUUID(),
  });
  expect(first).toEqual({ pinnedIds: [], version: 1 });
  await saveQuickAccessPreferences({
    ...owner, pinnedIds: ["history", "sale"], expectedVersion: 1,
    availableIds: ["sale", "history"], requestKey: crypto.randomUUID(),
  });
  expect(await readQuickAccessPreferences(owner.userId, owner.surface, owner.contextKey))
    .toEqual({ pinnedIds: ["history", "sale"], version: 2 });
  await saveQuickAccessPreferences({
    ...owner, pinnedIds: null, expectedVersion: 2,
    availableIds: [], requestKey: crypto.randomUUID(),
  });
  expect(await readQuickAccessPreferences(owner.userId, owner.surface, owner.contextKey))
    .toEqual({ pinnedIds: null, version: 3 });
});

it("isolates users, surfaces, and contexts", async () => {
  const owner = scope();
  await saveQuickAccessPreferences({
    ...owner, pinnedIds: ["sale"], expectedVersion: 0,
    availableIds: ["sale"], requestKey: crypto.randomUUID(),
  });
  for (const other of [
    { ...owner, userId: crypto.randomUUID() },
    { ...owner, surface: "front-pms" },
    { ...owner, contextKey: "acme:company-2" },
  ]) {
    expect(await readQuickAccessPreferences(other.userId, other.surface, other.contextKey))
      .toEqual({ pinnedIds: null, version: 0 });
  }
});

it("replays retries and rejects a key reused for a different save", async () => {
  const owner = scope(), requestKey = crypto.randomUUID();
  const input = { ...owner, pinnedIds: ["sale"], expectedVersion: 0,
    availableIds: ["sale"], requestKey };
  const result = await saveQuickAccessPreferences(input);
  expect(await saveQuickAccessPreferences(input)).toEqual(result);
  await expect(saveQuickAccessPreferences({ ...input, pinnedIds: [] }))
    .rejects.toMatchObject({ status: 409, code: "IDEMPOTENCY_CONFLICT" });
  expect((await readQuickAccessPreferences(owner.userId, owner.surface, owner.contextKey)).version).toBe(1);
});

it("rejects stale versions and new actions missing from current grants", async () => {
  const owner = scope();
  await saveQuickAccessPreferences({
    ...owner, pinnedIds: ["sale"], expectedVersion: 0,
    availableIds: ["sale"], requestKey: crypto.randomUUID(),
  });
  await expect(saveQuickAccessPreferences({
    ...owner, pinnedIds: [], expectedVersion: 0,
    availableIds: ["sale"], requestKey: crypto.randomUUID(),
  })).rejects.toMatchObject({ status: 409, code: "VERSION_CONFLICT" });
  await expect(saveQuickAccessPreferences({
    ...owner, pinnedIds: ["history"], expectedVersion: 1,
    availableIds: ["sale"], requestKey: crypto.randomUUID(),
  })).rejects.toMatchObject({ status: 403, code: "SHORTCUT_UNAVAILABLE" });
  expect(await saveQuickAccessPreferences({
    ...owner, pinnedIds: ["sale"], expectedVersion: 1,
    availableIds: [], requestKey: crypto.randomUUID(),
  })).toEqual({ pinnedIds: ["sale"], version: 2 });
});

it("allows one winner for concurrent edits and validates six unique IDs", async () => {
  const owner = scope();
  const saves = await Promise.allSettled(["sale", "history"].map((id) =>
    saveQuickAccessPreferences({
      ...owner, pinnedIds: [id], expectedVersion: 0,
      availableIds: ["sale", "history"], requestKey: crypto.randomUUID(),
    })));
  expect(saves.filter((result) => result.status === "fulfilled")).toHaveLength(1);
  await expect(saveQuickAccessPreferences({
    ...owner, pinnedIds: ["sale", "sale"], expectedVersion: 1,
    availableIds: ["sale"], requestKey: crypto.randomUUID(),
  })).rejects.toBeDefined();
  await expect(saveQuickAccessPreferences({
    ...owner, pinnedIds: ["1", "2", "3", "4", "5", "6", "7"], expectedVersion: 1,
    availableIds: [], requestKey: crypto.randomUUID(),
  })).rejects.toBeDefined();
});
