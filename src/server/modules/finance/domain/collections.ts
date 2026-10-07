import {z} from 'zod';
export const collectionCaseSchema = z.object({
  invoiceId: z.string().min(1),
  expectedVersion: z.number().int().positive(),
  asOf: z.string().date(),
  message: z.string().trim().min(3).max(4000).optional(),
});
export const reviseCollectionCaseSchema = collectionCaseSchema
  .omit({ invoiceId: true })
  .extend({
    expectedInvoiceVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
  });
export type CollectionInvoice = {
  id: string;
  version: number;
  kind: string;
  status: string;
  outstanding_minor: number;
  currency: string;
  due_date: string;
  invoice_number: string;
  party_id: string;
  party_name: string;
  party_version: number;
};
export function stage(days: number) {
  return days > 90
    ? "over_90"
    : days > 60
      ? "61_90"
      : days > 30
        ? "31_60"
        : "1_30";
}
export type CollectionQuery = {
  invoiceCursor?: string;
  caseCursor?: string;
  q?: string;
};

export type CollectionParent={id:string;root_id:string;revision_number:number;version:number;invoice_id:string;stage:string;status:string;as_of_date:string};
export type CollectionCase={status:string;version:number;invoice_id:string;invoice_version:number;source_hash:string;message:string;prepared_by:string;reviewed_by:string|null};
export type CollectionExposure={id:string;kind:string;due_date:string;currency:string;historical_outstanding_minor:number;historical_base_outstanding_minor:number};
export type CollectionPage={items:(Record<string,unknown>&{id:string;page_key:string})[];nextCursor:string|null;previousCursor:string|null;hasMore:boolean};
