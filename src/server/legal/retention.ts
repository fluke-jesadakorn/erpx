import "server-only";
import {legalDestructionManifest, validateLegalDestructionSources, sealLegalDestructionSources, type LegalDestructionManifest} from "./destruction-sources";
import { z } from "zod";
import type { SqlDatabase, FileStore, SqlStatement } from "../runtime/types";
import type { PostgresDatabase } from "../runtime/postgres";
import type { WorkContext } from "../authorization";
import { requirePermission } from "../iam-permissions";
import { samePerson } from "@/lib/iam";
import { executeCommand, replayCommand, assertion } from "../commands";
import { access, guards, event, fail, lock } from "./access";
export async function retentionView(
  db: SqlDatabase,
  a: WorkContext,
  id?: string,
) {
  requirePermission(a, "legal.read");
  if (id) await access(db, a, id);
  const policies = (
    await db
      .prepare(
        "SELECT * FROM legal_retention_policies WHERE company_id=? ORDER BY created_at DESC",
      )
      .bind(a.companyId)
      .all()
  ).results;
  const assignment = id
    ? await db
        .prepare("SELECT * FROM legal_retention_assignments WHERE item_id=?")
        .bind(id)
        .first()
    : null;
  const requests = id
    ? (
        await db
          .prepare(
            "SELECT * FROM legal_destructions WHERE item_id=? ORDER BY created_at DESC",
          )
          .bind(id)
          .all()
      ).results
    : [];
  return { policies, assignment, requests };
}
export async function retentionCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum([
        "retention.policy",
        "retention.approve-policy",
        "retention.retire-policy",
        "retention.assign",
        "retention.request",
        "retention.approve",
        "retention.cancel",
        "retention.retry",
      ]),
      id: z.string().optional(),
      expectedVersion: z.number().int().positive().optional(),
      policyId: z.string().optional(),
      policyVersion: z.number().int().positive().optional(),
      requestId: z.string().optional(),
      confirmation: z.string().optional(),
      reason: z.string().trim().min(3).max(4000),
    })
    .parse(raw);
  if (
    x.action.startsWith("retention.") &&
    [
      "retention.policy",
      "retention.approve-policy",
      "retention.retire-policy",
    ].includes(x.action)
  ) {
    requirePermission(a, "legal.manage");
    if (a.branchId || a.departmentId)
      fail("Manage retention policies at Company scope.");
    const replay = await replayCommand(db, a, key, x.action, a.companyId, raw);
    if (replay) return replay;
    const steps: SqlStatement[] = [lock(db, a.companyId)];
    if (x.action === "retention.policy") {
      const p = z
        .object({
          name: z.string().trim().min(3).max(200),
          days: z.coerce.number().int().min(1).max(36500),
        })
        .parse(raw);
      steps.push(
        db
          .prepare(
            "INSERT INTO legal_retention_policies(id,company_id,name,days,basis,state,created_by,created_at) VALUES(?,?,?,?,?,'draft',?,?)",
          )
          .bind(
            key,
            a.companyId,
            p.name,
            p.days,
            x.reason,
            a.userId,
            Date.now(),
          ),
      );
    } else {
      const p = await db
        .prepare(
          "SELECT * FROM legal_retention_policies WHERE id=? AND company_id=?",
        )
        .bind(x.policyId, a.companyId)
        .first<{
          id: string;
          created_by: string;
          state: string;
          version: number;
        }>();
      if (!p || p.version !== x.policyVersion) fail("Policy changed. Refresh.");
      if (
        x.action === "retention.approve-policy" &&
        (p.state !== "draft" || samePerson(a, p.created_by))
      )
        fail(
          "An independent manager must approve the policy.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(
        assertion(
          db,
          key,
          "retention-policy-version",
          "EXISTS(SELECT 1 FROM legal_retention_policies WHERE id=? AND version=?)",
          [p.id, p.version],
        ),
        db
          .prepare(
            "UPDATE legal_retention_policies SET state=?,approved_by=?,version=version+1 WHERE id=?",
          )
          .bind(
            x.action === "retention.approve-policy" ? "approved" : "retired",
            a.userId,
            p.id,
          ),
      );
    }
    return executeCommand(
      db,
      a,
      key,
      x.action,
      raw,
      a.companyId,
      1,
      { id: a.companyId, version: 1 },
      steps,
    );
  }
  const item = await access(db, a, z.string().parse(x.id), "legal.destroy");
  const replay = await replayCommand(db, a, key, x.action, item.id, raw);
  if (replay) return replay;
  const steps = guards(
    db,
    a,
    item.id,
    z.number().int().positive().parse(x.expectedVersion),
    key,
    false,
    x.action === "retention.retry",
  );
  if (x.action === "retention.assign") {
    if (
      !["archived", "closed", "completed", "expired", "terminated"].includes(
        item.state,
      )
    )
      fail("Close or archive the record before assigning retention.");
    const policy = await db
      .prepare(
        "SELECT * FROM legal_retention_policies WHERE id=? AND company_id=? AND state='approved'",
      )
      .bind(x.policyId, a.companyId)
      .first<{ id: string; days: number; version: number }>();
    if (!policy) fail("Select an approved Company policy.");
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_retention_assignments(item_id,policy_id,policy_version,eligible_at,assigned_by,created_at) VALUES(?,?,?,(extract(epoch FROM (((to_timestamp(?::double precision/1000) AT TIME ZONE ?)::date + ?::int)::timestamp AT TIME ZONE ?))*1000)::bigint,?,?) ON CONFLICT(item_id) DO UPDATE SET policy_id=excluded.policy_id,policy_version=excluded.policy_version,eligible_at=excluded.eligible_at,assigned_by=excluded.assigned_by",
        )
        .bind(
          item.id,
          policy.id,
          policy.version,
          item.updated_at,
          item.timezone,
          policy.days,
          item.timezone,
          a.userId,
          Date.now(),
        ),
    );
  } else if (x.action === "retention.request") {
    const assignment = await db
      .prepare(
        "SELECT r.* FROM legal_retention_assignments r JOIN legal_retention_policies p ON p.id=r.policy_id AND p.version=r.policy_version AND p.state='approved' WHERE r.item_id=? AND r.eligible_at<=?",
      )
      .bind(item.id, Date.now())
      .first<{ policy_id: string; policy_version: number }>();
    if (!assignment)
      fail("Retention has not elapsed under the currently approved policy.");
    if (
      !["archived", "closed", "completed", "expired", "terminated"].includes(
        item.state,
      )
    )
      fail("This record is still active.");
    steps.push(noHold(db, item.id, key));
    const manifest = await legalDestructionManifest(db,item.id,item.company_id);
    steps.push(
      db
        .prepare(
          "INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) VALUES(?,?,?,?,?,'requested',?,?::jsonb,?)",
        )
        .bind(
          key,
          item.id,
          assignment.policy_id,
          assignment.policy_version,
          a.userId,
          x.reason,
          JSON.stringify(manifest),
          Date.now(),
        ),
    );
  } else {
    const r = await db
      .prepare("SELECT * FROM legal_destructions WHERE id=? AND item_id=?")
      .bind(x.requestId, item.id)
      .first<{
        id: string;
        state: string;
        requested_by: string;
        approved_by: string | null;
      }>();
    if (!r) fail("Destruction request unavailable.");
    let state: string;
    if (x.action === "retention.approve") {
      if (x.confirmation !== r.id)
        fail("Confirm the exact destruction request ID.");
      if (r.state !== "requested" || samePerson(a, r.requested_by))
        fail(
          "An independent manager must authorize destruction.",
          "LEGAL_INDEPENDENT_REVIEW",
          403,
        );
      steps.push(noHold(db, item.id, key));
      state = "approved";
    } else if (x.action === "retention.retry") {
      if (!["failed", "blocked", "executing"].includes(r.state))
        fail("Only blocked or failed work can be retried.");
      steps.push(noHold(db, item.id, key));
      state = r.state === "executing" ? "executing" : "requested";
    } else {
      if (["completed", "executing"].includes(r.state))
        fail("This destruction cannot be cancelled.");
      state = "cancelled";
    }
    steps.push(
      db
        .prepare(
          "UPDATE legal_destructions SET state=?,approved_by=coalesce(?,approved_by),approved_at=coalesce(?,approved_at),version=version+1,error=NULL,attempts=0 WHERE id=?",
        )
        .bind(
          state,
          state === "approved" ? a.userId : null,
          state === "approved" ? Date.now() : null,
          r.id,
        ),
    );
  }
  steps.push(
    db
      .prepare(
        "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
      )
      .bind(Date.now(), item.id),
    event(db, a, item.id, x.action, x.reason, item.version + 1),
  );
  return executeCommand(
    db,
    a,
    key,
    x.action,
    raw,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1 },
    steps,
  );
}
function noHold(db: SqlDatabase, id: string, key: string) {
  return assertion(
    db,
    key,
    "retention-no-hold",
    "NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state!='released')",
    [id],
  );
}
export async function drainLegalDestruction(
  db: PostgresDatabase,
  files: FileStore,
) {
  const candidate = await db
    .prepare(
      "SELECT id FROM legal_destructions WHERE state IN ('approved','executing') AND attempts<6 ORDER BY created_at LIMIT 1",
    )
    .first<{ id: string }>();
  if (!candidate) return;
  try {
    // Commit the execution journal before touching filesystem bytes. Recovery
    // resumes this same manifest after a crash; the item remains frozen.
    await db.transaction(async (tx) => {
      await tx
        .prepare(
          "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
        )
        .run();
      const r = await tx
        .prepare(
          "SELECT x.*,i.company_id FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='approved' FOR UPDATE OF x,i",
        )
        .bind(candidate.id)
        .first<{
          id: string;
          item_id: string;
          company_id: string;
          policy_id: string;
          policy_version: number;
        }>();
      if (!r) return;
      await lock(tx, r.company_id).run();
      const eligible = await tx
        .prepare(
          "SELECT 1 FROM legal_retention_assignments a JOIN legal_retention_policies p ON p.id=a.policy_id JOIN legal_items i ON i.id=a.item_id WHERE a.item_id=? AND a.policy_id=? AND a.policy_version=? AND p.version=a.policy_version AND p.state='approved' AND a.eligible_at<=? AND i.state IN ('archived','closed','completed','expired','terminated') AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=i.id AND h.state!='released')",
        )
        .bind(r.item_id, r.policy_id, r.policy_version, Date.now())
        .first();
      if (!eligible) {
        await tx
          .prepare(
            "UPDATE legal_destructions SET state='blocked',error='HOLD_OR_RETENTION_POLICY_CHANGED' WHERE id=?",
          )
          .bind(r.id)
          .run();
        return;
      }
      await tx
        .prepare(
          "UPDATE legal_destructions SET state='executing',started_at=? WHERE id=?",
        )
        .bind(Date.now(), r.id)
        .run();
      await tx
        .prepare(
          "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'retention.started','system:legal',?,version,? FROM legal_items WHERE id=?",
        )
        .bind(
          crypto.randomUUID(),
          `Execution journal ${r.id}`,
          Date.now(),
          r.item_id,
        )
        .run();
    });
    await db.transaction(async (tx) => {
      await tx
        .prepare(
          "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
        )
        .run();
      const r = await tx
        .prepare(
          "SELECT x.*,i.company_id,i.version item_version FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id WHERE x.id=? AND x.state='executing' FOR UPDATE OF x,i",
        )
        .bind(candidate.id)
        .first<{
          id: string;
          item_id: string;
          company_id: string;
          item_version: number;
          policy_id: string;
          policy_version: number;
          manifest: string;
        }>();
      if (!r) return;
      await lock(tx, r.company_id).run();
      const held = await tx
        .prepare(
          "SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state!='released'",
        )
        .bind(r.item_id)
        .first();
      if (held) throw new Error("LEGAL_HOLD");
      await tx
        .prepare("UPDATE legal_destructions SET attempts=attempts+1 WHERE id=?")
        .bind(r.id)
        .run();
      if (!files.destroyPreserved)
        throw new Error("LEGAL_DESTRUCTION_STORE_UNAVAILABLE");
      const manifest = JSON.parse(r.manifest) as LegalDestructionManifest;
      await validateLegalDestructionSources(tx,r.item_id,r.company_id,manifest);
      await tx
        .prepare("UPDATE legal_destructions SET state='executing' WHERE id=?")
        .bind(r.id)
        .run();
      await files.destroyPreserved(manifest.keys, {
        database: tx,
        destructionId: r.id,
      });
      await sealLegalDestructionSources(tx,r.company_id,r.id,manifest);
      for (const d of manifest.documents) {
        await tx
          .prepare(
            "INSERT INTO legal_disposals VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
          )
          .bind(d.id, r.id, d.checksum, Date.now())
          .run();
        await tx
          .prepare(
            "UPDATE legal_documents SET body='',object_key=NULL WHERE id=?",
          )
          .bind(d.id)
          .run();
      }
      await tx
        .prepare(
          "UPDATE legal_processing_jobs SET state='failed',result_json=NULL,error='CONTENT_DESTROYED',lease_token=NULL,lease_until=NULL WHERE item_id=?",
        )
        .bind(r.item_id)
        .run();
      await tx
        .prepare(
          "DELETE FROM legal_document_inspections WHERE document_id IN (SELECT id FROM legal_documents WHERE item_id=?)",
        )
        .bind(r.item_id)
        .run();
      await tx
        .prepare(
          "UPDATE legal_ai_proposals SET proposal=NULL,question='[disposed under retention policy]',state='disposed' WHERE item_id=?",
        )
        .bind(r.item_id)
        .run();
      await tx
        .prepare(
          "UPDATE legal_shares SET revoked_at=? WHERE item_id=? AND revoked_at IS NULL",
        )
        .bind(Date.now(), r.item_id)
        .run();
      await tx
        .prepare(
          "UPDATE legal_destructions SET state='completed',completed_at=?,error=NULL,version=version+1 WHERE id=?",
        )
        .bind(Date.now(), r.id)
        .run();
      await tx
        .prepare(
          "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,'retention.destroyed','system:legal',?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          r.item_id,
          `Destruction certificate ${r.id}; checksums retained`,
          r.item_version,
          Date.now(),
        )
        .run();
    });
  } catch {
    await db
      .prepare(
        "UPDATE legal_destructions SET state=CASE WHEN started_at IS NULL THEN 'failed' ELSE 'executing' END,error='DESTRUCTION_REQUIRES_REVIEW',attempts=attempts+1 WHERE id=? AND state!='completed'",
      )
      .bind(candidate.id)
      .run();
  }
}
