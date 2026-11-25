import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {TargetRow} from "@erpx/server-files/public/contracts/attachments-operational";
import {nativeQualityGuardAssertion,RD_ATTACH_SPEC_CAS} from "./native-quality-read";
import {qualityAttachmentScopeReadSql} from "./quality-attachment-scope-read-sql";
const versionOwners:Record<string,string>={"inventory_quality_spec": "inventory_quality_specs"};
const allowedTargets=new Set<string>(["inventory_quality_spec"]);
function remainingOperationalAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const table = versionOwners[targetType];
  if (!table) return null;
  return nativeQualityGuardAssertion(
    db,
    commandId,
    `attachment-owner-${index}`,
    RD_ATTACH_SPEC_CAS,
    [targetId, companyId, version],
  );
}
function remainingOperationalAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const table = versionOwners[targetType];
  if (!table) return null;
  return db
    .prepare(
      `UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`,
    )
    .bind(now, targetId, expectedVersion);
}
export function OperationalAttachmentQualityFacts(db:SqlDatabase){return {
readInventoryQualitySpecs1:(facts:{targetId:string;companyId:string;})=>db.prepare(qualityAttachmentScopeReadSql('spec')).bind(facts.targetId,facts.companyId).first<TargetRow>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
