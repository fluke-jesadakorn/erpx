import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {companyPurchaseReadLocks} from '../../commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {crmAttachmentCustomerSourceLocks,crmAttachmentCustomerSourceImage} from '../../finance/public/crm-attachment-sources';
import {crmAttachmentOrderSourceLocks,crmAttachmentOrderSourceImage} from '../../commerce/public/crm-attachment-sources';
import {crmAttachmentHotelSourceLocks,crmAttachmentHotelSourceImage} from '../../pms/public/crm-attachment-sources';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {attachmentDeniedReadAudits} from '../../files/public/attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '../../files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {crmPurchaseAccountRows,crmPurchaseOpportunityRows,crmPurchaseSourceReadLocks} from './purchases';

type Target='crm_account'|'crm_opportunity';
export function isCRMAttachmentReadTarget(target:string):target is Target{return target==='crm_account'||target==='crm_opportunity';}
type Source={image:string;customer_id:string|null;quote_id:string|null;sales_id:string|null;reservation_id:string|null};
async function image(database:SqlDatabase,companyId:string,target:Target,id:string){
 const row=target==='crm_account'?await database.prepare(`SELECT jsonb_build_object('account',to_jsonb(account),
  'contacts',(SELECT jsonb_agg(to_jsonb(contact) ORDER BY contact.id) FROM crm_contacts contact WHERE contact.account_id=account.id AND contact.company_id=account.company_id),
  'leads',(SELECT jsonb_agg(to_jsonb(lead) ORDER BY lead.id) FROM crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id),
  'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (${crmPurchaseAccountRows}) native WHERE native.id=account.id AND native.company_id=account.company_id))::text image,
  account.finance_party_id customer_id,NULL::text quote_id,NULL::text sales_id,NULL::text reservation_id FROM crm_accounts account WHERE account.id=? AND account.company_id=?`).bind(id,companyId).first<Source>()
 :await database.prepare(`SELECT jsonb_build_object('opportunity',to_jsonb(opportunity),'lead',to_jsonb(lead),'account',to_jsonb(account),'contact',to_jsonb(contact),
  'guestLinks',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id),
  'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (${crmPurchaseOpportunityRows}) native WHERE native.id=lead.id AND native.company_id=lead.company_id))::text image,
  opportunity.customer_id,opportunity.quote_id,opportunity.sales_id,opportunity.reservation_id FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id
  LEFT JOIN crm_accounts account ON account.id=lead.account_id AND account.company_id=lead.company_id
  LEFT JOIN crm_contacts contact ON contact.id=lead.contact_id AND contact.account_id=account.id AND contact.company_id=lead.company_id
  WHERE opportunity.id=? AND lead.company_id=?`).bind(id,companyId).first<Source>();
 if(!row)return null;
 return JSON.stringify([row.image,row.customer_id?await crmAttachmentCustomerSourceImage(database,companyId,row.customer_id):null,row.quote_id?await crmAttachmentOrderSourceImage(database,companyId,row.quote_id):null,row.sales_id?await crmAttachmentOrderSourceImage(database,companyId,row.sales_id):null,row.reservation_id?await crmAttachmentHotelSourceImage(database,companyId,row.reservation_id):null]);
}
function changed(){return new ApiError(409,'CRM_FILE_READ_CHANGED','The actual CRM identity, linked customer document, private evidence or selected authority changed during its read.');}
/** The complete original Files policy decides both CRM source ownership and
 * purchased purpose. Raw native images stay private and cannot grant access. */
export async function withCRMAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(!isCRMAttachmentReadTarget(selected.targetType))throw changed();const target=selected.targetType;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'CRM_FILE_READ_TRANSACTION_REQUIRED','Private CRM bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([crmAttachmentCustomerSourceLocks(database),crmAttachmentOrderSourceLocks(database),crmAttachmentHotelSourceLocks(database),...crmPurchaseSourceReadLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();const before=await image(database,actor.companyId,target,source.targetId);
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first(),deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{if(JSON.stringify(actor)!==context)throw changed();if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}else await requirePrivateAttachmentReaderCurrent(database,actor);if(authorizeSource){if(!before)throw changed();await authorize(database,source);}const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);if(after?.witness!==source.witness||await image(database,actor.companyId,target,source.targetId)!==before)throw changed();};
  let result:Result;try{result=await read(database);}catch(error){if(!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;await flushFinanceDeferredEffects(database);await check(false,true);if((await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id)?.witness!==denied[0].witness)throw changed();return {kind:'denied',error};}
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
