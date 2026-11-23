import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Owner-local fixed SQL for the Assets attachment target descriptor. Shared by
 * the policy and remaining-operational Files reads so the TS read tail carries
 * no duplicate statement; the native counterpart is
 * `observe_asset_attachment_descriptor`. Callers pass only fixed column and
 * relation names, never SQL or a relation name from a request. */
export const assetAttachmentDescriptorSql = (table:string,branch:string,department:string,version:string,extra="") =>
  `SELECT company_id,${branch} AS branch_id,${department} AS department_id,${version} AS version FROM ${table} WHERE id=? AND company_id=?${extra}`;
const descriptors:Record<string,{table:string;version:string;branch?:string;department?:string;extra?:string;readPermission:string;writePermission:string}>={"asset": {"table": "operational_assets", "version": "version", "branch": "branch_id", "department": "department_id", "readPermission": "records.read", "writePermission": "records.write"}};
export function policyAssetsFacts(db:SqlDatabase){return {readDescriptor:(facts:{targetId:string;companyId:string;targetType:string;})=>{const descriptor=descriptors[facts.targetType];if(!descriptor)return Promise.resolve(null);const version=descriptor.version==="1"?"1":descriptor.version,branch=descriptor.branch??"NULL",department=descriptor.department??"NULL",extra=descriptor.extra?` AND ${descriptor.extra}`:"";return db.prepare(assetAttachmentDescriptorSql(descriptor.table,branch,department,version,extra)).bind(facts.targetId,facts.companyId).first<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; }>();}};}
