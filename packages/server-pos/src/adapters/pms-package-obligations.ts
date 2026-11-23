import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { assertPmsPackageObligationSources } from "@erpx/server-pms/public/package-obligations";
import { assertPosPackageObligationSources } from "@erpx/server-pos/adapters/package-obligations";
import { nativePosPmsPackageObligationsStatement } from "./native-pos-obligation-statement";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
/** The shared provider transport remains POS-owned. This reader is scoped to
 * Hotel v1, independently of POS base product withdrawal.
 *
 * CROSS-OWNER CONSUMER (retained TypeScript, removal documented): called by
 * `packages/server-commercial/src/adapters/owner-package-obligations.ts` for
 * the `pms` package. Its source lease is asserted in-process against the PMS
 * owner (`assertPmsPackageObligationSources`) and the PMS settlement fragments
 * consumed by this family remain in
 * `packages/server-pms/src/adapters/package-obligations.ts`; those callers need
 * their own native source port and removal task before retirement. The owner
 * SQL is mirrored in `erpx_server_pos::public::obligation_reads`. */
export async function pmsPosPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  assertPosPackageObligationSources(owner, database, input.companyId);
  assertPmsPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "pos" || input.packageId !== "pms" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("PMS_POS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const facts = await nativePosPmsPackageObligationsStatement(database, input.companyId);
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "pos", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
