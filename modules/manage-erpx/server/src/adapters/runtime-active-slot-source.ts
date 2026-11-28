import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeBetaSlotActive } from "./native-owner-read";
/** The native owner plans the byte-identical active-slot read; this adapter
 * only transports the bounded holding id and the boolean result. */
export async function runtimeBetaSlotActive(db: SqlDatabase, holdingId: string): Promise<boolean> {
  return nativeBetaSlotActive(db, holdingId);
}
