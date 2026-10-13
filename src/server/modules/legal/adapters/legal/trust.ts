import "server-only";
import { X509Certificate } from "node:crypto";
import { samePerson } from "@/lib/identity_access/iam";
import type { SqlDatabase, SqlStatement } from "../../../runtime/public/runtime/types";
import type { WorkContext } from "../../../identity_access/public/authorization";
import { requirePermission } from "../../../identity_access/public/iam-permissions";
import { ApiError } from "../../../runtime/public/http";
import { assertion, executeCommand, replayCommand } from "../../../workflow/public/commands";
import { lock } from "./access";
import { hash } from "./service";
import type { LegalWrite } from "../../application/service";
import { createLegalTrustApplication, type LegalTrustMaterial } from "../../application/trust";
function application(db: SqlDatabase, a: WorkContext) {
  const statements = new WeakMap<LegalWrite, SqlStatement>(), stage = (statement: SqlStatement): LegalWrite => { const write: LegalWrite = { owner: "legal" }; statements.set(write, statement); return write; };
  return createLegalTrustApplication({ companyScope: !a.branchId && !a.departmentId, require: () => requirePermission(a, "legal.manage"), samePerson: id => samePerson(a, id), certificate: content => { const certificate = new X509Certificate(content); return { ca: certificate.ca, validTo: certificate.validTo, raw: certificate.raw }; }, decodeBase64: content => Buffer.from(content, "base64"), encodeBase64: bytes => Buffer.from(bytes).toString("base64"), hash, clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: {
    view: async () => (await db.prepare("SELECT id,kind,name,source,checksum,state,created_by,approved_by,version FROM legal_trust_material WHERE company_id=? ORDER BY created_at DESC").bind(a.companyId).all()).results,
    replay: (key, operation, raw) => replayCommand(db, a, key, operation, a.companyId, raw), lock: () => stage(lock(db, a.companyId)),
    material: id => db.prepare("SELECT * FROM legal_trust_material WHERE id=? AND company_id=?").bind(id, a.companyId).first<LegalTrustMaterial>(),
    addition: (key, input, content, checksum, now) => [assertion(db, key, "trust-limit", "(SELECT count(*) FROM legal_trust_material WHERE company_id=? AND state!='retired')<100", [a.companyId]), db.prepare("INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,state,created_by,created_at) VALUES(?,?,?,?,?,?,?,'proposed',?,?)").bind(key, a.companyId, input.kind, input.name, input.source, content, checksum, a.userId, now)].map(stage),
    decision: (key, row, state) => [assertion(db, key, "trust-version", "EXISTS(SELECT 1 FROM legal_trust_material WHERE id=? AND version=?)", [row.id, row.version]), db.prepare("UPDATE legal_trust_material SET state=?,approved_by=?,version=version+1 WHERE id=?").bind(state, a.userId, row.id)].map(stage),
    commit: (key, operation, raw, writes) => executeCommand(db, a, key, operation, raw, a.companyId, 1, { id: a.companyId, version: 1 }, writes.map(write => { const statement = statements.get(write); if (!statement) throw new Error("LEGAL_WRITE_REPOSITORY_MISMATCH"); return statement; })),
  } });
}
export function trustView(db: SqlDatabase, a: WorkContext) { return application(db, a).trustView(); }
export function trustCommand(db: SqlDatabase, a: WorkContext, raw: unknown, key: string) { return application(db, a).trustCommand(raw, key); }
