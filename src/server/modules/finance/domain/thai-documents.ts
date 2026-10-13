import { z } from 'zod';
import type { ThaiDocumentSnapshot } from "@/lib/finance/finance-thai-document";
import type { TaxIdentity } from './tax-identities';
export type IssuedDocument = {
    id: string;
    company_id: string;
    document_kind: ThaiDocumentSnapshot["kind"];
    document_number: string;
    issue_date: string;
    payment_id: string | null;
    invoice_id: string | null;
    assessment_id: string | null;
    previous_document_id: string | null;
    edition: number;
    source_fingerprint: string;
    snapshot_json: string;
    export_html: string;
    export_sha256: string;
    created_at: number;
};
export const thaiDocumentChangeSchema = z.object({ payerIdentityId: z.string().min(1), payeeIdentityId: z.string().min(1), issueDate: z.iso.date(), reason: z.string().trim().min(10).max(2000) });
export const thaiDocumentDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "cancel"]), reason: z.string().trim().min(10).max(2000) });
export const thaiInvoiceIssueSchema = z.object({ expectedVersion: z.number().int().positive(), payerIdentityId: z.string().min(1), payeeIdentityId: z.string().min(1), reason: z.string().trim().min(10).max(2000) });
export type IssueInput = {
    snapshot: Omit<ThaiDocumentSnapshot, 'documentNumber'>;
    payer: TaxIdentity;
    payee: TaxIdentity;
    paymentId?: string;
    invoiceId?: string;
    assessmentId?: string;
    previous?: IssuedDocument;
    commandId: string;
};
export type IssueSummary = {
    id: string;
    documentNumber: string;
    sha256: string;
    html: string;
};
export type IssuePlan = IssueSummary & {
    complete: ThaiDocumentSnapshot;
    fingerprint: string;
    year: number;
    last: number;
    sequence: number;
    now: number;
};
export type InvoiceIssueInput = z.infer<typeof thaiInvoiceIssueSchema>;
export type DocumentChangeInput = z.infer<typeof thaiDocumentChangeSchema>;
export type DocumentDecision = z.infer<typeof thaiDocumentDecisionSchema>;
export type DocumentChangeSource = {
    originalSha256: string;
    payerVersion: number;
    payeeVersion: number;
    snapshot: ThaiDocumentSnapshot;
};
