import "server-only";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import { capabilityRevisionSource, effectiveCapabilityDecision, type EffectiveCapabilityInput } from "../domain/capability-decision";
import { readCompanyPackageGrants, readCompanyPurchaseFacts, readCompanyPackageHistoryRights } from "./package-grants";
import { packageWriterFence } from "./package-locks";

function decide(input: EffectiveCapabilityInput) {
  const decision = effectiveCapabilityDecision(input);
  const revision = createHash("sha256").update(capabilityRevisionSource(input.companyId, input.grants, input.now, input.history))
    .update(JSON.stringify({ capability: input.capability, purpose: input.purpose, ...decision })).digest("hex");
  return { ...decision, revision };
}

/** Presentation snapshot; command callers use the protected reader below. */
export async function readEffectiveCapability(database: SqlDatabase, input: Omit<EffectiveCapabilityInput, "grants" | "history">) {
  const facts = input.purpose === "history" ? await readCompanyPurchaseFacts(database, input.companyId)
    : { grants: await readCompanyPackageGrants(database, input.companyId), history: [] };
  return decide({ ...input, ...facts });
}

/** Call at the start of the existing owner transaction, before aggregate locks.
 * Serializes with grant Apply and evaluates owner gates inside that same unit. */
export async function readCommandCapability(owner: PostgresDatabase, transaction: SqlDatabase,
  input: Pick<EffectiveCapabilityInput, "companyId" | "capability" | "purpose">,
  gates: (database: SqlDatabase) => Promise<Pick<EffectiveCapabilityInput, "release" | "authority" | "record">>) {
  owner.assertActiveTransaction(transaction);
  await packageWriterFence(transaction).run();
  await transaction.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
    .bind(`commercial.package:company:${input.companyId}`).run();
  // A SERIALIZABLE snapshot can predate the advisory-lock wait. Row locking
  // turns an intervening grant update into a normal whole-transaction retry,
  // rather than allowing a command against the old active grant.
  const grants = await readCompanyPackageGrants(transaction, input.companyId, true);
  const history = input.purpose === "history" ? await readCompanyPackageHistoryRights(transaction, input.companyId) : [];
  const currentGates = await gates(transaction);
  // Waiting for the Company lock or owner gates must never preserve a stale
  // pre-wait purchase timestamp. Do not accept an HTTP/caller clock here.
  return decide({ companyId: input.companyId, capability: input.capability,
    purpose: input.purpose, grants, history, ...currentGates, now: Date.now() });
}
