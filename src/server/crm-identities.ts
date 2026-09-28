import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {Row} from './commerce';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {assertion,executeCommand,replayCommand} from './commands';
import {ApiError} from './http';
import {hasPermission} from '@/lib/permissions';

const id=z.string().min(1), evidence=z.string().trim().min(3).max(2000);
const command=z.discriminatedUnion('action',[
 z.object({action:z.literal('create-account'),kind:z.enum(['business','individual']),name:z.string().trim().min(2).max(160)}),
 z.object({action:z.literal('create-contact'),accountId:id,name:z.string().trim().min(2).max(160),detail:evidence}),
 z.object({action:z.literal('link-finance'),accountId:id,customerId:id,expectedVersion:z.number().int().positive(),evidence}),
 z.object({action:z.literal('link-guest'),contactId:id,guestId:id,propertyId:id,evidence}),
]);

export async function crmIdentities(a:WorkContext){
 requirePermission(a,'commerce.read');
 const {write}=await holdingResources(a.holdingId);
 const [accounts,contacts,customers]=await Promise.all([
  write.prepare("SELECT a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN finance_parties p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY a.updated_at DESC LIMIT 200").bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare("SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.company_id=? AND (?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))) ORDER BY c.updated_at DESC LIMIT 300").bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>(),
  write.prepare("SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name").bind(a.companyId).all<Row>()
 ]);
 return {accounts:accounts.results,contacts:contacts.results,customers:customers.results,canReview:hasPermission(a,'commerce.approve'),canCreate:hasPermission(a,'commerce.sales')};
}

export async function changeCRMIdentity(a:WorkContext,raw:unknown,key:string){
 const i=command.parse(raw);requirePermission(a,'commerce.sales');
 const {write}=await holdingResources(a.holdingId);
 const recordId=i.action==='create-account'?`account-${key}`:i.action==='create-contact'?`contact-${key}`:i.action==='link-finance'?i.accountId:i.contactId;
 const operation=`crm.identity.${i.action}`,prior=await replayCommand(write,a,key,operation,recordId,i);
 if(prior)return prior;
 const now=Date.now(),steps=[];
 if(i.action==='create-account')steps.push(write.prepare('INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)').bind(recordId,a.companyId,i.kind,i.name,a.userId,now,now));
 if(i.action==='create-contact')steps.push(
  assertion(write,key,'account',"EXISTS(SELECT 1 FROM crm_accounts x WHERE x.id=? AND x.company_id=? AND (?::text IS NULL OR x.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=x.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))",[i.accountId,a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId]),
  write.prepare('INSERT INTO crm_contacts(id,company_id,account_id,name,detail,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)').bind(recordId,a.companyId,i.accountId,i.name,i.detail,a.userId,now,now));
 if(i.action==='link-finance'){
  requirePermission(a,'commerce.approve');
  steps.push(assertion(write,key,'account',"EXISTS(SELECT 1 FROM crm_accounts x WHERE x.id=? AND x.company_id=? AND x.version=? AND x.finance_party_id IS NULL AND x.created_by<>? AND (?::text IS NULL OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=x.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))",[i.accountId,a.companyId,i.expectedVersion,a.userId,a.branchId,a.branchId,a.departmentId,a.departmentId]),
   assertion(write,key,'customer',"EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')",[i.customerId,a.companyId]),
   write.prepare('UPDATE crm_accounts SET finance_party_id=?,review_evidence=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?').bind(i.customerId,i.evidence,a.userId,now,i.accountId));
 }
 if(i.action==='link-guest'){
  requirePermission(a,'commerce.approve');requirePermission(a,'pms.read');
  steps.push(assertion(write,key,'guest',"EXISTS(SELECT 1 FROM crm_contacts c JOIN pms_records g ON g.id=? AND g.kind='guest' JOIN pms_properties p ON p.id=g.property_id WHERE c.id=? AND c.company_id=? AND c.created_by<>? AND p.id=? AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?))))",[i.guestId,i.contactId,a.companyId,a.userId,i.propertyId,a.companyId,a.branchId,a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId]),
   write.prepare('INSERT INTO crm_guest_links(id,company_id,contact_id,guest_id,property_id,evidence,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?,?)').bind(`guest-link-${key}`,a.companyId,i.contactId,i.guestId,i.propertyId,i.evidence,a.userId,now));
 }
 const version=i.action==='link-finance'?i.expectedVersion+1:1;
 return executeCommand(write,a,key,operation,i,recordId,version,{id:recordId,version},steps);
}

export async function crmContactHistory(a:WorkContext,contactId:string){
 requirePermission(a,'commerce.read');requirePermission(a,'pms.read');
 const {write}=await holdingResources(a.holdingId);
 const contact=await write.prepare("SELECT c.id,c.name,c.account_id FROM crm_contacts c JOIN crm_accounts acc ON acc.id=c.account_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))").bind(contactId,a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).first<Row>();
 if(!contact)throw new ApiError(404,'CRM_CONTACT_NOT_FOUND','Contact not found.');
 const stays=(await write.prepare(`SELECT g.guest_id,r.id reservation_id,r.data_json::jsonb->>'state' reservation_state,
  s.id stay_id,s.data_json::jsonb->>'state' stay_state,s.data_json::jsonb->>'arrival' arrival,
  s.data_json::jsonb->>'departure' departure,p.id property_id
  FROM crm_guest_links g JOIN pms_properties p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN pms_records r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id
  LEFT JOIN pms_records s ON s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=r.id
  WHERE g.contact_id=? AND g.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY r.updated_at DESC,s.id`).bind(contactId,a.companyId,a.branchId,a.branchId).all<Row>()).results;
 return {contact,stays};
}
