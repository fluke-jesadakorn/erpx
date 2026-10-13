import {z} from 'zod';
import type {DirectoryInventory} from "@/lib/identity_access/directory-operations";
import type {ProviderConnection} from '../public/iam-providers';
import {BusinessRuleError} from '../../shared';
export const id=z.string().min(1).max(180);
export const positionGpoInput=z.object({
  positionId:id,expectedVersion:z.number().int().min(0),connectionId:id,gpoId:id,
  userFilterGroupId:id,computerTargetOuId:id,
  reason:z.string().trim().min(3).max(1000),
});
export type Input=z.infer<typeof positionGpoInput>;
export const previewToken=z.string().regex(/^[a-f0-9]{64}$/);
export type Connection=ProviderConnection&{company_id:string;status:string;version:number};
export const unmapInput=z.object({positionId:id,expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(1000)});
export type Job={id:string;company_id:string;position_id:string;person_id:string;engagement_id:string|null;account_id:string|null;revision:number;engagement_version:number;connection_id:string;gpo_id:string;filter_group_id:string;computer_target_ou_id:string;desired_action:"add"|"remove";version:number;rule_id:string;source_id:string};
export type Claim={connection_id:string;group_id:string;member_id:string;source_id:string;rule_id:string;company_id:string;position_id:string;person_id:string;engagement_id:string;engagement_version:number;revision:number;gpo_id:string;computer_target_ou_id:string};
export type Ownership={connection_id:string;gpo_id:string;kind:"filter_group"|"ou_link";target_id:string;status:"pending"|"written"|"confirmed"};
export type Desired=Omit<Job,"id"|"version"|"desired_action"> & {engagement_id:string;account_id:string|null};
export const membershipKey=(connectionId:string,groupId:string,accountId:string,source:string)=>JSON.stringify([connectionId,groupId,accountId,source]);
export const codeFor=(error:unknown)=>error instanceof BusinessRuleError?error.code:"DIRECTORY_RETRY_REQUIRED";
export const sameIds=(left:string[],right:string[])=>left.length===right.length&&left.every(id=>right.includes(id));
export const waitingKey=(row:Pick<Job,"company_id"|"engagement_id"|"position_id"|"rule_id"|"revision">)=>JSON.stringify([row.company_id,row.engagement_id,row.position_id,row.rule_id,row.revision]);
export function filters(inventory:DirectoryInventory,gpoId:string){
  const gpo=inventory.objects.find(o=>o.id===gpoId&&o.kind==="gpo");
  if(!gpo)throw new BusinessRuleError(409,"DIRECTORY_TARGET_INVALID","The mapped GPO is absent from this Directory.");
  return Array.isArray(gpo.properties.securityFilterGroupIds)?gpo.properties.securityFilterGroupIds as string[]:[];
}
export function links(inventory:DirectoryInventory,ouId:string){
  const ou=inventory.objects.find(o=>o.id===ouId&&o.kind==="ou");
  if(!ou)throw new BusinessRuleError(409,"DIRECTORY_TARGET_INVALID","The mapped OU is absent from this Directory.");
  return Array.isArray(ou.properties.gpoLinks)?ou.properties.gpoLinks as {gpoId:string;enabled?:boolean}[]:[];
}
