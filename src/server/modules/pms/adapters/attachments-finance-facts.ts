import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"pms_room_type": {"table": "pms_records", "extra": "kind='room_type'"}, "pms_guest": {"table": "pms_records", "extra": "kind='guest'"}, "pms_reservation": {"table": "pms_records", "extra": "kind='reservation'"}, "pms_stay": {"table": "pms_records", "extra": "kind='stay'"}, "pms_task": {"table": "pms_records", "extra": "kind='task'"}, "pms_lost_property": {"table": "pms_records", "extra": "kind='lost'"}, "pms_property": {"table": "pms_properties"}};
const pmsKinds:Record<string,string>={"pms_room_type": "room_type", "pms_guest": "guest", "pms_reservation": "reservation", "pms_stay": "stay", "pms_task": "task", "pms_lost_property": "lost"};
const allowedTargets=new Set(["pms_room_type", "pms_guest", "pms_reservation", "pms_stay", "pms_task", "pms_lost_property", "pms_property"]);
function financeOperationsAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  if (targetType in pmsKinds)
    return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=? AND r.version=? AND r.kind=?)`, [targetId, companyId, version, pmsKinds[targetType]]);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}
function financeOperationsAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return db.prepare(`UPDATE ${item.table} SET ${column}=${column}+1,updated_at=? WHERE id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""}`)
    .bind(now, targetId, expectedVersion);
}
export function FinanceAttachmentPmsFacts(db:SqlDatabase){return {
readOwner4:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(`SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind
       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
       WHERE r.id=? AND p.company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},
readOwner5:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare("SELECT company_id,branch_id,NULL::text department_id,version FROM pms_properties WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
