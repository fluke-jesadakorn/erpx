import { z } from 'zod';
import { thaiFinanceTaxPack, validThaiTaxId } from "@/lib/finance/thai-finance-tax-pack";
const reason = z.string().trim().min(10).max(2000);
export const taxIdentitySchema = z.object({
    partyId: z.string().min(1).nullable().default(null), legalName: z.string().trim().min(2).max(300),
    taxId: z.string().refine(validThaiTaxId, "Use a valid 13-digit Thai tax ID."), address: z.string().trim().min(10).max(2000), branchCode: z.string().regex(/^\d{5}$/),
    entityKind: z.enum(["domestic_company", "resident_individual"]), vatRegistered: z.boolean().default(false), vatRegistrationEvidence: reason.nullable().default(null),
    effectiveFrom: z.iso.date(), effectiveTo: z.iso.date(), previousIdentityId: z.string().min(1).nullable().default(null),
    signerName: z.string().trim().min(2).max(200).nullable().default(null), signerAuthority: reason.nullable().default(null),
    evidenceRef: reason, rulePackVersion: z.literal(thaiFinanceTaxPack.version), expectedVersion: z.number().int().positive().optional(),
}).superRefine((value, context) => {
    if (value.effectiveTo < value.effectiveFrom)
        context.addIssue({ code: "custom", path: ["effectiveTo"], message: "The identity's effective end must follow its start." });
    if (!value.partyId && (value.entityKind !== "domestic_company" || !value.signerName || !value.signerAuthority))
        context.addIssue({ code: "custom", message: "The Company payer needs its domestic legal entity and authorized signer." });
    if (value.vatRegistered && !value.vatRegistrationEvidence)
        context.addIssue({ code: "custom", path: ["vatRegistrationEvidence"], message: "Record VAT registration evidence before issuing a VAT invoice." });
});
export const taxIdentityDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "cancel"]), reason, signatureLinkId: z.string().min(1).optional() });
export type TaxIdentity = {
    id: string;
    company_id: string;
    party_id: string | null;
    legal_name: string;
    tax_id: string;
    address: string;
    branch_code: string;
    entity_kind: "domestic_company" | "resident_individual";
    vat_registered: boolean;
    effective_from: string;
    effective_to: string;
    previous_identity_id: string | null;
    signer_name: string | null;
    signature_link_id: string | null;
    signature_sha256: string | null;
    signature_media_type: string | null;
    signature_data_uri: string | null;
    status: string;
    version: number;
    created_by: string;
    reviewed_by: string | null;
    rule_pack_version: string;
};
export type IdentityInput = z.infer<typeof taxIdentitySchema>;
export type IdentityDecision = z.infer<typeof taxIdentityDecisionSchema>;
export type TaxPermission = 'finance.read' | 'finance.write' | 'finance.approve' | 'finance.pay';
export type TaxSignatureMetadata = {
    checksum_sha256: string;
    media_type: string;
    bytes: number;
};
export type TaxSignature = {
    linkId: string;
    sha256: string;
    mediaType: string;
    dataUri: string;
};
export type TaxProjection = Record<string, string | number | boolean | null>;
