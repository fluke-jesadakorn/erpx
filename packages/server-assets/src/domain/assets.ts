import {z} from "zod";
import {currencyDefinition} from "@erpx/feature-finance/lib/company-money";
import type {Row} from "@erpx/server-commerce/public/commerce";
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive(),date=z.string().date();
export const createAssetSchema=z.object({branchId:z.string().min(1),departmentId:z.string().nullable().optional(),code:z.string().trim().min(1).max(80),name:z.string().trim().min(2).max(160),category:z.string().trim().min(2).max(80),currency:z.string().length(3).transform(x=>x.toUpperCase()).refine(x=>Boolean(currencyDefinition(x)),'Use a current ISO currency.'),costMinor:z.number().int().safe().nonnegative().optional(),cost:z.string().regex(/^\d+(\.\d{1,12})?$/).optional(),acquiredDate:date,warrantyEnd:date.nullable().optional(),financeAssetId:z.string().min(1).optional(),financeAssetVersion:version.optional()}).refine(i=>(i.costMinor!==undefined)!==(i.cost!==undefined),'Supply one acquisition cost.').refine(i=>Boolean(i.financeAssetId)===Boolean(i.financeAssetVersion),'Use the current Finance asset version.');
export const assetActionSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('place'),expectedVersion:version,placement:z.string().trim().min(2).max(200),custodianId:z.string().min(1),reason}),
 z.object({action:z.literal('condition'),expectedVersion:version,condition:z.enum(['good','degraded','unsafe']),reason}),
 z.object({action:z.literal('retire-request'),expectedVersion:version,reason}),z.object({action:z.literal('retire-approve'),expectedVersion:version,reason}),
 z.object({action:z.literal('link-finance'),expectedVersion:version,financeAssetId:z.string().min(1),financeAssetVersion:version,reason}),
]);
export type Asset=Row&{finance_asset_id:string|null;finance_asset_version:number|null;id:string;version:number;lifecycle_status:string;condition:string;placement:string|null;custodian_id:string|null;retirement_reason:string|null;created_by:string;updated_at:number};
export type AssetAction = z.infer<typeof assetActionSchema>;
export type AssetState = {status:string;condition:string;placement:string|null;custodian:string|null;retirement:string|null};
