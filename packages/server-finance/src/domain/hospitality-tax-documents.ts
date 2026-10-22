import {z} from "zod";
export const taxDocumentId = z.string().uuid();
export const taxDocumentCommandSchema = z.discriminatedUnion('action', [
  z.object({ action: z.literal('record'), invoiceId: taxDocumentId, issuer: z.string().trim().min(2).max(160), externalReference: z.string().trim().min(2).max(100), issuedOn: z.iso.date(), documentTotalMinor:z.number().int().nonnegative(), documentTaxMinor:z.number().int().nonnegative(), reason: z.string().trim().min(3).max(500) }).refine(value=>value.documentTaxMinor<=value.documentTotalMinor,{message:'Document tax cannot exceed its total.'}),
  z.object({ action: z.literal('verify'), invoiceId: taxDocumentId, documentId: taxDocumentId, expectedVersion: z.number().int().positive() }),
  z.object({ action: z.literal('void'), invoiceId: taxDocumentId, documentId: taxDocumentId, expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(500) }),
]);
export type TaxDocumentCommand = z.infer<typeof taxDocumentCommandSchema>;
export type Invoice = { id:string; company_id:string; source_type:string; source_id:string; currency:string; total_minor:number; tax_minor:number; status:string; branch_id:string | null };
export type Document = { id:string; company_id:string; invoice_id:string; source_type:string; issuer:string; external_reference:string; issued_on:string; currency:string; total_minor:number; tax_minor:number; document_total_minor:number|null; document_tax_minor:number|null; state:string; version:number; recorded_by:string; verified_by:string|null; voided_by:string|null; reason:string; created_at:number; updated_at:number };

export type TaxDocumentResult={id:string;state:string;version:number};
