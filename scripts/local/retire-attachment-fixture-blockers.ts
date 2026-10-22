/** One-off, explicitly approved reset of the 2026-09-28 synthetic dev blockers.
 * This is NOT a production attachment-destruction path. It preserves source,
 * link, scan, ledger, and audit rows; full database dumps and verified object
 * bytes are archived before any active object is removed.
 */
import "server-only";

import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { Pool } from "pg";
import { getRuntime, closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { resourceSlots, registeredResources } from "@erpx/feature-runtime/lib/holding-registry";
import { backfillLegacyAttachments, backfillSpecializedAttachments, reconcileAttachmentStorage } from "@erpx/server-files/public/attachments/migration";
import { billingEvidenceStore, reconcileBillingEvidenceStorage } from "@erpx/server-billing/public/billing-attachments";
import { attachmentUploadsFrozen, attachmentsV2Enabled } from "@erpx/server-files/public/attachments/feature";
import type { FileStore } from "@erpx/server-runtime/adapters/runtime/types";

const exec = promisify(execFile);
const apply = process.argv.includes("--apply");
const approval = "2026-09-28 user approval: retire the 133 synthetic dev reconciliation blockers";
const expected = { h001: 92, h002: 15, h003: 0, billingOrphans: 23 } as const;
const sha = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
type Source = { id: string; owner_type: string; owner_id: string; purpose: string; media_type: string; object_key: string; preview_object_key: string | null; bytes: number; checksum_sha256: string; source_state: string; company_id: string | null; legal_hold: number };
type Target = { holdingId: string; source: Source; companyId: string; store: FileStore };
type ObjectTarget = { scope: string; key: string; store: FileStore; expectedSha?: string; expectedBytes?: number };

const env = getRuntime();
const pools: Pool[] = [];
try {
  if (env.ERPX_ENVIRONMENT !== "local" || process.env.NODE_ENV === "production" || resourceSlots(env).some(slot => slot.beta)) throw new Error("LOCAL_NON_BETA_ONLY");
  if (!resolve(env.ERPX_DATA_DIR).endsWith("/erpx/.runtime/local-test")) throw new Error("UNEXPECTED_DEV_DATA_ROOT");
  if (attachmentsV2Enabled() || !attachmentUploadsFrozen()) throw new Error("CLOSED_GATE_AND_FROZEN_UPLOADS_REQUIRED");
  const manifestPath = process.env.ERPX_MIGRATION_DATABASE_MANIFEST ?? join(dirname(process.env.ERPX_DATABASE_MANIFEST!), "databases-migrations.json");
  const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as { databases: Record<string, string> };
  const keys = ["CONTROL_DB", "H001_DB", "H002_DB", "H003_DB"];
  for (const key of keys) if (new URL(manifest.databases[key]!).pathname.slice(1) !== `erpx_dev_${key === "CONTROL_DB" ? "control" : `${key.slice(0, 4).toLowerCase()}_write`}`) throw new Error(`UNEXPECTED_DATABASE:${key}`);

  const targets: Target[] = [];
  const unknownCompanies = new Map<string, string>();
  const reports: Record<string, unknown> = {};
  for (const slot of resourceSlots(env)) {
    const { write, files } = registeredResources(env, slot.id);
    const legacy = await backfillLegacyAttachments(write), specialized = await backfillSpecializedAttachments(write);
    const reconciliation = await reconcileAttachmentStorage(write, files, { verifyChecksums: true });
    reports[slot.id] = { legacy, specialized, reconciliation };
    if (reconciliation.unavailable !== expected[slot.id as keyof typeof expected]) throw new Error(`SOURCE_COUNT_DRIFT:${slot.id}`);
    if (specialized.unknown.length || reconciliation.missingObjects.length || reconciliation.sizeMismatches.length || reconciliation.checksumMismatches.length || reconciliation.orphanObjects.length || reconciliation.duplicateLedgerObjects || reconciliation.crossCompanyLinks.length || reconciliation.crossCompanyIntents.length || reconciliation.crossCompanyStorage.length || reconciliation.crossCompanyDerivatives.length || reconciliation.crossCompanyPublications.length) throw new Error(`OTHER_BLOCKERS:${slot.id}`);
    for (const item of legacy.unknown) {
      if (!item.targetCompanyId) throw new Error(`UNKNOWN_COMPANY:${item.id}`);
      unknownCompanies.set(`${slot.id}:${item.id}`, item.targetCompanyId);
    }
    for (const unavailable of reconciliation.unavailableSources) {
      const source = await write.prepare("SELECT id,owner_type,owner_id,purpose,media_type,object_key,preview_object_key,bytes,checksum_sha256,source_state,company_id,legal_hold FROM attachments WHERE id=?").bind(unavailable.id).first<Source>();
      if (!source || source.legal_hold || source.preview_object_key) throw new Error(`UNSAFE_SOURCE:${slot.id}:${unavailable.id}`);
      const group = `${source.owner_type}:${source.source_state}`;
      if (!(["leave_request:quarantined", "staff_task:quarantined", "invoice_payment:scan_error", "commerce_order_delivery:scan_error", "social_draft:scan_error"] as string[]).includes(group)) throw new Error(`UNEXPECTED_SOURCE_GROUP:${group}`);
      if (source.owner_type === "leave_request" && (source.purpose !== "leave evidence" || !source.owner_id.startsWith(`seed-${slot.id}-leave-`))) throw new Error(`NON_FIXTURE_LEAVE:${source.id}`);
      if (source.owner_type === "staff_task" && (source.company_id !== null || !source.media_type?.includes("csv"))) throw new Error(`NON_FIXTURE_TASK:${source.id}`);
      if (source.source_state === "scan_error" && !source.company_id) throw new Error(`SCAN_ERROR_WITHOUT_COMPANY:${source.id}`);
      const companyId = source.company_id ?? unknownCompanies.get(`${slot.id}:${source.id}`);
      if (!companyId) throw new Error(`NO_VERIFIED_COMPANY:${source.id}`);
      targets.push({ holdingId: slot.id, source, companyId, store: files });
    }
  }
  if (targets.length !== 107 || unknownCompanies.size !== 36) throw new Error("FIXTURE_SET_DRIFT");
  const billing = await reconcileBillingEvidenceStorage(env, { verifyChecksums: true });
  if (billing.orphanObjects.length !== expected.billingOrphans || billing.unavailable || billing.missingObjects.length || billing.sizeMismatches.length || billing.checksumMismatches.length || billing.missingLedger.length || billing.quotaDeltaBytes || billing.duplicateLedgerObjects) throw new Error("BILLING_SET_DRIFT");
  if (billing.orphanObjects.some(key => !key.startsWith("billing_api_holding/"))) throw new Error("NON_FIXTURE_BILLING_ORPHAN");
  const billingStore = billingEvidenceStore(env);
  const objects: ObjectTarget[] = [
    ...targets.map(target => ({ scope: target.holdingId, key: target.source.object_key, store: target.store, expectedSha: target.source.checksum_sha256, expectedBytes: Number(target.source.bytes) })),
    ...billing.orphanObjects.map(key => ({ scope: "billing", key, store: billingStore })),
  ];
  if (new Set(objects.map(object => `${object.scope}:${object.key}`)).size !== 130) throw new Error("DUPLICATE_OBJECT_TARGET");
  const runId = `attachment-fixture-retirement-${new Date().toISOString().replace(/[:.]/g, "-")}`;
  const directory = join(process.cwd(), ".scratch/dev", runId);
  const decision = { runId, approval, operator: process.env.USER ?? "local-operator", decidedAt: new Date().toISOString(), sourceCount: targets.length, billingOrphans: billing.orphanObjects.length, sourceReportSha256: sha(JSON.stringify(reports)), targets: targets.map(({ holdingId, source, companyId }) => ({ holdingId, sourceId: source.id, ownerType: source.owner_type, ownerId: source.owner_id, purpose: source.purpose, state: source.source_state, companyId, key: source.object_key, checksum: source.checksum_sha256, bytes: Number(source.bytes) })), billingKeys: billing.orphanObjects };
  if (!apply) {
    process.stdout.write(JSON.stringify({ mode: "plan", runId, sourceCount: targets.length, billingOrphans: billing.orphanObjects.length, sourceReportSha256: decision.sourceReportSha256, groups: Object.fromEntries([...new Set(targets.map(t => `${t.holdingId}:${t.source.owner_type}:${t.source.source_state}`))].map(group => [group, targets.filter(t => `${t.holdingId}:${t.source.owner_type}:${t.source.source_state}` === group).length])) }, null, 2) + "\n");
  } else {
    await mkdir(join(directory, "objects"), { recursive: true, mode: 0o700 });
    await writeFile(join(directory, "decision.json"), JSON.stringify(decision, null, 2), { mode: 0o600, flag: "wx" });
    for (const key of keys) {
      const output = join(directory, `${key.toLowerCase()}.dump`);
      await exec("/opt/homebrew/opt/postgresql@18/bin/pg_dump", ["--format=custom", "--no-owner", "--file", output, manifest.databases[key]!], { maxBuffer: 1024 * 1024 });
    }
    const archived: Array<{ scope: string; key: string; bytes: number; sha256: string; file: string }> = [];
    for (const object of objects) {
      const stored = await object.store.get(object.key);
      if (!stored) throw new Error(`MISSING_BACKUP_OBJECT:${object.scope}:${object.key}`);
      const bytes = new Uint8Array(await stored.arrayBuffer()), digest = sha(bytes);
      if (object.expectedSha && digest !== object.expectedSha || object.expectedBytes !== undefined && bytes.length !== object.expectedBytes) throw new Error(`OBJECT_CONTENT_DRIFT:${object.scope}:${object.key}`);
      const file = `${sha(`${object.scope}\0${object.key}`)}.bin`;
      await writeFile(join(directory, "objects", file), bytes, { mode: 0o600, flag: "wx" });
      archived.push({ scope: object.scope, key: object.key, bytes: bytes.length, sha256: digest, file });
    }
    await writeFile(join(directory, "archived-objects.json"), JSON.stringify(archived, null, 2), { mode: 0o600, flag: "wx" });
    for (const slot of resourceSlots(env)) {
      const localTargets = targets.filter(target => target.holdingId === slot.id);
      if (!localTargets.length) continue;
      const pool = new Pool({ connectionString: manifest.databases[slot.write], max: 1 }); pools.push(pool);
      const client = await pool.connect();
      try {
        await client.query("BEGIN");
        await client.query("LOCK TABLE attachments IN ACCESS EXCLUSIVE MODE");
        await client.query("ALTER TABLE attachments DISABLE TRIGGER attachment_source_immutable");
        await client.query("ALTER TABLE attachments DISABLE TRIGGER attachment_security_destruction_seal");
        for (const { source, companyId } of localTargets) {
          const references = await client.query(`SELECT
            (SELECT count(*)::int FROM attachment_links WHERE attachment_id=$1) links,
            (SELECT count(*)::int FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id WHERE l.attachment_id=$1) publications,
            (SELECT count(*)::int FROM attachment_derivatives WHERE attachment_id=$1) derivatives,
            (SELECT count(*)::int FROM legal_file_preservations WHERE source_type='attachment' AND source_id=$1) preservations`, [source.id]);
          const ref = references.rows[0]!;
          if (Number(ref.publications) || Number(ref.derivatives) || Number(ref.preservations) || Number(ref.links) !== (source.source_state === "scan_error" ? 1 : 0)) throw new Error(`REFERENCE_DRIFT:${source.id}`);
          const changed = await client.query("UPDATE attachments SET company_id=$1,source_state='destroyed',destroyed_at=$2 WHERE id=$3 AND source_state=$4 AND object_key=$5 AND checksum_sha256=$6 AND legal_hold=0 RETURNING id", [companyId, Date.now(), source.id, source.source_state, source.object_key, source.checksum_sha256]);
          if (changed.rowCount !== 1) throw new Error(`SOURCE_DRIFT:${source.id}`);
          await client.query("UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=$1 WHERE attachment_id=$2 AND state='stored'", [Date.now(), source.id]);
          await client.query("INSERT INTO attachment_events(id,company_id,attachment_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)", [`attachment-event-${randomUUID()}`, companyId, source.id, "system:operator", "dev.synthetic_fixture_retired", JSON.stringify({ runId, approval, sourceReportSha256: decision.sourceReportSha256, backup: directory, previousState: source.source_state, objectSha256: source.checksum_sha256 }), Date.now()]);
        }
        await client.query("ALTER TABLE attachments ENABLE TRIGGER attachment_security_destruction_seal");
        await client.query("ALTER TABLE attachments ENABLE TRIGGER attachment_source_immutable");
        await client.query("COMMIT");
      } catch (error) { await client.query("ROLLBACK"); throw error; }
      finally { client.release(); }
    }
    for (const object of objects) await object.store.delete(object.key);
    await writeFile(join(directory, "completed.json"), JSON.stringify({ runId, completedAt: new Date().toISOString(), removedActiveObjects: objects.length, retainedSources: targets.length, backupDirectory: directory }, null, 2), { mode: 0o600, flag: "wx" });
    process.stdout.write(JSON.stringify({ mode: "applied", runId, backupDirectory: directory, retainedSources: targets.length, removedActiveObjects: objects.length }) + "\n");
  }
} finally {
  await Promise.all(pools.map(pool => pool.end()));
  await closeRuntime();
}
