import "server-only";
import { z } from "zod";
import { X509Certificate } from "node:crypto";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { requirePermission } from "../iam-permissions";
import { executeCommand, replayCommand, assertion } from "../commands";
import { samePerson } from "@/lib/iam";
import { hash } from "./service";
import { lock, fail } from "./access";
export async function trustView(db: SqlDatabase, a: WorkContext) {
  requirePermission(a, "legal.manage");
  return (
    await db
      .prepare(
        "SELECT id,kind,name,source,checksum,state,created_by,approved_by,version FROM legal_trust_material WHERE company_id=? ORDER BY created_at DESC",
      )
      .bind(a.companyId)
      .all()
  ).results;
}
export async function trustCommand(
  db: SqlDatabase,
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  requirePermission(a, "legal.manage");
  if (a.branchId || a.departmentId) fail("Manage trust at Company scope.");
  const x = z
    .object({
      action: z.enum(["trust.add", "trust.approve", "trust.retire"]),
      materialId: z.string().optional(),
      materialVersion: z.number().int().positive().optional(),
    })
    .parse(raw);
  const prior = await replayCommand(db, a, key, x.action, a.companyId, raw);
  if (prior) return prior;
  const steps = [lock(db, a.companyId)];
  if (x.action === "trust.add") {
    const p = z
      .object({
        kind: z.enum(["root", "intermediate", "crl", "ocsp"]),
        name: z.string().trim().min(3).max(200),
        source: z.url().refine((v) => v.startsWith("https:")),
        content: z.string().min(20).max(2000000),
      })
      .parse(raw);
    let bytes: Buffer;
    if (["root", "intermediate"].includes(p.kind)) {
      const cert = new X509Certificate(p.content);
      if (!cert.ca) fail("A CA certificate is required.");
      if (Date.parse(cert.validTo) < Date.now())
        fail("This CA certificate is expired.");
      bytes = cert.raw;
    } else {
      if (!/^[A-Za-z0-9+/=\r\n]+$/.test(p.content))
        fail("Upload base64 DER revocation material.");
      bytes = Buffer.from(p.content, "base64");
    }
    steps.push(
      assertion(
        db,
        key,
        "trust-limit",
        "(SELECT count(*) FROM legal_trust_material WHERE company_id=? AND state!='retired')<100",
        [a.companyId],
      ),
      db
        .prepare(
          "INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,state,created_by,created_at) VALUES(?,?,?,?,?,?,?,'proposed',?,?)",
        )
        .bind(
          key,
          a.companyId,
          p.kind,
          p.name,
          p.source,
          bytes.toString("base64"),
          hash(bytes),
          a.userId,
          Date.now(),
        ),
    );
  } else {
    const row = await db
      .prepare("SELECT * FROM legal_trust_material WHERE id=? AND company_id=?")
      .bind(x.materialId, a.companyId)
      .first<{
        id: string;
        state: string;
        version: number;
        created_by: string;
      }>();
    if (!row || row.version !== x.materialVersion)
      fail("Trust material changed. Refresh.");
    if (
      x.action === "trust.approve" &&
      (row.state !== "proposed" || samePerson(a, row.created_by))
    )
      fail(
        "A different manager must verify the source and fingerprint.",
        "LEGAL_INDEPENDENT_REVIEW",
        403,
      );
    steps.push(
      assertion(
        db,
        key,
        "trust-version",
        "EXISTS(SELECT 1 FROM legal_trust_material WHERE id=? AND version=?)",
        [row.id, row.version],
      ),
      db
        .prepare(
          "UPDATE legal_trust_material SET state=?,approved_by=?,version=version+1 WHERE id=?",
        )
        .bind(
          x.action === "trust.approve" ? "approved" : "retired",
          a.userId,
          row.id,
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
