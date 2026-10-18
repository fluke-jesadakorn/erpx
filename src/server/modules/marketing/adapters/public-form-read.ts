import 'server-only';
import {createHash} from 'node:crypto';
import {ApiError} from '../../runtime/public/http';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {marketingCreativeSourceReadLocks,marketingCreativeSourceImage} from '../../files/public/marketing-creative';
import {marketingPresentationReferences} from '../domain/presentation';

async function image(database:SqlDatabase,id:string,submissionKey?:string){
 const form=await database.prepare("SELECT company_id,status,configuration_json,to_jsonb(form)::text image FROM marketing_assets form WHERE id=? AND kind='form'").bind(id).first<{company_id:string;status:string;configuration_json:string;image:string}>();
 if(!form)return null;
 // Inactive/non-consented forms retain the original reader's 404. There is
 // no reason to resolve a retired form's supporting presentation first.
 if(form.status!=='active'||JSON.parse(form.configuration_json).consentRequired!==true)return createHash('sha256').update(form.image).digest('hex');
 const refs=marketingPresentationReferences(form.configuration_json),content=refs.contentAssetId?await database.prepare("SELECT to_jsonb(content)::text image FROM marketing_assets content WHERE id=? AND company_id=? AND kind='content'").bind(refs.contentAssetId,form.company_id).first<string>('image'):null,
 creative=refs.mediaPublicationId?await marketingCreativeSourceImage(database,form.company_id,refs.mediaPublicationId,refs.contentAssetId!):null,
 receipt=submissionKey?await database.prepare(`SELECT jsonb_build_array(to_jsonb(receipt),to_jsonb(candidate),to_jsonb(activated),to_jsonb(submission))::text image FROM marketing_public_receipts receipt
 JOIN marketing_lead_candidates candidate ON candidate.id=receipt.candidate_id AND candidate.company_id=? AND candidate.form_id=?
 JOIN marketing_asset_events activated ON activated.asset_id=candidate.form_id AND activated.version=candidate.form_version AND activated.action='activate' AND activated.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE receipt.key=?`).bind(form.company_id,id,submissionKey).first<string>('image'):null;
 return createHash('sha256').update(JSON.stringify([form.image,content,creative,receipt])).digest('hex');
}
/** Anonymous tenant reads have no staff role to inherit. Own a fresh source
 * snapshot, keep exact consent/publication/receipt rules in the original
 * reader, then recheck after all effects before exposing the presentation. */
export async function withPublicMarketingFormRead<Result>(owner:SqlDatabase,id:string,submissionKey:string|undefined,read:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'FORM_READ_TRANSACTION_REQUIRED','Public form presentation requires its fresh owned read transaction.');
 return owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([database.prepare('LOCK TABLE marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_public_receipts IN SHARE MODE'),...marketingCreativeSourceReadLocks(database)]);
  await lockScheduledFeatureSources(database);
  const before=await image(database,id,submissionKey),result=await read(database);
  await flushFinanceDeferredEffects(database);
  if(await image(database,id,submissionKey)!==before)throw new ApiError(409,'FORM_READ_CHANGED','The approved form or public creative changed during its presentation. Reload the form.');
  if(JSON.stringify(await read(database))!==JSON.stringify(result))throw new ApiError(409,'FORM_READ_CHANGED','The approved form presentation changed. Reload the form.');
  return result;
 });
}
