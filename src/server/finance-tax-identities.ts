import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { thaiFinanceTaxPack, validThaiTaxId } from "@/lib/thai-finance-tax-pack";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import { attachmentContent } from "./attachments/service";
import type { SqlDatabase, SqlStatement } from "./runtime/types";

const reason = z.string().trim().min(10).max(2000);
export const taxIdentitySchema = z.object({
  partyId: z.string().min(1).nullable().default(null), legalName: z.string().trim().min(2).max(300),
  taxId: z.string().refine(validThaiTaxId, "Use a valid 13-digit Thai tax ID."), address: z.string().trim().min(10).max(2000), branchCode: z.string().regex(/^\d{5}$/),
  entityKind: z.enum(["domestic_company", "resident_individual"]), vatRegistered: z.boolean().default(false), vatRegistrationEvidence: reason.nullable().default(null),
  effectiveFrom: z.iso.date(), effectiveTo: z.iso.date(), previousIdentityId: z.string().min(1).nullable().default(null),
  signerName: z.string().trim().min(2).max(200).nullable().default(null), signerAuthority: reason.nullable().default(null),
  evidenceRef: reason, rulePackVersion: z.literal(thaiFinanceTaxPack.version), expectedVersion: z.number().int().positive().optional(),
}).superRefine((value, context) => {
  if (value.effectiveTo < value.effectiveFrom) context.addIssue({ code: "custom", path: ["effectiveTo"], message: "The identity's effective end must follow its start." });
  if (!value.partyId && (value.entityKind !== "domestic_company" || !value.signerName || !value.signerAuthority)) context.addIssue({ code: "custom", message: "The Company payer needs its domestic legal entity and authorized signer." });
  if (value.vatRegistered && !value.vatRegistrationEvidence) context.addIssue({ code: "custom", path: ["vatRegistrationEvidence"], message: "Record VAT registration evidence before issuing a VAT invoice." });
});
export const taxIdentityDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "cancel"]), reason, signatureLinkId: z.string().min(1).optional() });
export type TaxIdentity = { id: string; company_id: string; party_id: string | null; legal_name: string; tax_id: string; address: string; branch_code: string; entity_kind: "domestic_company" | "resident_individual"; vat_registered: boolean; effective_from: string; effective_to: string; previous_identity_id: string | null; signer_name: string | null; signature_link_id: string | null; signature_sha256: string | null; signature_media_type: string | null; signature_data_uri: string | null; status: string; version: number; created_by: string; reviewed_by: string | null; rule_pack_version: string };

export function requireCompanyTaxAuthority(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve" | "finance.pay") {
  actor = requirePermission(actor, permission);
  if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_TAX_AUTHORITY_REQUIRED", "Use a Company Finance assignment for statutory identities and documents.");
  return actor;
}
export function taxSourceDigest(value: unknown) { return createHash("sha256").update(JSON.stringify(value)).digest("hex"); }

export async function prepareTaxIdentity(actor: WorkContext, input: z.infer<typeof taxIdentitySchema>, commandId: string, existingId?: string) {
  actor = requireCompanyTaxAuthority(actor, "finance.write"); input = taxIdentitySchema.parse(input);
  const { write } = await holdingResources(actor.holdingId), id = existingId ?? `tax-identity-${commandId}`, operation = existingId ? "finance.tax-identity.edit" : "finance.tax-identity.prepare";
  const previous = await replayCommand(write, actor, commandId, operation, id, input); if (previous) return previous;
  if (input.partyId && !await write.prepare("SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND person_id IS NULL").bind(input.partyId, actor.companyId).first()) throw new ApiError(422, "TAX_PARTY_INVALID", "Select an active Company customer or supplier.");
  if (input.previousIdentityId && !await write.prepare("SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND party_id IS NOT DISTINCT FROM ?::text AND status='approved'").bind(input.previousIdentityId, actor.companyId, input.partyId).first()) throw new ApiError(422, "TAX_IDENTITY_PREDECESSOR_INVALID", "Select the current reviewed identity for this same Company and legal party.");
  const now = Date.now(), values = [input.partyId, input.legalName, input.taxId, input.address, input.branchCode, input.entityKind, input.vatRegistered, input.vatRegistrationEvidence, input.effectiveFrom, input.effectiveTo, input.previousIdentityId, input.signerName, input.signerAuthority, input.rulePackVersion, input.evidenceRef];
  return executeCommand(write, actor, commandId, operation, input, id, existingId ? (input.expectedVersion ?? 0) + 1 : 1, { id, status: "draft", version: existingId ? (input.expectedVersion ?? 0) + 1 : 1 }, [
    ...(existingId ? [assertion(write, commandId, "draft", "EXISTS(SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND status='draft' AND version=? AND created_by=?)", [id, actor.companyId, input.expectedVersion ?? 0, actor.userId])] : []),
    ...(input.partyId ? [assertion(write, commandId, "party", "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND person_id IS NULL)", [input.partyId, actor.companyId])] : []),
    existingId ? write.prepare("UPDATE finance_tax_identities SET party_id=?,legal_name=?,tax_id=?,address=?,branch_code=?,entity_kind=?,vat_registered=?,vat_registration_evidence=?,effective_from=?,effective_to=?,previous_identity_id=?,signer_name=?,signer_authority=?,rule_pack_version=?,evidence_ref=?,version=version+1,updated_at=? WHERE id=?").bind(...values, now, id)
      : write.prepare("INSERT INTO finance_tax_identities(id,company_id,party_id,legal_name,tax_id,address,branch_code,entity_kind,vat_registered,vat_registration_evidence,effective_from,effective_to,previous_identity_id,signer_name,signer_authority,rule_pack_version,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, ...values, actor.userId, now, now),
  ]);
}

export async function decideTaxIdentity(actor: WorkContext, id: string, input: z.infer<typeof taxIdentityDecisionSchema>, commandId: string) {
  actor = requireCompanyTaxAuthority(actor, input.action === "cancel" ? "finance.write" : "finance.approve"); input = taxIdentityDecisionSchema.parse(input);
  const { write, files } = await holdingResources(actor.holdingId), operation = `finance.tax-identity.${input.action}`;
  const previous = await replayCommand(write, actor, commandId, operation, id, input); if (previous) return previous;
  const row = await write.prepare("SELECT * FROM finance_tax_identities WHERE id=? AND company_id=?").bind(id, actor.companyId).first<TaxIdentity>();
  if (!row) throw new ApiError(404, "TAX_IDENTITY_NOT_FOUND", "The statutory identity was not found in this Company.");
  if (row.status !== "draft" || Number(row.version) !== input.expectedVersion) throw new ApiError(409, "TAX_IDENTITY_CHANGED", "Refresh the editable statutory identity.");
  if (input.action === "cancel" ? !samePerson(actor, row.created_by) : samePerson(actor, row.created_by)) throw new ApiError(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "The preparer may cancel its draft; a different Finance approver must review it.");
  let signature: { linkId: string; sha256: string; mediaType: string; dataUri: string } | null = null;
  const guards: SqlStatement[] = [];
  if (input.action === "approve" && row.party_id === null) {
    if (!input.signatureLinkId) throw new ApiError(422, "TAX_SIGNATURE_REQUIRED", "Select a scanned signature bound to this Company payer's identity.");
    const source = await write.prepare(`SELECT a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id
      WHERE l.id=? AND s.company_id=? AND s.target_type='finance_tax_identity' AND s.target_id=? AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.company_id=s.company_id AND a.source_state='available'`).bind(input.signatureLinkId, actor.companyId, id).first<{ checksum_sha256: string; media_type: string; bytes: number }>();
    if (!source || !["image/png", "image/jpeg"].includes(source.media_type) || Number(source.bytes) > 1024 * 1024) throw new ApiError(422, "TAX_SIGNATURE_NOT_READY", "The exact bound PNG/JPEG signature must pass scan and fit within 1 MiB.");
    const content = await attachmentContent(write, files, actor, input.signatureLinkId, "original"), bytes = Buffer.from(await content.object.arrayBuffer()), sha256 = createHash("sha256").update(bytes).digest("hex");
    if (sha256 !== source.checksum_sha256) throw new ApiError(503, "TAX_SIGNATURE_CHECKSUM_MISMATCH", "The signer source no longer matches its recorded checksum.");
    signature = { linkId: input.signatureLinkId, sha256, mediaType: source.media_type, dataUri: `data:${source.media_type};base64,${bytes.toString("base64")}` };
    guards.push(assertion(write, commandId, "signature", "EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND s.company_id=? AND s.target_id=? AND s.target_type='finance_tax_identity' AND l.state='bound' AND s.state='bound' AND a.source_state='available' AND a.checksum_sha256=?)", [signature.linkId, actor.companyId, id, signature.sha256]));
  }
  const now = Date.now(), status = input.action === "approve" ? "approved" : input.action === "reject" ? "rejected" : "cancelled";
  return executeCommand(write, actor, commandId, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1, signatureSha256: signature?.sha256 ?? null }, [
    write.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`finance-taxpayer:${actor.companyId}`),
    assertion(write, commandId, "identity", "EXISTS(SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, actor.companyId, input.expectedVersion]), ...guards,
    ...(input.action === "approve" ? [
      ...(row.previous_identity_id ? [assertion(write, commandId, "previous", "EXISTS(SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND status='approved' AND party_id IS NOT DISTINCT FROM ?::text)", [row.previous_identity_id, actor.companyId, row.party_id]), write.prepare("UPDATE finance_tax_identities SET status='superseded',version=version+1,updated_at=? WHERE id=?").bind(now, row.previous_identity_id)] : []),
      assertion(write, commandId, "one-current", "NOT EXISTS(SELECT 1 FROM finance_tax_identities WHERE company_id=? AND party_id IS NOT DISTINCT FROM ?::text AND status='approved')", [actor.companyId, row.party_id]),
    ] : []),
    write.prepare("UPDATE finance_tax_identities SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,signature_link_id=?,signature_sha256=?,signature_media_type=?,signature_data_uri=?,version=version+1,updated_at=? WHERE id=?").bind(status, actor.userId, input.reason, now, signature?.linkId ?? null, signature?.sha256 ?? null, signature?.mediaType ?? null, signature?.dataUri ?? null, now, id),
  ]);
}

export async function reviewedTaxIdentity(db: SqlDatabase, actor: WorkContext, id: string, partyId: string | null, date: string) {
  const identity = await db.prepare("SELECT * FROM finance_tax_identities WHERE id=? AND company_id=? AND party_id IS NOT DISTINCT FROM ?::text AND status='approved' AND effective_from<=? AND effective_to>=? AND rule_pack_version=?").bind(id, actor.companyId, partyId, date, date, thaiFinanceTaxPack.version).first<TaxIdentity>();
  if (!identity) throw new ApiError(422, "TAX_IDENTITY_NOT_READY", "A current independently reviewed Thai legal identity is required for the source date.");
  return identity;
}

export function taxIdentityGuards(db: SqlDatabase, actor: WorkContext, commandId: string, identities: TaxIdentity[], prefix="tax-identity") {
  return identities.map((identity, index) => assertion(db, commandId, `${prefix}-${index}`, "EXISTS(SELECT 1 FROM finance_tax_identities WHERE id=? AND company_id=? AND status='approved' AND version=? AND rule_pack_version=?)", [identity.id, actor.companyId, identity.version, thaiFinanceTaxPack.version]));
}

export async function currentTaxSignerGuards(db:SqlDatabase,actor:WorkContext,payer:TaxIdentity,commandId:string){
 if(!payer.signature_link_id||!payer.signature_sha256)throw new ApiError(422,"TAX_SIGNATURE_NOT_READY","The reviewed payer needs its exact scanned signer source.");
 const {files}=await holdingResources(actor.holdingId),content=await attachmentContent(db,files,actor,payer.signature_link_id,"original"),bytes=Buffer.from(await content.object.arrayBuffer());
 if(createHash("sha256").update(bytes).digest("hex")!==payer.signature_sha256)throw new ApiError(503,"TAX_SIGNATURE_CHECKSUM_MISMATCH","The signer source no longer matches the independently reviewed signature.");
 return [assertion(db,commandId,"current-tax-signer","EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND s.company_id=? AND s.target_id=? AND s.target_type='finance_tax_identity' AND s.state='bound' AND l.state='bound' AND a.source_state='available' AND a.checksum_sha256=?)",[payer.signature_link_id,actor.companyId,payer.id,payer.signature_sha256])];
}

export async function taxIdentityWorkspace(actor: WorkContext) {
  actor = requireCompanyTaxAuthority(actor, "finance.read"); const { write } = await holdingResources(actor.holdingId);
  const [identities, parties,signatures] = await Promise.all([
    write.prepare("SELECT id,party_id,legal_name,tax_id,address,branch_code,entity_kind,vat_registered,vat_registration_evidence,effective_from,effective_to,previous_identity_id,signer_name,signer_authority,signature_link_id,signature_sha256,rule_pack_version,evidence_ref,status,created_by,reviewed_by,version FROM finance_tax_identities WHERE company_id=? ORDER BY created_at DESC").bind(actor.companyId).all(),
    write.prepare("SELECT id,name,kind FROM finance_parties WHERE company_id=? AND status='active' AND person_id IS NULL ORDER BY name").bind(actor.companyId).all(),
    write.prepare("SELECT l.id,s.target_id,a.original_name,a.checksum_sha256 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE s.company_id=? AND s.target_type='finance_tax_identity' AND s.purpose_key='signer_signature' AND s.component_kind='record' AND s.state='bound' AND l.state='bound' AND a.source_state='available'").bind(actor.companyId).all(),
  ]);
  return { identities: identities.results, parties: parties.results, signatures:signatures.results,rulePack: thaiFinanceTaxPack };
}
