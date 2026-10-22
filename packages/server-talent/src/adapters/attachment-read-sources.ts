import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {talentHirePeopleSources,talentHirePeopleSourceReadLocks} from '@erpx/server-people_payroll/public/hire-purchase-sources';

export function isTalentAttachmentReadTarget(target:string){return target==='talent_candidate'||target==='talent_offer';}
export function talentAttachmentReadSourceLocks(database:SqlDatabase){return [
 database.prepare('LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts,talent_candidate_privacy_events IN SHARE ROW EXCLUSIVE MODE'),
 ...talentHirePeopleSourceReadLocks(database),
];}
const applicationImages=`SELECT a.id,a.candidate_id,a.company_id,jsonb_build_object('application',to_jsonb(a),'requisition',to_jsonb(r),
 'proposals',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM talent_proposals p WHERE p.application_id=a.id),
 'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM talent_offers o WHERE o.application_id=a.id AND o.company_id=a.company_id),
 'handoffs',(SELECT jsonb_agg(jsonb_build_object('handoff',to_jsonb(h),'receipt',to_jsonb(receipt),'people',hired.image) ORDER BY h.id) FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=h.id AND receipt.company_id=h.company_id LEFT JOIN (${talentHirePeopleSources}) hired ON hired.handoff_id=h.id AND hired.company_id=h.company_id WHERE o.application_id=a.id AND o.company_id=a.company_id)) image
 FROM talent_applications a LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id`;
/** Current original application ownership, branch/department, candidate
 * privacy and exact accepted-hire delivery remain private server witnesses. */
export function talentAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:string,id:string){
 if(!isTalentAttachmentReadTarget(target))throw new Error('TALENT_FILE_SOURCE_TARGET_REQUIRED');
 const candidate=target==='talent_candidate'?'c.id=?':'EXISTS(SELECT 1 FROM talent_offers selected JOIN talent_applications a ON a.id=selected.application_id AND a.company_id=selected.company_id WHERE selected.id=? AND selected.company_id=c.company_id AND a.candidate_id=c.id)';
 return database.prepare(`SELECT jsonb_build_object('candidate',to_jsonb(c),
 'privacy',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM talent_candidate_privacy_events event WHERE event.candidate_id=c.id AND event.company_id=c.company_id),
 'applications',(SELECT jsonb_agg(a.image ORDER BY a.id) FROM (${applicationImages}) a WHERE a.candidate_id=c.id AND a.company_id=c.company_id))::text image
 FROM talent_candidates c WHERE ${candidate} AND c.company_id=?`).bind(id,companyId).first<string>('image');
}
