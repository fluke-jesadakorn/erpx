import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {TargetRow} from "../../files/public/contracts/attachments-operational";
const versionOwners:Record<string,string>={"ecommerce_listing": "ecommerce_listings"};
const allowedTargets=new Set<string>(["ecommerce_listing"]);
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
  if (targetType === "ecommerce_listing")
    return assertion(
      db,
      commandId,
      `attachment-owner-${index}`,
      "EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)",
      [targetId, companyId, version],
    );
  const table = versionOwners[targetType];
  if (!table) return null;
  return assertion(
    db,
    commandId,
    `attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
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
export function OperationalAttachmentEcommerceFacts(db:SqlDatabase){return {
readEcommerceListings1:(facts:{targetId:string;companyId:string;})=>db.prepare("SELECT c.company_id,NULL::text branch_id,NULL::text department_id,l.version FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?").bind(facts.targetId,facts.companyId).first<TargetRow>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
