import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import type {TargetRow} from "@erpx/files-server/public/contracts/attachments-operational";
import {nativeQualityGuardAssertion,RD_ATTACH_SPEC_CAS,RD_ATTACHMENT_SPEC_SCOPE} from "./native-quality-read";
import {readNativeQualityScope} from "./native-quality-scope";
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
readInventoryQualitySpecs1:(facts:{targetId:string;companyId:string;}):Promise<TargetRow|null>=>readNativeQualityScope(RD_ATTACHMENT_SPEC_SCOPE,[facts.targetId,facts.companyId],db),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
