import { z } from "zod";
export const legalReferenceKinds = ["commerce_order", "person", "employee_request", "expense"] as const;
export type LegalReferenceKind = (typeof legalReferenceKinds)[number];
export const legalReferenceInput = z.object({ id:z.string(), expectedVersion:z.number().int().positive(), targetKind:z.enum(legalReferenceKinds), targetId:z.string() });
export type LegalReferenceInput = z.infer<typeof legalReferenceInput>;
export type LegalReference = { kind:LegalReferenceKind; id:string; label:string; version:number; path:string };
export function referenceTarget(kind:LegalReferenceKind,id:string,row:Record<string,unknown>):LegalReference {
 const paths={person:"people",commerce_order:"commerce/orders",employee_request:"requests",expense:"expenses"};
 return {kind,id,label:String(row.title??row.legal_name??row.order_number??id),version:Number(row.version??0),path:`${paths[kind]}/${encodeURIComponent(id)}`};
}
