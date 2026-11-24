import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS audit actor reads for caller-owned transactions. The SQL
 * for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::audit_reads`); this bridge sends only a typed
 * kind and the positional record id. The caller executes the statement on its
 * own unit, so no TypeScript SQL, mirror or fallback reader remains for this
 * family. Row locks, advisory/LOCK, PMS cross-owner reads, the day-report and
 * Stripe registries stay in their callers and dedicated modules.
 */

const AUDIT_ACTOR_CHARGE_TO_ROOM = 'pos_audit_actor_charge_to_room';
const AUDIT_ACTOR_KITCHEN_START = 'pos_audit_actor_kitchen_start';

type ActorFacts = {
  id: string | number | boolean | null;
};

/** Actor that charged a sale to the room, by audit record id. */
export async function nativePosAuditChargeToRoomStatement<T = Record<string, unknown>>(db: SqlDatabase, facts: ActorFacts): Promise<T | null> {
  return nativePosOne<T>(db, '', AUDIT_ACTOR_CHARGE_TO_ROOM, [facts.id]);
}

/** Actor that started a kitchen fulfillment, by audit record id. */
export async function nativePosAuditKitchenStartStatement<T = Record<string, unknown>>(db: SqlDatabase, facts: ActorFacts): Promise<T | null> {
  return nativePosOne<T>(db, '', AUDIT_ACTOR_KITCHEN_START, [facts.id]);
}
