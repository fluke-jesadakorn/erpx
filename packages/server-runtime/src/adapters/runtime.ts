import "server-only";

import { registeredMigrationDatabase, registeredResources, resourceSlots, type HoldingId } from "@erpx/feature-runtime/lib/holding-registry";
export type { HoldingId } from "@erpx/feature-runtime/lib/holding-registry";
import { getRuntime, type RuntimeEnv } from "@erpx/server-runtime/adapters/runtime/env";
import { holdingBindingsSql } from './read-tail-sql';

export async function runtimeEnv(): Promise<RuntimeEnv> { return getRuntime(); }

const HOLDING_BINDINGS_SELECT = holdingBindingsSql('holding_id, slug, display_name');

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

export async function holdingBindingBySlug(
  slug: string,
): Promise<HoldingBinding> {
  const env = await runtimeEnv();
  const row = await env.CONTROL_DB.prepare(
    `${HOLDING_BINDINGS_SELECT}
     WHERE slug = ? AND status = 'active'`,
  )
    .bind(slug)
    .first<{ holding_id: string; slug: string; display_name: string }>();
  if (!row || !resourceSlots(env).some(s => s.id === row.holding_id)) throw new Error("HOLDING_NOT_FOUND");
  return { id: row.holding_id, slug: row.slug, displayName: row.display_name };
}

export async function defaultHoldingBinding(): Promise<HoldingBinding> {
  const env = await runtimeEnv();
  const row = await env.CONTROL_DB.prepare(
    `${HOLDING_BINDINGS_SELECT}
     WHERE status = 'active' ORDER BY holding_id LIMIT 1`,
  ).first<{ holding_id: string; slug: string; display_name: string }>();
  if (!row || !resourceSlots(env).some(s => s.id === row.holding_id)) throw new Error("HOLDING_NOT_FOUND");
  return { id: row.holding_id, slug: row.slug, displayName: row.display_name };
}

export async function activeHoldingBindings(): Promise<HoldingBinding[]> {
  const env = await runtimeEnv();
  const result = await env.CONTROL_DB.prepare(
    `${HOLDING_BINDINGS_SELECT}
     WHERE status = 'active' ORDER BY holding_id`,
  ).all<{ holding_id: string; slug: string; display_name: string }>();
  return result.results
    .filter((row) => resourceSlots(env).some(s => s.id === row.holding_id))
    .map((row) => ({
      id: row.holding_id as HoldingId,
      slug: row.slug,
      displayName: row.display_name,
    }));
}
