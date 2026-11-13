import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {changeManufacturingOrder as retained} from "@erpx/server-manufacturing/public/production";
import {changeManufacturingOrder as legacy} from "./adapters-production";

/**
 * Test-only legacy oracle for the retired TypeScript material-issue and
 * independent-QC acceptance writers.
 *
 * `manufacturing.order.issue` and `manufacturing.order.pass-qc` are owned by
 * the native Manufacturing command (`POST /manufacturing/orders/{id}/{action}`).
 * The PostgreSQL integration suite still needs their exact pre-cutover effects
 * to exercise purchase, supply, Finance and evidence behaviour, so the retired
 * implementation is retained here as a fixture. Production code must never
 * import this module.
 */
export function changeManufacturingOrderForTest(
  actor: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  const action = (raw as {action?: string} | null)?.action;
  return action === "issue" || action === "pass-qc"
    ? legacy(actor, id, raw, key)
    : retained(actor, id, raw, key);
}

export { changeManufacturingOrder as legacyChangeManufacturingOrder } from "./adapters-production";
