import type { SqlDatabase, FileStore } from "@/server/modules/runtime/adapters/runtime/types";
/** The same operator-owned binding registry is used by the app and queue worker. */
export type HoldingId = string;
export type ResourceSlot = {
  id: string;
  write: string;
  read: string;
  files: string;
  beta?: boolean;
  internal?: boolean;
};
export type RegistryEnv = { ERPX_RESOURCE_SLOTS?: string };
const legacy: ResourceSlot[] = [1, 2, 3].map((n) => ({
  id: `h00${n}`,
  write: `H00${n}_DB`,
  read: `H00${n}_READ_DB`,
  files: `H00${n}_FILES`,
  ...(n === 3 ? { internal: true } : {}),
}));
export function resourceSlots(env: RegistryEnv): ResourceSlot[] {
  const value: unknown = env.ERPX_RESOURCE_SLOTS
    ? JSON.parse(env.ERPX_RESOURCE_SLOTS)
    : legacy;
  if (!Array.isArray(value) || value.length > 22)
    throw new Error("INVALID_RESOURCE_REGISTRY");
  const seen = new Set<string>();
  for (const slot of value) {
    if (
      !slot ||
      typeof slot !== "object" ||
      !/^[a-z0-9]{3,24}$/.test(slot.id) ||
      ![slot.write, slot.read, slot.files].every(
        (x) => typeof x === "string" && /^[A-Z][A-Z0-9_]{2,60}$/.test(x),
      )
    )
      throw new Error("INVALID_RESOURCE_REGISTRY");
    for (const name of [slot.id, slot.write, slot.read, slot.files]) {
      if (seen.has(name)) throw new Error("DUPLICATE_RESOURCE_BINDING");
      seen.add(name);
    }
  }
  if (value.filter((s) => s.beta).length > 20)
    throw new Error("BETA_CAPACITY_EXCEEDED");
  return value;
}
export function registeredDatabases(env: RegistryEnv, id: string) {
  const slot = resourceSlots(env).find((s) => s.id === id);
  if (!slot) throw new Error("HOLDING_NOT_FOUND");
  const bindings = env as Record<string, unknown>;
  const write = bindings[slot.write] as SqlDatabase | undefined;
  const read = bindings[slot.read] as SqlDatabase | undefined;
  if (!write?.prepare || !read?.prepare)
    throw new Error("HOLDING_RESOURCES_UNAVAILABLE");
  return { write, read, slot };
}
export function registeredResources(env: RegistryEnv, id: string) {
  const { write, read, slot } = registeredDatabases(env, id);
  const files = (env as Record<string, unknown>)[slot.files] as
    | FileStore
    | undefined;
  if (!files?.get || !files?.put)
    throw new Error("HOLDING_RESOURCES_UNAVAILABLE");
  return { write, read, files };
}
export function registeredMigrationDatabase(env: RegistryEnv, id: string) {
  const { slot } = registeredDatabases(env, id);
  const migration = (env as Record<string, unknown>)[`MIGRATION_${slot.write}`] as SqlDatabase | undefined;
  if (!migration?.prepare) throw new Error("HOLDING_MIGRATION_RESOURCE_UNAVAILABLE");
  return migration;
}
