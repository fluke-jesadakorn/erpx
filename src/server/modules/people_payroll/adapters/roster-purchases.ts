import "server-only";
import { randomUUID } from "node:crypto";
import type { WorkContext } from "../../identity_access/public/authorization";
import { iamMutationGuards } from "../../identity_access/public/iam-store";
import { workContextGuards } from "../../workflow/public/commands";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "../../runtime/public/runtime/types";
import { ApiError } from "../../runtime/public/http";
import { checkPurchasedCapability, companyPurchaseReadLocks, holdingPurchaseReadLocks, peoplePackageEnforcementMode, purchaseCommandBoundary, purchaseCommandFailure, readPurchasedAccess } from "../../commercial/public/purchase-command-boundary";
import type { CapabilityPurpose } from "../../commercial/public/rules/packages";
import type { RosterCommand, RosterData } from "../domain/roster";

/** Existing requests and cancellations discharge already-created obligations.
 * Publishing, assigning and creating requests continue to require active work. */
export function rosterPurchasePurpose(input: RosterCommand): CapabilityPurpose {
  return input.action === "shift.cancel" || input.action === "request.decide" ? "settle-existing" : "new-work";
}

async function requireCurrentRosterAuthority(database: SqlDatabase, actor: WorkContext, key: string) {
  const now = Date.now();
  const assignment = await database.prepare(`SELECT id FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=?
    AND valid_from<=? AND valid_until>? AND revoked_at IS NULL
    AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) FOR SHARE`)
    .bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, now, now, actor.branchId, actor.branchId).first();
  if (!assignment) throw new ApiError(403, "ROSTER_ACCESS_CHANGED", "Choose a current roster assignment.");
  await database.batch([...workContextGuards(database, actor, key), ...iamMutationGuards(database, actor, key), database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}

export async function rosterPurchasedAccess(database: SqlDatabase, actor: WorkContext): Promise<NonNullable<RosterData["purchase"]> | undefined> {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return undefined;
  await checkPurchasedCapability(database, mode, {companyId: actor.companyId, capability: "people.workforce", purpose: "history"});
  await requireCurrentRosterAuthority(database, actor, `roster-read:${randomUUID()}`);
  const access = await readPurchasedAccess(database, mode, {companyId: actor.companyId, capability: "people.workforce"});
  return {newWork: access.newWork, settleExisting: access.settleExisting, history: access.history, revision: access.revision};
}

/** Shared projections can omit unavailable Roster rows while retaining other
 * Calendar sources. The original acting authority is still checked live. */
export async function rosterSourceAccess(database: SqlDatabase, actor: WorkContext): Promise<NonNullable<RosterData["purchase"]> | undefined> {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return undefined;
  await requireCurrentRosterAuthority(database, actor, `roster-source:${randomUUID()}`);
  const access = await readPurchasedAccess(database, mode, {companyId: actor.companyId, capability: "people.workforce"});
  return {newWork: access.newWork, settleExisting: access.settleExisting, history: access.history, revision: access.revision};
}

/** Called by both the Roster owner and Calendar's existing shift-reschedule
 * transaction. Receipt/audit/outbox, notifications and final deferred effects
 * remain in the same original Holding transaction. */
export async function withRosterPurchasedTransaction<Result>(owner: SqlDatabase, actor: WorkContext,
  command: {key: string; operation: string; purpose: CapabilityPurpose}, run: (transaction: SqlDatabase) => Promise<Result>): Promise<Result> {
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, "ROSTER_TRANSACTION_REQUIRED", "Roster changes require PostgreSQL transactions.");
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return owner.transaction(run);
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, actor.companyId));
      const replay = await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
        .bind(command.key, command.operation, actor.userId, actor.companyId).first();
      const scope = {companyId: actor.companyId, capability: "people.workforce", purpose: replay ? "history" as const : command.purpose};
      await checkPurchasedCapability(transaction, mode, scope);
      const key = `roster-purchase:${randomUUID()}`, boundary = purchaseCommandBoundary(transaction, mode, key, scope)!;
      await transaction.batch(boundary.before);
      await requireCurrentRosterAuthority(transaction, actor, key);
      const result = await run(transaction);
      await transaction.prepare("SET CONSTRAINTS ALL IMMEDIATE").run();
      await requireCurrentRosterAuthority(transaction, actor, key);
      const observations = await transaction.batch([...boundary.after, transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
      boundary.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}

/** Coverage monitoring retains its independent scheduled authority. Company
 * purchase fences precede the owner source snapshot; inactive or unpurchased
 * Companies cannot generate new notices, while draining work stays monitored. */
export async function withRosterCoveragePurchasedTransaction<Result>(owner: SqlDatabase,
  run: (transaction: SqlDatabase, permitsCompany: (companyId: string) => Promise<boolean>) => Promise<Result>): Promise<Result> {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return run(owner, async () => true);
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, "ROSTER_TRANSACTION_REQUIRED", "Roster coverage requires PostgreSQL transactions.");
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(holdingPurchaseReadLocks(transaction));
      const decisions = new Map<string, boolean>();
      const boundaries: {key: string; boundary: NonNullable<ReturnType<typeof purchaseCommandBoundary>>}[] = [];
      const result = await run(transaction, async companyId => {
        if (decisions.has(companyId)) return decisions.get(companyId)!;
        const scope = {companyId, capability: "people.workforce", purpose: "settle-existing" as const};
        const access = await readPurchasedAccess(transaction, mode, scope);
        decisions.set(companyId, access.settleExisting);
        if (access.settleExisting) {
          const key = `roster-coverage:${randomUUID()}`, boundary = purchaseCommandBoundary(transaction, mode, key, scope)!;
          await transaction.batch(boundary.before);
          boundaries.push({key, boundary});
        }
        return access.settleExisting;
      });
      await transaction.prepare("SET CONSTRAINTS ALL IMMEDIATE").run();
      const observations = await transaction.batch(boundaries.flatMap(({key, boundary}) => [...boundary.after, transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]));
      boundaries[0]?.boundary.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
