import "server-only";
import { createHash } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { packageGrantCommands, type PackageGrantUnit } from "@erpx/server-commercial/application/package-grants";
import { canonicalPackageGrant, type CompanyPackageGrant, type CompanyPackageHistoryRight } from "@erpx/server-commercial/domain/packages";
import { validatePackageDependencies } from "@erpx/server-commercial/domain/package-dependencies";
import { packageGrantBatchCommands, type PackageGrantBatch, type PackageGrantBatchPreview, type PackageGrantBatchUnit } from "@erpx/server-commercial/application/package-grant-batches";

import { packageWriterFence } from "@erpx/server-commercial/adapters/package-locks";

type Row = { company_id: string; package_id: string; package_version: number; state: CompanyPackageGrant["state"]; starts_at: number; ends_at: number | null; version: number; reason: string; evidence: string };
const grantFromRow = (row: Row): CompanyPackageGrant => ({ companyId: row.company_id, packageId: row.package_id, packageVersion: Number(row.package_version), state: row.state, startsAt: Number(row.starts_at), endsAt: row.ends_at === null ? null : Number(row.ends_at), version: Number(row.version), reason: row.reason, evidence: row.evidence });
type HistoryRow = { company_id: string; package_id: string; package_version: number; source_grant_version: number; source_event_id: string; retained_at: number };
const historyFromRow = (row: HistoryRow): CompanyPackageHistoryRight => ({ companyId: row.company_id, packageId: row.package_id,
  packageVersion: Number(row.package_version), sourceGrantVersion: Number(row.source_grant_version), sourceEventId: row.source_event_id, retainedAt: Number(row.retained_at) });

/** One presentation snapshot. Audit/history bodies stay inside Commercial. */
export async function readCompanyPurchaseFacts(database: SqlDatabase, companyId: string) {
  const row = await database.prepare(`SELECT
    COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?),'[]'::jsonb) grants,
    COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?),'[]'::jsonb) history`)
    .bind(companyId, companyId).first<{ grants: Row[] | string; history: HistoryRow[] | string }>();
  if (!row) throw new Error("COMPANY_PURCHASE_FACTS_REQUIRED");
  const grants = typeof row.grants === "string" ? JSON.parse(row.grants) as Row[] : row.grants;
  const history = typeof row.history === "string" ? JSON.parse(row.history) as HistoryRow[] : row.history;
  return { grants: grants.map(grantFromRow), history: history.map(historyFromRow) };
}

/** Commands call on the same Company-locked owner transaction as grant reads. */
export async function readCompanyPackageHistoryRights(database: SqlDatabase, companyId: string) {
  return (await database.prepare("SELECT * FROM company_package_history_rights WHERE company_id=? ORDER BY package_id,package_version")
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
  function unit(database: SqlDatabase): PackageGrantUnit {
    return {
      authorize: companyId => ports.authorize(database, companyId),
      lock: async key => { await database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`commercial.package:${key}`).run(); },
      current: async (companyId, packageId) => {
        const row = await database.prepare("SELECT * FROM company_package_grants WHERE company_id=? AND package_id=?").bind(companyId, packageId).first<Row>();
        return row ? grantFromRow(row) : null;
      },
      receipt: async key => {
        if (await database.prepare("SELECT 1 FROM company_package_change_sets WHERE id=?").bind(key).first()) throw new Error("IDEMPOTENCY_CONFLICT");
        const row = await database.prepare("SELECT request_hash,after_json FROM company_package_grant_events WHERE id=?").bind(key).first<{ request_hash: string; after_json: CompanyPackageGrant | string }>();
        return row ? { hash: row.request_hash, grant: typeof row.after_json === "string" ? JSON.parse(row.after_json) as CompanyPackageGrant : row.after_json } : null;
      },
      obligations: (companyId, packageId) => ports.obligations(database, companyId, packageId),
      dependencies: async grant => validatePackageDependencies(grant, await readCompanyPackageGrants(database, grant.companyId), now()),
      save: async (input, current, actorId, hash) => {
        const g = input.grant, instant = now();
        await database.prepare(`INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)
          VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,package_id) DO UPDATE SET package_version=excluded.package_version,state=excluded.state,starts_at=excluded.starts_at,
          ends_at=excluded.ends_at,version=excluded.version,reason=excluded.reason,evidence=excluded.evidence,updated_at=excluded.updated_at`)
          .bind(g.companyId, g.packageId, g.packageVersion, g.state, g.startsAt, g.endsAt, g.version, g.reason, g.evidence, instant).run();
        await database.prepare(`INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,?,?,?)`)
          .bind(input.idempotencyKey, g.companyId, g.packageId, actorId, hash, current ? JSON.stringify(current) : null, JSON.stringify(g), instant).run();
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
          await database.prepare("INSERT INTO company_package_revisions(company_id,version) VALUES(?,0) ON CONFLICT DO NOTHING").bind(companyId).run();
          await database.prepare("SELECT version FROM company_package_revisions WHERE company_id=? FOR UPDATE").bind(companyId).first();
        }
        return readCompanyPackageGrants(database, companyId, locking);
      },
      revision: packageGrantRevision,
      previewDependencies: async (grant, prospective) => validatePackageDependencies(grant, prospective, now()),
      receipt: async key => {
        if (await database.prepare("SELECT 1 FROM company_package_grant_events WHERE id=?").bind(key).first()) throw new Error("IDEMPOTENCY_CONFLICT");
        const row = await database.prepare("SELECT request_hash,result_json FROM company_package_change_sets WHERE id=?").bind(key)
          .first<{ request_hash: string; result_json: PackageGrantBatchPreview | string }>();
        return row ? { hash: row.request_hash, result: typeof row.result_json === "string" ? JSON.parse(row.result_json) as PackageGrantBatchPreview : row.result_json } : null;
      },
      saveReceipt: async (key, digest, actorId, result) => {
        await database.prepare("INSERT INTO company_package_change_sets(id,company_id,actor_id,request_hash,result_json,created_at) VALUES(?,?,?,?,?,?)")
          .bind(key, result.companyId, actorId, digest, JSON.stringify(result), now()).run();
      },
    };
  }
  const batch = packageGrantBatchCommands({ unit: () => batchUnit(db), transaction: action => db.transaction(async tx => { await packageWriterFence(tx).run(); await ports.beforeTransaction?.(tx); return action(batchUnit(tx, true)); }), hash });
  return { ...commands, batch: { ...batch,
    /** Apply through normal Commercial policy/audit in an owner's existing unit.
     * The owner can validate its source lease after the receipt, before COMMIT. */
    applyIn(transaction: SqlDatabase, input: PackageGrantBatch, verifyPreview: (preview: PackageGrantBatchPreview) => Promise<void>) {
      db.assertActiveTransaction(transaction);
      return packageGrantBatchCommands({ unit: () => batchUnit(transaction), hash,
        transaction: async action => { await packageWriterFence(transaction).run(); await ports.beforeTransaction?.(transaction); return action(batchUnit(transaction, true)); },
      }).apply(input, verifyPreview);
    },
    /** Read-only preview on the owner's existing snapshot. It neither checks
     * out another pool connection nor inserts a revision anchor. */
    previewIn(transaction: SqlDatabase, input: PackageGrantBatch) {
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
  return (await database.prepare(`SELECT * FROM company_package_grants WHERE company_id=? ORDER BY package_id${forShare ? " FOR SHARE" : ""}`).bind(companyId).all<Row>()).results.map(grantFromRow);
}

/** Optimistic Company revision binds every persisted grant, including unchanged packages. */
export function packageGrantRevision(companyId: string, grants: readonly CompanyPackageGrant[]) {
  return createHash("sha256").update(JSON.stringify({ companyId, grants: grants.filter(grant => grant.companyId === companyId)
    .map(canonicalPackageGrant).sort((a, b) => a.packageId.localeCompare(b.packageId)) })).digest("hex");
}
