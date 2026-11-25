import "server-only";
import {headers} from 'next/headers';
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {nativeNodeReferences} from './native-lane-23/client';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-files/public/contracts/attachments-finance";
import { nativeOrgStatement } from "./native-organization-owner-write";
import {ownerVersionGuardSource} from './organization-read-tail-sources';
import {nodeChangeByIdCompany} from './organization-read-tail-sources-r40';
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"organization_change": {"table": "organization_node_changes"}};
const allowedTargets=new Set(["organization_change", "organization_node"]);
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
  if (targetType === "organization_node")
    return assertion(db, commandId, `attachment-owner-${index}`, ownerVersionGuardSource('organization_nodes', 'hierarchy_revision'), [targetId, companyId, version]);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return assertion(db, commandId, `attachment-owner-${index}`, ownerVersionGuardSource(item.table, column, item.extra), [targetId, companyId, version]);
}
/** Owner-owned version step. The SQL is selected by the native owner registry
 * from a typed kind; no SQL text is assembled here. */
function financeOperationsAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
  companyId: string,
): Promise<SqlStatement> | null {
if(!allowedTargets.has(targetType))return null;
  if (targetType === "organization_node")
    return nativeOrgStatement(db, companyId, 'org_attachment_node_version_step', [targetId, expectedVersion], `attachment-node:${targetId}:${expectedVersion}`, now);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  return nativeOrgStatement(db, companyId, 'attachment_owner_version_step', [item.table, now, targetId, expectedVersion], `attachment-owner:${targetType}:${targetId}:${expectedVersion}`, now);
}
export function FinanceAttachmentOrganizationFacts(db:SqlDatabase,companyId=''){return {
/** Native owner read: SQL and projection are owned by the Rust crate; this
 * adapter transports validated facts only and fails closed. */
readOwner7:async (facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;}):Promise<Row|null>=> (await nativeNodeReferences(await headers(),{companyId:facts.companyId,nodeId:facts.targetId})).attachment as Row | null,
readOwner8:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(nodeChangeByIdCompany('company_id,NULL::text branch_id,NULL::text department_id,version,requested_by,state')).bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now,companyId)
};}
