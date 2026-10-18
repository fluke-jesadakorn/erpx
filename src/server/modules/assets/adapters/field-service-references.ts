import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { ServiceReferenceScope } from "../../organization/public/field-service-references";

export type ServiceAssetReference = { id: string; code: string; name: string; version: number; branch_id: string; department_id: string | null };

export const fieldServiceAssetSourceRows = "SELECT id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version FROM operational_assets";
export function fieldServiceAssetSourceReadLocks(database:SqlDatabase) {
  return [database.prepare("LOCK TABLE operational_assets IN SHARE MODE")];
}
/** Private access-audit effects can write a source; lease before its snapshot. */
export function fieldServiceAssetAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE operational_assets IN SHARE ROW EXCLUSIVE MODE');}
export function fieldServiceAssetAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare('SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?').bind(id,companyId).first<string>('image');}

/** Current actor scope and asset lifecycle apply to new Service links as well as their picker. */
export function fieldServiceAssetReferencePredicate(alias: "a" = "a") {
  return `${alias}.lifecycle_status NOT IN ('retired','retirement_pending') AND (?::text IS NULL OR ${alias}.department_id=?)`;
}

/** This reference is composed only after Service's parts/assets purchase gate. */
export async function fieldServiceAssets(database: SqlDatabase, scope: ServiceReferenceScope): Promise<ServiceAssetReference[]> {
  return (await database.prepare(`SELECT a.id,a.code,a.name,a.version,a.branch_id,a.department_id FROM operational_assets a
    WHERE a.company_id=? AND (?::text IS NULL OR a.branch_id=?)
    AND ${fieldServiceAssetReferencePredicate()} ORDER BY a.code,a.id`)
    .bind(scope.companyId,scope.branchId,scope.branchId,scope.departmentId,scope.departmentId).all<ServiceAssetReference>()).results;
}
