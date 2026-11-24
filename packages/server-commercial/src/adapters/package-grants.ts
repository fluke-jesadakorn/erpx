import "server-only";
import { purchaseFactAgg } from "./commercial-read-tail-r4-sources";
import { receiptProbe } from "./commercial-read-tail-r5-sources";
import { companyScopedStarSql } from "./commercial-read-tail-r8-sources";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { packageGrantCommands, type PackageGrantUnit, type GrantChange } from "@erpx/server-commercial/application/package-grants";
import { canonicalPackageGrant, type CompanyPackageGrant, type CompanyPackageHistoryRight } from "@erpx/server-commercial/domain/packages";
import { validatePackageDependencies } from "@erpx/server-commercial/domain/package-dependencies";
import { packageGrantBatchCommands, type PackageGrantBatch, type PackageGrantBatchPreview, type PackageGrantBatchUnit } from "@erpx/server-commercial/application/package-grant-batches";

import { packageWriterFence } from "@erpx/server-commercial/adapters/package-locks";
import { nativeGrantSave } from "@erpx/server-commercial/adapters/native-wave2";
import { nativeCommercialOwnerWrite, NativeCommercialOwnerWriteError } from "@erpx/server-commercial/adapters/native-owner-write";
async function ownerWrite(kind: any, values: readonly unknown[], recordId: string, key: string, holdId?: string) {
  try { return await nativeCommercialOwnerWrite(kind, values, recordId, 'system', key, holdId); }
  catch (error) { if (error instanceof NativeCommercialOwnerWriteError) throw new Error(error.code + ': ' + error.message); throw error; }
}

type Row = { company_id: string; package_id: string; package_version: number; state: CompanyPackageGrant["state"]; starts_at: number; ends_at: number | null; version: number; reason: string; evidence: string };
const grantFromRow = (row: Row): CompanyPackageGrant => ({ companyId: row.company_id, packageId: row.package_id, packageVersion: Number(row.package_version), state: row.state, startsAt: Number(row.starts_at), endsAt: row.ends_at === null ? null : Number(row.ends_at), version: Number(row.version), reason: row.reason, evidence: row.evidence });
type HistoryRow = { company_id: string; package_id: string; package_version: number; source_grant_version: number; source_event_id: string; retained_at: number };
const historyFromRow = (row: HistoryRow): CompanyPackageHistoryRight => ({ companyId: row.company_id, packageId: row.package_id,
  packageVersion: Number(row.package_version), sourceGrantVersion: Number(row.source_grant_version), sourceEventId: row.source_event_id, retainedAt: Number(row.retained_at) });

/** One presentation snapshot. Audit/history bodies stay inside Commercial. */
export async function readCompanyPurchaseFacts(database: SqlDatabase, companyId: string) {
  const row = await database.prepare(`SELECT
    COALESCE((${purchaseFactAgg("company_package_grants","g","package_id")}),'[]'::jsonb) grants,
    COALESCE((${purchaseFactAgg("company_package_history_rights","h","package_id,package_version")}),'[]'::jsonb) history`)
    .bind(companyId, companyId).first<{ grants: Row[] | string; history: HistoryRow[] | string }>();
  if (!row) throw new Error("COMPANY_PURCHASE_FACTS_REQUIRED");
  const grants = typeof row.grants === "string" ? JSON.parse(row.grants) as Row[] : row.grants;
  const history = typeof row.history === "string" ? JSON.parse(row.history) as HistoryRow[] : row.history;
  return { grants: grants.map(grantFromRow), history: history.map(historyFromRow) };
}

/** Commands call on the same Company-locked owner transaction as grant reads. */
export async function readCompanyPackageHistoryRights(database: SqlDatabase, companyId: string) {
  return (await database.prepare(companyScopedStarSql("company_package_history_rights", " ORDER BY package_id,package_version"))
    .bind(companyId).all<HistoryRow>()).results.map(historyFromRow);
}

/** Required authority/obligation ports deliberately have no permissive defaults. */
export function packageGrantStore(db: PostgresDatabase, ports: {
  authorize(database: SqlDatabase, companyId: string): Promise<{ userId: string }>;
  obligations(database: SqlDatabase, companyId: string, packageId: string): Promise<number>;
  beforeTransaction?(database: SqlDatabase): Promise<void>;
  now?: () => number;
}) {
  const now = ports.now ?? Date.now;
  // Native Service owns every version/state, including replays. This fence is
  // below HTTP dispatch so legacy migrations/internal callers cannot bypass it.
  function legacyGrant(grant:CompanyPackageGrant){if(['service','service-parts-assets'].includes(grant.packageId))throw new Error('NATIVE_SERVICE_PACKAGE_REQUIRED');}
  function legacyBatch(input:PackageGrantBatch){for(const change of input.changes)legacyGrant(change.grant);}

  function unit(database: SqlDatabase): PackageGrantUnit {
    return {
      authorize: companyId => ports.authorize(database, companyId),
      lock: async key => { await database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`commercial.package:${key}`).run(); },
      current: async (companyId, packageId) => {
        const row = await database.prepare(companyScopedStarSql("company_package_grants", " AND package_id=?")).bind(companyId, packageId).first<Row>();
        return row ? grantFromRow(row) : null;
      },
      receipt: async key => {
        if (await database.prepare(receiptProbe("company_package_change_sets","1")).bind(key).first()) throw new Error("IDEMPOTENCY_CONFLICT");
        const row = await database.prepare(receiptProbe("company_package_grant_events","request_hash,after_json")).bind(key).first<{ request_hash: string; after_json: CompanyPackageGrant | string }>();
        return row ? { hash: row.request_hash, grant: typeof row.after_json === "string" ? JSON.parse(row.after_json) as CompanyPackageGrant : row.after_json } : null;
      },
      obligations: (companyId, packageId) => ports.obligations(database, companyId, packageId),
      dependencies: async grant => validatePackageDependencies(grant, await readCompanyPackageGrants(database, grant.companyId), now()),
      save: async (input, current, actorId, hash) => {
        legacyGrant(input.grant);
        const g = input.grant, instant = now();
        // Write authority moved to the native Commercial axum command route:
        // it executes the grant upsert and grant-event insert in the caller's
        // serializable unit, with its own receipt/audit/outbox. No TS SQL and
        // no fallback remain here; missing native config fails closed.
        await nativeGrantSave({
          companyId: g.companyId,
          packageId: g.packageId,
          packageVersion: g.packageVersion,
          state: g.state,
          startsAt: g.startsAt,
          endsAt: g.endsAt,
          version: g.version,
          reason: g.reason,
          evidence: g.evidence,
          updatedAt: instant,
          actorId,
          idempotencyKey: input.idempotencyKey,
          requestHash: hash,
          beforeJson: current ? JSON.stringify(current) : null,
          afterJson: JSON.stringify(g),
        });
      },
    };
  }
  const hash = (value: string) => createHash("sha256").update(value).digest("hex");
  const commands = packageGrantCommands({ unit: () => unit(db), transaction: action => db.transaction(async tx => { await packageWriterFence(tx).run(); await ports.beforeTransaction?.(tx); return action(unit(tx)); }), hash });
  function batchUnit(database: SqlDatabase, locking = false): PackageGrantBatchUnit {
    return {
      grants: unit(database), all: async companyId => {
        if (locking) {
          // The grant trigger also advances this row for older compatible code.
          // Lock it to detect inserts outside an older SERIALIZABLE snapshot.
          await ownerWrite('grant.revision-anchor', [companyId], companyId, `commercial:revision:${companyId}`);
          await database.prepare("SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE").bind(companyId).first();
        }
        return readCompanyPackageGrants(database, companyId, locking);
      },
      revision: packageGrantRevision,
      previewDependencies: async (grant, prospective) => validatePackageDependencies(grant, prospective, now()),
      receipt: async key => {
        if (await database.prepare(receiptProbe("company_package_grant_events","1")).bind(key).first()) throw new Error("IDEMPOTENCY_CONFLICT");
        const row = await database.prepare(receiptProbe("company_package_change_sets","request_hash,result_json")).bind(key)
          .first<{ request_hash: string; result_json: PackageGrantBatchPreview | string }>();
        return row ? { hash: row.request_hash, result: typeof row.result_json === "string" ? JSON.parse(row.result_json) as PackageGrantBatchPreview : row.result_json } : null;
      },
      saveReceipt: async (key, digest, actorId, result) => {
        await ownerWrite('change-set.receipt', [key, result.companyId, actorId, digest, JSON.stringify(result), now()], result.companyId, key);
      },
    };
  }
  const batch = packageGrantBatchCommands({ unit: () => batchUnit(db), transaction: action => db.transaction(async tx => { await packageWriterFence(tx).run(); await ports.beforeTransaction?.(tx); return action(batchUnit(tx, true)); }), hash });
  return { ...commands,
    preview(input:GrantChange){legacyGrant(input.grant);return commands.preview(input);},
    apply(input:GrantChange){legacyGrant(input.grant);return commands.apply(input);},
    batch: { ...batch,
    preview(input:PackageGrantBatch){legacyBatch(input);return batch.preview(input);},
    apply(input:PackageGrantBatch,verifyPreview:(preview:PackageGrantBatchPreview)=>Promise<void>){legacyBatch(input);return batch.apply(input,verifyPreview);},
    /** Apply through normal Commercial policy/audit in an owner's existing unit.
     * The owner can validate its source lease after the receipt, before COMMIT. */
    applyIn(transaction: SqlDatabase, input: PackageGrantBatch, verifyPreview: (preview: PackageGrantBatchPreview) => Promise<void>) {
      legacyBatch(input);
      db.assertActiveTransaction(transaction);
      return packageGrantBatchCommands({ unit: () => batchUnit(transaction), hash,
        transaction: async action => { await packageWriterFence(transaction).run(); await ports.beforeTransaction?.(transaction); return action(batchUnit(transaction, true)); },
      }).apply(input, verifyPreview);
    },
    /** Read-only preview on the owner's existing snapshot. It neither checks
     * out another pool connection nor inserts a revision anchor. */
    previewIn(transaction: SqlDatabase, input: PackageGrantBatch) {
      legacyBatch(input);
      db.assertActiveTransaction(transaction);
      return packageGrantBatchCommands({ unit: () => batchUnit(transaction), hash,
        transaction: async () => { throw new Error("PREVIEW_CANNOT_WRITE"); } }).preview(input);
    },
  } };
}

/** Presentation only; command adapters must read grants in their own write transaction. */
export async function companyPackageGrants(context: WorkContext): Promise<CompanyPackageGrant[]> {
  const { write } = await holdingResources(context.holdingId);
  return readCompanyPackageGrants(write, context.companyId);
}

/** Reads on the supplied owner unit; commands must supply their protected transaction. */
export async function readCompanyPackageGrants(database: SqlDatabase, companyId: string, forShare = false): Promise<CompanyPackageGrant[]> {
  return (await database.prepare(companyScopedStarSql("company_package_grants", ` ORDER BY package_id${forShare ? " FOR SHARE" : ""}`)).bind(companyId).all<Row>()).results.map(grantFromRow);
}

/** Optimistic Company revision binds every persisted grant, including unchanged packages. */
export function packageGrantRevision(companyId: string, grants: readonly CompanyPackageGrant[]) {
  return createHash("sha256").update(JSON.stringify({ companyId, grants: grants.filter(grant => grant.companyId === companyId)
    .map(canonicalPackageGrant).sort((a, b) => a.packageId.localeCompare(b.packageId)) })).digest("hex");
}
