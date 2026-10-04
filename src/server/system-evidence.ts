import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {iamActorFromWork,assertCompanyScope} from './iam-context';
import {iamCommand} from './iam-transactions';
import {requireFreshMfa} from './iam-mfa';
import {transactionDatabase,type PostgresDatabase} from './runtime/postgres';
import {ApiError} from './http';
import {prepareAttachmentBinding} from './attachments/service';

export const systemEvidenceInput=z.object({
 expectedVersion:z.number().int().positive(),
 attachmentSetIds:z.array(z.string().min(1)).min(1).max(20),
 reason:z.string().trim().min(3).max(2000),
}).strict();

const owners={
 iam_review:{table:'iam_reviews',purpose:'sanitized_review_evidence',permission:'iam.manage'},
 iam_migration:{table:'iam_migrations',purpose:'migration_evidence',permission:'iam.manage'},
 people_provision_operation:{table:'people_provision_operations',purpose:'sanitized_provisioning_evidence',permission:'iam.directory.apply'},
} as const;

/** Sanitized artifacts supplement the native owner's history. They do not
 * approve access, run a migration or advance a provisioning decision. */
async function bindSystemEvidence(db:PostgresDatabase,work:WorkContext,targetType:keyof typeof owners,id:string,raw:unknown,key:string){
 const input=systemEvidenceInput.parse(raw),actor=iamActorFromWork(work),owner=owners[targetType];
 assertCompanyScope(actor);await requireFreshMfa(actor);
 return iamCommand(db,actor,key,`${targetType}.evidence.bind`,{id,...input},owner.permission,async client=>{
  const row=(await client.query(`SELECT version,status FROM ${owner.table} WHERE id=$1 AND company_id=$2 FOR UPDATE`,[id,actor.companyId])).rows[0];
  if(!row)throw new ApiError(404,'SYSTEM_EVIDENCE_OWNER_NOT_FOUND','The evidence owner is unavailable in this Company.');
  if(Number(row.version)!==input.expectedVersion)throw new ApiError(409,'STALE_VERSION','Reload the owner before binding its sanitized evidence.');
  const tx=transactionDatabase(client),version=input.expectedVersion+1,prepared=await prepareAttachmentBinding(tx,work,{
   attachmentSetIds:input.attachmentSetIds,
   bindingContract:{targetType,bindingType:`${targetType}_evidence`,contexts:[{purposeKey:owner.purpose,componentKind:'action',componentRef:`${targetType}:evidence`}]},
   targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${id}:evidence:${version}`,
  });
  await tx.batch([...prepared.guards,...prepared.steps]);
  await client.query(`UPDATE ${owner.table} SET version=version+1${targetType==='people_provision_operation'?',updated_at=$3':''} WHERE id=$1 AND company_id=$2`,targetType==='people_provision_operation'?[id,actor.companyId,Date.now()]:[id,actor.companyId]);
  return {id,version,status:row.status,manifest:prepared.manifest};
 });
}

export const bindIamReviewEvidence=(db:PostgresDatabase,actor:WorkContext,id:string,raw:unknown,key:string)=>bindSystemEvidence(db,actor,'iam_review',id,raw,key);
export const bindIamMigrationEvidence=(db:PostgresDatabase,actor:WorkContext,id:string,raw:unknown,key:string)=>bindSystemEvidence(db,actor,'iam_migration',id,raw,key);
export const bindProvisioningEvidence=(db:PostgresDatabase,actor:WorkContext,id:string,raw:unknown,key:string)=>bindSystemEvidence(db,actor,'people_provision_operation',id,raw,key);
