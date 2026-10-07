import { z } from 'zod';
import { samePerson } from '@/lib/iam';
import { thaiFinanceTaxPack } from '@/lib/thai-finance-tax-pack';
import { taxIdentitySchema, taxIdentityDecisionSchema, type TaxIdentity, type IdentityInput, type IdentityDecision, type TaxSignature, type TaxSignatureMetadata, type TaxProjection } from '../domain/tax-identities';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export interface TaxIdentityPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    activeParty(actor: WorkContext, id: string): Promise<boolean>;
    predecessor(actor: WorkContext, id: string, partyId: string | null): Promise<boolean>;
    identity(actor: WorkContext, id: string): Promise<TaxIdentity | null>;
    signatureMetadata(actor: WorkContext, id: string, linkId: string): Promise<TaxSignatureMetadata | null>;
    signatureContent(actor: WorkContext, linkId: string): Promise<{
        sha256: string;
        base64: string;
    }>;
    reviewedIdentity(actor: WorkContext, id: string, partyId: string | null, date: string): Promise<TaxIdentity | null>;
    currentSignerHash(actor: WorkContext, linkId: string): Promise<string>;
    identities(actor: WorkContext): Promise<TaxProjection[]>;
    parties(actor: WorkContext): Promise<TaxProjection[]>;
    signatures(actor: WorkContext): Promise<TaxProjection[]>;
    commitPreparation(facts: {
        actor: WorkContext;
        input: IdentityInput;
        commandId: string;
        existingId?: string;
        id: string;
        operation: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitDecision(facts: {
        actor: WorkContext;
        id: string;
        input: IdentityDecision;
        commandId: string;
        row: TaxIdentity;
        operation: string;
        signature: TaxSignature | null;
        status: string;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createTaxIdentities(ports: TaxIdentityPorts) {
    function requireCompanyTaxAuthority(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve" | "finance.pay") {
        actor = ports.authorize(actor, permission);
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_TAX_AUTHORITY_REQUIRED", "Use a Company Finance assignment for statutory identities and documents.");
        return actor;
    }
    async function prepareTaxIdentity(actor: WorkContext, raw: z.input<typeof taxIdentitySchema>, commandId: string, existingId?: string) {
        actor = requireCompanyTaxAuthority(actor, "finance.write");
        const input = taxIdentitySchema.parse(raw);
        await ports.ready(actor);
        const id = existingId ?? `tax-identity-${commandId}`, operation = existingId ? "finance.tax-identity.edit" : "finance.tax-identity.prepare";
        const previous = await ports.replay(actor, commandId, operation, id, input);
        if (previous)
            return previous;
        if (input.partyId && !await ports.activeParty(actor, input.partyId))
            throw ports.error(422, "TAX_PARTY_INVALID", "Select an active Company customer or supplier.");
        if (input.previousIdentityId && !await ports.predecessor(actor, input.previousIdentityId, input.partyId))
            throw ports.error(422, "TAX_IDENTITY_PREDECESSOR_INVALID", "Select the current reviewed identity for this same Company and legal party.");
        return ports.commitPreparation({ actor, input, commandId, existingId, id, operation, now: ports.now() });
    }
    async function decideTaxIdentity(actor: WorkContext, id: string, raw: z.input<typeof taxIdentityDecisionSchema>, commandId: string) {
        actor = requireCompanyTaxAuthority(actor, raw.action === "cancel" ? "finance.write" : "finance.approve");
        const input = taxIdentityDecisionSchema.parse(raw);
        await ports.ready(actor);
        const operation = `finance.tax-identity.${input.action}`;
        const previous = await ports.replay(actor, commandId, operation, id, input);
        if (previous)
            return previous;
        const row = await ports.identity(actor, id);
        if (!row)
            throw ports.error(404, "TAX_IDENTITY_NOT_FOUND", "The statutory identity was not found in this Company.");
        if (row.status !== "draft" || Number(row.version) !== input.expectedVersion)
            throw ports.error(409, "TAX_IDENTITY_CHANGED", "Refresh the editable statutory identity.");
        if (input.action === "cancel" ? !samePerson(actor, row.created_by) : samePerson(actor, row.created_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "The preparer may cancel its draft; a different Finance approver must review it.");
        let signature: {
            linkId: string;
            sha256: string;
            mediaType: string;
            dataUri: string;
        } | null = null;
        if (input.action === "approve" && row.party_id === null) {
            if (!input.signatureLinkId)
                throw ports.error(422, "TAX_SIGNATURE_REQUIRED", "Select a scanned signature bound to this Company payer's identity.");
            const source = await ports.signatureMetadata(actor, id, input.signatureLinkId);
            if (!source || !["image/png", "image/jpeg"].includes(source.media_type) || Number(source.bytes) > 1024 * 1024)
                throw ports.error(422, "TAX_SIGNATURE_NOT_READY", "The exact bound PNG/JPEG signature must pass scan and fit within 1 MiB.");
            const { sha256, base64 } = await ports.signatureContent(actor, input.signatureLinkId);
            if (sha256 !== source.checksum_sha256)
                throw ports.error(503, "TAX_SIGNATURE_CHECKSUM_MISMATCH", "The signer source no longer matches its recorded checksum.");
            signature = { linkId: input.signatureLinkId, sha256, mediaType: source.media_type, dataUri: `data:${source.media_type};base64,${base64}` };
        }
        const now = ports.now(), status = input.action === 'approve' ? 'approved' : input.action === 'reject' ? 'rejected' : 'cancelled';
        return ports.commitDecision({ actor, id, input, commandId, row, operation, signature, status, now });
    }
    async function reviewedTaxIdentity(actor: WorkContext, id: string, partyId: string | null, date: string) {
        const identity = await ports.reviewedIdentity(actor, id, partyId, date);
        if (!identity)
            throw ports.error(422, 'TAX_IDENTITY_NOT_READY', 'A current independently reviewed Thai legal identity is required for the source date.');
        return identity;
    }
    async function currentTaxSigner(actor: WorkContext, payer: TaxIdentity) {
        if (!payer.signature_link_id || !payer.signature_sha256)
            throw ports.error(422, 'TAX_SIGNATURE_NOT_READY', 'The reviewed payer needs its exact scanned signer source.');
        if (await ports.currentSignerHash(actor, payer.signature_link_id) !== payer.signature_sha256)
            throw ports.error(503, 'TAX_SIGNATURE_CHECKSUM_MISMATCH', 'The signer source no longer matches the independently reviewed signature.');
    }
    async function taxIdentityWorkspace(actor: WorkContext) { actor = requireCompanyTaxAuthority(actor, 'finance.read'); await ports.ready(actor); const [identities, parties, signatures] = await Promise.all([ports.identities(actor), ports.parties(actor), ports.signatures(actor)]); return { identities, parties, signatures, rulePack: thaiFinanceTaxPack }; }
    return { requireCompanyTaxAuthority, prepareTaxIdentity, decideTaxIdentity, reviewedTaxIdentity, currentTaxSigner, taxIdentityWorkspace };
}

