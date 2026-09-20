import { signingAuthority } from "./authority";
import "server-only";
import { z } from "zod";
import { currentSession } from "../authorization";
import { holdingBindingBySlug, holdingResources } from "../runtime";
import type { SqlDatabase } from "../runtime/types";
import { PostgresDatabase } from "../runtime/postgres";
import { requireFreshMfa } from "../iam-mfa";
import { fail, lock } from "./access";
import { hash } from "./service";
import { documentBytes } from "./documents";
import type { DocumentVersion } from "@/lib/legal/model";
export type PortalActor = { userId: string; email: string; sessionId: string };
export async function portalContext(slug: string) {
  const { session } = await currentSession();
  if (!session.user.emailVerified)
    fail(
      "Verify your account email before opening shared legal documents.",
      "LEGAL_EMAIL_UNVERIFIED",
      403,
    );
  const binding = await holdingBindingBySlug(slug),
    resources = await holdingResources(binding.id);
  return {
    ...resources,
    actor: {
      userId: session.user.id,
      email: session.user.email.toLowerCase(),
      sessionId: session.session.id,
    } satisfies PortalActor,
  };
}
export async function portalList(db: SqlDatabase, a: PortalActor) {
  return (
    await db
      .prepare(
        "SELECT s.id,s.document_id,s.expires_at,s.user_id,d.name,d.version,d.checksum FROM legal_shares s JOIN legal_documents d ON d.id=s.document_id WHERE lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>? ORDER BY s.created_at DESC LIMIT 100",
      )
      .bind(a.email, a.userId, Date.now())
      .all()
  ).results;
}
async function share(db: SqlDatabase, a: PortalActor, id: string) {
  const s = await db
    .prepare(
      "SELECT s.*,i.company_id,i.kind FROM legal_shares s JOIN legal_items i ON i.id=s.item_id WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>?",
    )
    .bind(id, a.email, a.userId, Date.now())
    .first<{
      id: string;
      item_id: string;
      document_id: string;
      company_id: string;
      kind: string;
      version: number;
    }>();
  if (!s)
    fail(
      "This share expired or is unavailable.",
      "LEGAL_SHARE_UNAVAILABLE",
      404,
    );
  if (
    await db
      .prepare(
        "SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed')",
      )
      .bind(s.item_id)
      .first()
  )
    fail("Document retention is in progress.", "LEGAL_RETENTION_FROZEN", 409);
  return s;
}
export async function portalDetail(
  db: SqlDatabase,
  a: PortalActor,
  id: string,
) {
  const s = await share(db, a, id),
    doc = await db
      .prepare(
        "SELECT id,name,body,version,checksum,media_type FROM legal_documents WHERE id=?",
      )
      .bind(s.document_id)
      .first();
  const comments = (
    await db
      .prepare(
        "SELECT body,created_at,created_by FROM legal_comments WHERE item_id=? AND document_id=? AND visibility='external' ORDER BY created_at LIMIT 100",
      )
      .bind(s.item_id, s.document_id)
      .all()
  ).results;
  const envelopes = (
    await db
      .prepare(
        "SELECT e.id,e.state,e.consent,e.expires_at,s.state signer_state,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.item_id=? AND e.document_id=? AND s.user_id=? ORDER BY e.created_at DESC LIMIT 10",
      )
      .bind(s.item_id, s.document_id, a.userId)
      .all()
  ).results;
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(
      crypto.randomUUID(),
      s.item_id,
      s.document_id,
      a.userId,
      "portal.view",
      Date.now(),
    )
    .run();
  const billing =
    s.kind === "counsel"
      ? (
          await db
            .prepare(
              "SELECT id,title,state,data,version FROM legal_counsel_entries WHERE item_id=? AND created_by=? ORDER BY updated_at DESC LIMIT 100",
            )
            .bind(s.item_id, a.userId)
            .all()
        ).results.map((r) => ({
          ...r,
          data: typeof r.data === "string" ? JSON.parse(r.data) : r.data,
        }))
      : [];
  return { shareId: id, doc, comments, envelopes, kind: s.kind, billing };
}
export async function portalDownload(
  db: SqlDatabase,
  files: Parameters<typeof documentBytes>[0],
  a: PortalActor,
  id: string,
) {
  const s = await share(db, a, id),
    d = await db
      .prepare("SELECT * FROM legal_documents WHERE id=?")
      .bind(s.document_id)
      .first<DocumentVersion>();
  if (!d) fail("Document unavailable.", "LEGAL_NOT_FOUND", 404);
  const bytes = await documentBytes(files, d);
  await share(db, a, id);
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(
      crypto.randomUUID(),
      s.item_id,
      d.id,
      a.userId,
      "portal.download",
      Date.now(),
    )
    .run();
  return new Response(new Uint8Array(bytes), {
    headers: {
      "Content-Type": d.media_type,
      "Content-Disposition": `attachment; filename="document"; filename*=UTF-8''${encodeURIComponent(d.name)}`,
      "Cache-Control": "private, no-store",
      "X-Content-Type-Options": "nosniff",
    },
  });
}
export async function portalCommand(
  db: PostgresDatabase,
  a: PortalActor,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      shareId: z.string(),
      action: z.enum([
        "accept",
        "comment",
        "view",
        "sign",
        "decline",
        "billing.submit",
      ]),
      envelopeId: z.string().optional(),
      body: z.string().min(1).max(20000).optional(),
      consent: z.string().optional(),
    })
    .parse(raw);
  z.string().uuid().parse(key);
  if (["sign", "decline"].includes(x.action)) await requireFreshMfa(a);
  const digest = hash(JSON.stringify({ user: a.userId, input: raw }));
  return db.transaction(async (tx) => {
    const s = await share(tx, a, x.shareId);
    await lock(tx, s.company_id).run();
    await share(tx, a, x.shareId);
    const prior = await tx
      .prepare("SELECT * FROM legal_portal_receipts WHERE id=?")
      .bind(key)
      .first<{ user_id: string; request_hash: string; result_json: string }>();
    if (prior) {
      if (prior.user_id !== a.userId || prior.request_hash !== digest)
        fail(
          "This command key belongs to another request.",
          "IDEMPOTENCY_CONFLICT",
          409,
        );
      return JSON.parse(prior.result_json);
    }
    if (
      !(await tx
        .prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled")
        .bind(s.company_id)
        .first())
    )
      fail("Legal is currently read-only.");
    if (x.action === "accept")
      await tx
        .prepare(
          "UPDATE legal_shares SET user_id=?,version=version+1 WHERE id=?",
        )
        .bind(a.userId, s.id)
        .run();
    else if (x.action === "comment") {
      if (!x.body) fail("Enter a comment.");
      await tx
        .prepare(
          "INSERT INTO legal_comments(id,item_id,document_id,body,visibility,created_by,created_at) VALUES(?,?,?,?,'external',?,?)",
        )
        .bind(
          crypto.randomUUID(),
          s.item_id,
          s.document_id,
          x.body,
          a.userId,
          Date.now(),
        )
        .run();
    } else if (x.action === "billing.submit") {
      if (s.kind !== "counsel")
        fail("Billing submissions require a counsel engagement share.");
      const p = z
        .object({
          entryType: z.enum(["time", "fee", "invoice"]),
          description: z.string().trim().min(3).max(12000),
          minutes: z.coerce.number().int().min(0).max(100000),
          amount: z.coerce.number().int().min(0).max(Number.MAX_SAFE_INTEGER),
          evidence: z.string().trim().min(3).max(12000),
        })
        .parse(raw);
      if (p.entryType === "time" && p.amount !== 0)
        fail("Submit charges separately as a fee or invoice.");
      const entry = {
        id: key,
        item_id: s.item_id,
        category: "counsel_entry",
        title: p.description.slice(0, 200),
        owner_id: a.userId,
        due_date: null,
        state: "submitted",
        version: 1,
        data: {
          entryType: p.entryType,
          description: p.description,
          minutes: String(p.minutes),
          amount: String(p.amount),
          evidence: p.evidence,
        },
        created_by: a.userId,
        updated_by: a.userId,
        updated_at: Date.now(),
      };
      await tx
        .prepare(
          "INSERT INTO legal_counsel_entries(id,item_id,category,title,owner_id,state,version,data,created_by,updated_by,updated_at) VALUES(?,?,'counsel_entry',?,?,'submitted',1,?::jsonb,?,?,?)",
        )
        .bind(
          key,
          s.item_id,
          entry.title,
          a.userId,
          JSON.stringify(entry.data),
          a.userId,
          a.userId,
          Date.now(),
        )
        .run();
      await tx
        .prepare(
          "INSERT INTO legal_feature_history(id,item_id,feature_id,category,version,snapshot,actor_id,reason,created_at) VALUES(?,?,?,'counsel_entry',1,?::jsonb,?,'External counsel submission',?)",
        )
        .bind(
          crypto.randomUUID(),
          s.item_id,
          key,
          JSON.stringify(entry),
          a.userId,
          Date.now(),
        )
        .run();
      await tx
        .prepare(
          "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), s.item_id)
        .run();
    } else {
      const e = await tx
        .prepare(
          "SELECT e.*,s.ordinal,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.id=? AND e.item_id=? AND e.document_id=? AND s.user_id=? AND e.state='sent' AND s.state='pending' AND e.expires_at>?",
        )
        .bind(x.envelopeId, s.item_id, s.document_id, a.userId, Date.now())
        .first<{
          id: string;
          ordinal: number;
          viewed_at: number | null;
          consent: string;
          document_hash: string;
        }>();
      if (!e)
        fail("Envelope is no longer actionable.", "LEGAL_ENVELOPE_STATE", 409);
      if (x.action === "view")
        await tx
          .prepare(
            "UPDATE legal_signers SET viewed_at=? WHERE envelope_id=? AND user_id=?",
          )
          .bind(Date.now(), e.id, a.userId)
          .run();
      else if (x.action === "decline") {
        await tx
          .prepare(
            "UPDATE legal_signers SET state='declined' WHERE envelope_id=? AND user_id=?",
          )
          .bind(e.id, a.userId)
          .run();
        await tx
          .prepare("UPDATE legal_envelopes SET state='declined' WHERE id=?")
          .bind(e.id)
          .run();
        await tx
          .prepare(
            "UPDATE legal_items SET state='rejected',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), s.item_id)
          .run();
      } else {
        await signingAuthority(tx, s.item_id, a.userId);
        if (!e.viewed_at || x.consent !== e.consent)
          fail("Review the exact document and consent before signing.");
        if (
          await tx
            .prepare(
              "SELECT 1 FROM legal_signers WHERE envelope_id=? AND ordinal<? AND state!='signed'",
            )
            .bind(e.id, e.ordinal)
            .first()
        )
          fail("An earlier signer must sign first.");
        if (
          !(await tx
            .prepare(
              "SELECT 1 FROM legal_documents WHERE id=? AND checksum=? AND locked",
            )
            .bind(s.document_id, e.document_hash)
            .first())
        )
          fail("Document integrity check failed.");
        await tx
          .prepare(
            "UPDATE legal_signers SET state='signed',signed_at=?,evidence=coalesce(evidence,'{}'::jsonb)||?::jsonb WHERE envelope_id=? AND user_id=?",
          )
          .bind(
            Date.now(),
            JSON.stringify({
              actorId: a.userId,
              email: a.email,
              sessionId: a.sessionId,
              method: "verified-email+fresh-totp",
              consent: e.consent,
              documentHash: e.document_hash,
              viewedAt: e.viewed_at,
              signedAt: Date.now(),
            }),
            e.id,
            a.userId,
          )
          .run();
        const pending = await tx
          .prepare(
            "SELECT 1 FROM legal_signers WHERE envelope_id=? AND state!='signed'",
          )
          .bind(e.id)
          .first();
        if (!pending) {
          await tx
            .prepare("UPDATE legal_envelopes SET state='completed' WHERE id=?")
            .bind(e.id)
            .run();
          await tx
            .prepare(
              "UPDATE legal_items SET state='executed',version=version+1,updated_at=? WHERE id=?",
            )
            .bind(Date.now(), s.item_id)
            .run();
        }
      }
    }
    await tx
      .prepare(
        "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,?,?,?,version,? FROM legal_items WHERE id=?",
      )
      .bind(
        crypto.randomUUID(),
        `portal.${x.action}`,
        a.userId,
        s.document_id,
        Date.now(),
        s.item_id,
      )
      .run();
    await tx
      .prepare("INSERT INTO legal_portal_receipts VALUES(?,?,?,?,?)")
      .bind(key, a.userId, digest, JSON.stringify({ id: s.id }), Date.now())
      .run();
    await portalOutbox(tx, a, s.item_id, key, `portal.${x.action}`);
    return { id: s.id };
  });
}

export async function portalUpload(
  db: PostgresDatabase,
  files: Parameters<typeof documentBytes>[0],
  a: PortalActor,
  shareId: string,
  name: string,
  mediaType: string,
  bytes: Uint8Array,
  key: string,
) {
  z.string().uuid().parse(key);
  z.string().min(1).max(200).parse(name);
  if (!bytes.length || bytes.length > 10 * 1024 * 1024)
    fail("Upload between 1 byte and 10 MB.");
  if (
    !(
      mediaType === "application/pdf" &&
      Buffer.from(bytes.subarray(0, 5)).toString() === "%PDF-"
    ) &&
    !(
      mediaType ===
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" &&
      bytes[0] === 80 &&
      bytes[1] === 75
    )
  )
    fail("Upload a PDF or DOCX.");
  const s = await share(db, a, shareId),
    checksum = hash(bytes),
    objectKey = `legal/${s.company_id}/${s.item_id}/external/${key}/${checksum}`,
    digest = hash(
      JSON.stringify({ userId: a.userId, shareId, name, mediaType, checksum }),
    );
  await files.put(objectKey, bytes, {
    onlyIf: { etagDoesNotMatch: "*" },
    httpMetadata: { contentType: mediaType },
    customMetadata: { legalItem: s.item_id },
  });
  return db.transaction(async (tx) => {
    await lock(tx, s.company_id).run();
    await share(tx, a, shareId);
    if (
      !(await tx
        .prepare("SELECT 1 FROM legal_settings WHERE company_id=? AND enabled")
        .bind(s.company_id)
        .first())
    )
      fail("Legal is read-only.");
    const prior = await tx
      .prepare(
        "SELECT request_hash,result_json FROM legal_portal_receipts WHERE id=? AND user_id=?",
      )
      .bind(key, a.userId)
      .first<{ request_hash: string; result_json: string }>();
    if (prior) {
      if (prior.request_hash !== digest) fail("Upload key conflict.");
      return JSON.parse(prior.result_json);
    }
    await tx
      .prepare("INSERT INTO legal_external_uploads VALUES(?,?,?,?,?,?,?,?,?)")
      .bind(
        key,
        shareId,
        objectKey,
        checksum,
        name,
        mediaType,
        bytes.length,
        a.userId,
        Date.now(),
      )
      .run();
    await tx
      .prepare(
        "INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) SELECT ?,id,'portal.upload',?,?,version,? FROM legal_items WHERE id=?",
      )
      .bind(
        crypto.randomUUID(),
        a.userId,
        `${name} · ${checksum}`,
        Date.now(),
        s.item_id,
      )
      .run();
    await tx
      .prepare("INSERT INTO legal_portal_receipts VALUES(?,?,?,?,?)")
      .bind(key, a.userId, digest, JSON.stringify({ id: key }), Date.now())
      .run();
    await portalOutbox(tx, a, s.item_id, key, "portal.upload");
    return { id: key };
  });
}

async function portalOutbox(
  db: SqlDatabase,
  a: PortalActor,
  itemId: string,
  key: string,
  action: string,
) {
  const item = await db
    .prepare(
      "SELECT company_id,branch_id,department_id,version FROM legal_items WHERE id=?",
    )
    .bind(itemId)
    .first<{
      company_id: string;
      branch_id: string | null;
      department_id: string | null;
      version: number;
    }>();
  if (!item) fail("Record unavailable.");
  const authority = JSON.stringify({
    kind: "external-share",
    userId: a.userId,
    sessionId: a.sessionId,
  });
  await db
    .prepare(
      "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(
      crypto.randomUUID(),
      key,
      a.userId,
      "external-share",
      action,
      "legal",
      itemId,
      item.version,
      "Shared document interaction",
      Date.now(),
      authority,
    )
    .run();
  await db
    .prepare(
      "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(
      crypto.randomUUID(),
      key,
      `legal.${action}`,
      "legal",
      itemId,
      item.version,
      JSON.stringify({
        id: itemId,
        companyId: item.company_id,
        branchId: item.branch_id,
        departmentId: item.department_id,
        version: item.version,
      }),
      Date.now(),
    )
    .run();
}
