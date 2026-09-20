import "server-only";
import type { SqlDatabase, FileStore } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { executeCommand, assertion } from "../commands";
import { loadIamAuthority } from "../iam-store";
import { access, guards, fail } from "./access";
import { documentAccess, inspectDocument } from "./documents";
export async function queueInspection(
  db: SqlDatabase,
  a: WorkContext,
  documentId: string,
  kind: "ocr" | "verify",
  key: string,
) {
  const d = await documentAccess(db, a, documentId),
    item = await access(db, a, d.item_id);
  return executeCommand(
    db,
    a,
    key,
    `legal.processing.${kind}`,
    { documentId, kind },
    item.id,
    item.version,
    { id: item.id, jobId: key, state: "queued" },
    [
      ...guards(db, a, item.id, item.version, key),
      assertion(
        db,
        key,
        "processing-rate",
        "(SELECT count(*) FROM legal_processing_jobs WHERE company_id=? AND created_at>?)<20",
        [a.companyId, Date.now() - 60000],
      ),
      db
        .prepare(
          "INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at) VALUES(?,?,?,?,?,?,'queued',?,?,?)",
        )
        .bind(
          key,
          a.companyId,
          item.id,
          d.id,
          d.checksum,
          kind,
          JSON.stringify(a),
          a.userId,
          Date.now(),
        ),
    ],
  );
}
export async function processingStatus(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
) {
  const r = await db
    .prepare(
      "SELECT id,item_id,document_id,state,result_json,error,attempts FROM legal_processing_jobs WHERE id=? AND company_id=?",
    )
    .bind(id, a.companyId)
    .first<{
      id: string;
      item_id: string;
      document_id: string;
      state: string;
      result_json: string | null;
      error: string | null;
      attempts: number;
    }>();
  if (!r) fail("Processing job unavailable.", "LEGAL_NOT_FOUND", 404);
  await access(db, a, r.item_id);
  return {
    ...r,
    jobId: r.id,
    result: r.result_json ? JSON.parse(r.result_json) : null,
    result_json: undefined,
  };
}
export async function drainLegalProcessing(db: SqlDatabase, files: FileStore) {
  await db
    .prepare(
      "UPDATE legal_processing_jobs SET state=CASE WHEN attempts<3 THEN 'queued' ELSE 'failed' END,error='PROCESSING_INTERRUPTED',lease_token=NULL,lease_until=NULL WHERE state='running' AND lease_until<?",
    )
    .bind(Date.now())
    .run();
  const token = crypto.randomUUID(),
    j = await db
      .prepare(
        "UPDATE legal_processing_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT id FROM legal_processing_jobs WHERE state='queued' ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *",
      )
      .bind(token, Date.now() + 600000)
      .first<{
        id: string;
        item_id: string;
        document_id: string;
        checksum: string;
        kind: "ocr" | "verify";
        actor_json: string;
      }>();
  if (!j) return;
  try {
    const a = JSON.parse(j.actor_json) as WorkContext;
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?",
        )
        .bind(a.assignmentId, a.userId, a.companyId, Date.now(), Date.now())
        .first())
    )
      fail("Access expired.");
    a.iam = await loadIamAuthority(
      db,
      a.companyId,
      a.userId,
      a.role,
      a.authority?.sessionId,
    );
    const d = await documentAccess(db, a, j.document_id);
    if (d.checksum !== j.checksum) fail("Source changed.");
    const result = await inspectDocument(db, files, a, d.id, j.kind);
    await access(db, a, j.item_id);
    await db
      .prepare(
        "UPDATE legal_processing_jobs SET state='completed',result_json=?,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=? AND state='running'",
      )
      .bind(JSON.stringify(result), j.id, token)
      .run();
  } catch (e) {
    await db
      .prepare(
        "UPDATE legal_processing_jobs SET state='failed',error=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
      )
      .bind(
        e instanceof Error && "code" in e
          ? String(e.code)
          : "DOCUMENT_PROCESSING_FAILED",
        j.id,
        token,
      )
      .run();
  }
}
