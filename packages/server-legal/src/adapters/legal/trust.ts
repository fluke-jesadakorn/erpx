import "server-only";
import { X509Certificate } from "node:crypto";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { lock } from "@erpx/server-legal/adapters/legal/access";
import { hash } from "@erpx/server-legal/adapters/legal/service";
import type { LegalWrite } from "@erpx/server-legal/application/service";
import { createLegalTrustApplication, type LegalTrustMaterial } from "@erpx/server-legal/application/trust";
import { legalTrustProcessing } from '@erpx/server-collaboration/adapters/native-legal-trust-processing';
import { nativeLegalAuxiliaryRead } from '../native-legal-auxiliary';
/** Exact projection of the native `trust` auxiliary read; the transport accepts
 * every authoritative field and no SQL text remains in TypeScript. */
type LegalTrustViewRow = {
  id: string; kind: string; name: string; source: string; checksum: string;
  state: string; created_by: string; approved_by: string | null; version: number;
};
function application(db: SqlDatabase, a: WorkContext) {const native=legalTrustProcessing(db,a.companyId,a.userId);
  const statements = new WeakMap<LegalWrite, SqlStatement>(), stage = (statement: SqlStatement): LegalWrite => { const write: LegalWrite = { owner: "legal" }; statements.set(write, statement); return write; };
  return createLegalTrustApplication({ companyScope: !a.branchId && !a.departmentId, require: () => requirePermission(a, "legal.manage"), samePerson: id => samePerson(a, id), certificate: content => { const certificate = new X509Certificate(content); return { ca: certificate.ca, validTo: certificate.validTo, raw: certificate.raw }; }, decodeBase64: content => Buffer.from(content, "base64"), encodeBase64: bytes => Buffer.from(bytes).toString("base64"), hash, clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), repository: {
    view: () => nativeLegalAuxiliaryRead<LegalTrustViewRow[]>(a, 'trust'),
    replay: (key, operation, raw) => replayCommand(db, a, key, operation, a.companyId, raw), lock: () => stage(lock(db, a.companyId)),
    material: id => db.prepare("SELECT * FROM legal_trust_material WHERE id=? AND company_id=?").bind(id, a.companyId).first<LegalTrustMaterial>(),
    addition: async (key, input, content, checksum, now) => [assertion(db, key, "trust-limit", "(SELECT count(*) FROM legal_trust_material WHERE company_id=? AND state!='retired')<100", [a.companyId]), await native.addition({ key, kind: input.kind, name: input.name, source: input.source, content, checksum, now })].map(stage),
    decision: async (key, row, state) => [assertion(db, key, "trust-version", "EXISTS(SELECT 1 FROM legal_trust_material WHERE id=? AND version=?)", [row.id, row.version]), await native.decision({ key, state, id: row.id, version: row.version })].map(stage),
    commit: (key, operation, raw, writes) => executeCommand(db, a, key, operation, raw, a.companyId, 1, { id: a.companyId, version: 1 }, writes.map(write => { const statement = statements.get(write); if (!statement) throw new Error("LEGAL_WRITE_REPOSITORY_MISMATCH"); return statement; })),
  } });
}
export function trustView(db: SqlDatabase, a: WorkContext) { return application(db, a).trustView(); }
export function trustCommand(db: SqlDatabase, a: WorkContext, raw: unknown, key: string) { return application(db, a).trustCommand(raw, key); }
