import "server-only";

import { registeredMigrationDatabase, registeredResources, resourceSlots, type HoldingId } from "@erpx/feature-runtime/lib/holding-registry";
export type { HoldingId } from "@erpx/feature-runtime/lib/holding-registry";
import { getRuntime, type RuntimeEnv } from "@erpx/server-runtime/adapters/runtime/env";
import {
  readNativeActiveHoldingBindings,
  readNativeDefaultHoldingBinding,
  readNativeHoldingBindingBySlug,
  type NativeHoldingBinding,
} from "./native-lane-33/client";

export async function runtimeEnv(): Promise<RuntimeEnv> { return getRuntime(); }

export type HoldingBinding = {
  id: HoldingId;
  slug: string;
  displayName: string;
};

export function isHoldingId(value: string): value is HoldingId {
  return /^[a-z0-9]{3,24}$/.test(value);
}
export async function holdingResources(holdingId: string) {
  return registeredResources(await runtimeEnv(), holdingId);
}
export async function holdingMigrationDatabase(holdingId: string) {
  return registeredMigrationDatabase(await runtimeEnv(), holdingId);
}

function project(binding: NativeHoldingBinding): HoldingBinding {
  return { id: binding.holdingId as HoldingId, slug: binding.slug, displayName: binding.displayName };
}

export async function holdingBindingBySlug(
  slug: string,
): Promise<HoldingBinding> {
  const env = await runtimeEnv();
  const scope = resourceSlots(env).map(slot => slot.id);
  const binding = await readNativeHoldingBindingBySlug(scope, slug);
  if (!binding) throw new Error("HOLDING_NOT_FOUND");
  return project(binding);
}

export async function defaultHoldingBinding(): Promise<HoldingBinding> {
  const env = await runtimeEnv();
  const scope = resourceSlots(env).map(slot => slot.id);
  const binding = await readNativeDefaultHoldingBinding(scope);
  if (!binding) throw new Error("HOLDING_NOT_FOUND");
  return project(binding);
}

export async function activeHoldingBindings(): Promise<HoldingBinding[]> {
  const env = await runtimeEnv();
  const scope = resourceSlots(env).map(slot => slot.id);
  const bindings = await readNativeActiveHoldingBindings(scope);
  return bindings.map(project);
}
