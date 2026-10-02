import { signingAuthority } from "./authority";
import "server-only";
import { z } from "zod";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { requireFreshMfa } from "../iam-mfa";
import { runtimeEnv } from "../runtime";
import { executeCommand, assertion, replayCommand } from "../commands";
import { access, guards, event, fail } from "./access";
/** v1 records externally signed originals; sending/signing needs its own release. */
export function assertLegalSigningEnabled(){
  fail("Signing through ERPX is unavailable in this release. Import the signed original for independent review.","LEGAL_SIGNING_DISABLED",409);
}
export async function sendEnvelope(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.literal("signing.send"),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      expiresAt: z.number().int(),
      consent: z.string().trim().min(20).max(10000),
    })
    .parse(raw);
  const item = await access(db, a, x.id, "legal.sign");
  assertLegalSigningEnabled();
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    a,
    key,
    "legal.signing.send",
    x.id,
    x,
  );
  if (prior) return prior;
  if (item.state !== "approved" || item.version !== x.expectedVersion)
    fail(
      "Approve the current contract before sending.",
      "LEGAL_NOT_APPROVED",
      409,
    );
  if (x.expiresAt <= Date.now() || x.expiresAt > Date.now() + 90 * 86400000)
    fail("Choose an expiry within 90 days.");
  const approval = await db
    .prepare(
      "SELECT * FROM legal_approvals WHERE item_id=? AND cycle=? AND decision='approved'",
    )
    .bind(item.id, item.version)
    .first<{ document_id: string; document_hash: string; signers: string[] }>();
  if (!approval)
    fail("The current document and signers have not been approved.");
  if (typeof approval.signers === "string")
    approval.signers = JSON.parse(approval.signers);
  const authorities = await Promise.all(
    approval.signers.map((u) => signingAuthority(db, item.id, u)),
  );
  const id = crypto.randomUUID();
  return executeCommand(
    db,
    a,
    key,
    "legal.signing.send",
    x,
    item.id,
    item.version + 1,
    { id: item.id, envelopeId: id, version: item.version + 1 },
    [
      ...guards(db, a, item.id, item.version, key),
      ...authorities.map((r) =>
        assertion(
          db,
          key,
          `authority-${r.id}`,
          "EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=? AND state='approved')",
          [r.id, r.version],
        ),
      ),
      db
        .prepare("INSERT INTO legal_envelopes VALUES(?,?,?,?,?,'sent',?,?,?,?)")
        .bind(
          id,
          item.id,
          approval.document_id,
          approval.document_hash,
          item.version,
          x.consent,
          x.expiresAt,
          a.userId,
          Date.now(),
        ),
      ...approval.signers.map((u, i) =>
        db
          .prepare(
            "INSERT INTO legal_signers(envelope_id,user_id,ordinal,state,evidence) VALUES(?,?,?,'pending',?::jsonb)",
          )
          .bind(id, u, i, JSON.stringify({ authority: authorities[i] })),
      ),
      db
        .prepare(
          "UPDATE legal_items SET state='signing',version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), item.id),
      event(
        db,
        a,
        item.id,
        "signing.sent",
        "Approved version and signers locked",
        item.version + 1,
      ),
    ],
  );
}
export async function signingAction(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  const x = z
    .object({
      action: z.enum(["signing.view", "signing.sign", "signing.decline"]),
      id: z.string(),
      expectedVersion: z.number().int().positive(),
      envelopeId: z.string(),
      consent: z.string().optional(),
    })
    .parse(raw);
  if(x.action!=="signing.view")assertLegalSigningEnabled();
  const item = await access(db, a, x.id),
    e = await db
      .prepare(
        "SELECT e.*,s.ordinal,s.state signer_state,s.viewed_at FROM legal_envelopes e JOIN legal_signers s ON s.envelope_id=e.id WHERE e.id=? AND e.item_id=? AND s.user_id=?",
      )
      .bind(x.envelopeId, item.id, a.userId)
      .first<{
        id: string;
        state: string;
        signer_state: string;
        ordinal: number;
        viewed_at: number | null;
        consent: string;
        document_hash: string;
        document_id: string;
        expires_at: number;
      }>();
  if (!e) fail("This envelope is not assigned to you.", "LEGAL_SIGNER", 403);
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    a,
    key,
    `legal.${x.action}`,
    item.id,
    x,
  );
  if (prior) return prior;
  if (
    item.version !== x.expectedVersion ||
    e.state !== "sent" ||
    e.expires_at <= Date.now() ||
    e.signer_state !== "pending"
  )
    fail("This envelope is no longer actionable.", "LEGAL_ENVELOPE_STATE", 409);
  const steps = guards(db, a, item.id, item.version, key, false);
  let state = item.state;
  steps.push(
    assertion(
      db,
      key,
      "legal-envelope",
      "EXISTS(SELECT 1 FROM legal_envelopes WHERE id=? AND state='sent' AND expires_at>?)",
      [e.id, Date.now()],
    ),
  );
  if (x.action === "signing.view")
    steps.push(
      db
        .prepare(
          "UPDATE legal_signers SET viewed_at=? WHERE envelope_id=? AND user_id=?",
        )
        .bind(Date.now(), e.id, a.userId),
    );
  else {
    if (!a.authority?.sessionId)
      fail("Sign in again and verify MFA.", "FRESH_MFA_REQUIRED", 403);
    await requireFreshMfa({
      userId: a.userId,
      sessionId: a.authority.sessionId,
    });
    const env = await runtimeEnv();
    if (
      !(await env.CONTROL_DB.prepare(
        'SELECT 1 FROM "user" WHERE id=? AND "emailVerified"=1',
      )
        .bind(a.userId)
        .first())
    )
      fail("Verify your email before signing.", "LEGAL_EMAIL_UNVERIFIED", 403);
    if (x.action === "signing.sign") {
      const authority = await signingAuthority(db, item.id, a.userId);
      steps.push(
        assertion(
          db,
          key,
          "authority-current",
          "EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=? AND state='approved')",
          [authority.id, authority.version],
        ),
      );
      if (!e.viewed_at || x.consent !== e.consent)
        fail("Review the locked document and accept the exact consent text.");
      if (
        await db
          .prepare(
            "SELECT 1 FROM legal_signers WHERE envelope_id=? AND ordinal<? AND state!='signed'",
          )
          .bind(e.id, e.ordinal)
          .first()
      )
        fail("An earlier signer must sign first.", "LEGAL_SIGNING_ORDER", 409);
      const doc = await db
        .prepare(
          "SELECT checksum FROM legal_documents WHERE id=? AND locked=true",
        )
        .bind(e.document_id)
        .first<{ checksum: string }>();
      if (doc?.checksum !== e.document_hash)
        fail("The document integrity check failed.");
      steps.push(
        db
          .prepare(
            "UPDATE legal_signers SET state='signed',signed_at=?,evidence=coalesce(evidence,'{}'::jsonb)||?::jsonb WHERE envelope_id=? AND user_id=?",
          )
          .bind(
            Date.now(),
            JSON.stringify({
              actorId: a.userId,
              email: a.email,
              sessionId: a.authority.sessionId,
              method: "verified-email+fresh-totp",
              documentHash: e.document_hash,
              consent: e.consent,
              viewedAt: e.viewed_at,
              signedAt: Date.now(),
            }),
            e.id,
            a.userId,
          ),
      );
      const left = await db
        .prepare(
          "SELECT count(*)::int n FROM legal_signers WHERE envelope_id=? AND state='pending' AND user_id<>?",
        )
        .bind(e.id, a.userId)
        .first<{ n: number }>();
      if (left?.n === 0) {
        state = "executed";
        steps.push(
          db
            .prepare("UPDATE legal_envelopes SET state='completed' WHERE id=?")
            .bind(e.id),
        );
      }
    } else {
      state = "rejected";
      steps.push(
        db
          .prepare(
            "UPDATE legal_signers SET state='declined' WHERE envelope_id=? AND user_id=?",
          )
          .bind(e.id, a.userId),
        db
          .prepare("UPDATE legal_envelopes SET state='declined' WHERE id=?")
          .bind(e.id),
      );
    }
  }
  steps.push(
    db
      .prepare(
        "UPDATE legal_items SET version=version+1,state=?,updated_at=? WHERE id=?",
      )
      .bind(state, Date.now(), item.id),
    event(db, a, item.id, x.action, e.document_hash, item.version + 1),
  );
  return executeCommand(
    db,
    a,
    key,
    `legal.${x.action}`,
    x,
    item.id,
    item.version + 1,
    { id: item.id, version: item.version + 1, state },
    steps,
  );
}
