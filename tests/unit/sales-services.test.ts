import {describe,expect,test} from 'vitest';
import {createPriceListApplication,type PriceListRepository,type PriceListMutation} from '../../src/server/modules/sales/application/price-lists';
import {createFollowupApplication,type FollowupRepository,type FollowupMutation} from '../../src/server/modules/sales/application/followups';
import {createCustomerNoteApplication,type CustomerNoteRepository} from '../../src/server/modules/sales/application/notes';
import {createSalesCustomerApplication,type SalesCustomerRepository} from '../../src/server/modules/sales/application/customers';
import {createCustomerNotificationApplication,type CustomerNotificationRepository,type CustomerNotificationMutation} from '../../src/server/modules/sales/application/notifications';
import {createCustomerDeliveryApplication,type CustomerDeliveryRepository} from '../../src/server/modules/sales/application/notification-delivery';
import {customerNotificationRetryAt,type CustomerMail} from '../../src/server/modules/sales/domain/notifications';
import type {WorkItem} from '../../src/lib/work-item';

const priceList={name:'Current customer prices',currency:'THB',customerId:null,validFrom:'2026-10-01',validUntil:'2026-10-31',maxAutoDiscountBps:500,items:[{productId:'product',price:'10.25',taxBps:700}]};
function pricingFixture(overrides:Partial<PriceListRepository>={}){
 const mutations:PriceListMutation[]=[],calls:string[]=[];
 const repository:PriceListRepository={async workspace(){return {lists:[],items:[],products:[],customers:[],settings:{currency:'THB'}};},async replay(){calls.push('receipt');return null;},async list(){return {status:'draft',prepared_by:'author'};},async currency(){return {currency:'THB'};},async customer(){return true;},async product(){return {id:'product',version:4};},async commit(m){mutations.push(m);return m.result;},...overrides};
 const ports={authorize(){calls.push('access');},authorizeReview(){calls.push('review');},companyScope:true,canApprove:true,isSamePerson:(id:string|number|null)=>id==='reviewer',clock:{now:()=>42},async repository(){return repository;}};
 return {mutations,calls,repository,ports,app:createPriceListApplication(ports)};
}
describe('Sales price-list semantic ports',()=>{
 test('Company scope and date validity precede receipt lookup',async()=>{
  const f=pricingFixture();f.ports.companyScope=false;await expect(f.app.create(priceList,'key')).rejects.toMatchObject({code:'COMPANY_PRICING_SCOPE_REQUIRED'});expect(f.calls).toEqual(['access']);
  f.ports.companyScope=true;await expect(f.app.create({...priceList,validUntil:'2026-09-30'},'key')).rejects.toMatchObject({code:'PRICE_LIST_DATES_INVALID'});expect(f.calls).toEqual(['access','access']);expect(f.mutations).toEqual([]);
 });
 test('replay avoids reading a now-inactive product and a replacement currency',async()=>{
  const f=pricingFixture({async replay(){return {id:'saved',version:1};},async currency(){throw new Error('replacement source must not be read');},async product(){throw new Error('inactive product must not be read');}});
  expect(await f.app.create(priceList,'key')).toEqual({id:'saved',version:1});expect(f.mutations).toEqual([]);
 });
 test('current product versions and minor-unit values accompany a draft command',async()=>{
  const f=pricingFixture();await f.app.create(priceList,'key');expect(f.mutations[0]).toMatchObject({action:'create',id:'price-list-key',operation:'sales.price-list.create',now:42,products:[{id:'product',version:4}],prices:[1025],result:{status:'draft',version:1}});
  f.repository.currency=async()=>({currency:'USD'});await expect(f.app.create(priceList,'other')).rejects.toMatchObject({code:'PRICE_LIST_CURRENCY_INVALID'});expect(f.mutations).toHaveLength(1);
 });
 test('the person who prepared the current revision cannot activate it',async()=>{
  const f=pricingFixture({async list(){return {status:'draft',prepared_by:'reviewer',created_by:'another-account'};}});
  await expect(f.app.decide('list',{action:'activate',expectedVersion:2,reason:'Review this price control'},'key')).rejects.toMatchObject({code:'INDEPENDENT_PRICING_APPROVER_REQUIRED'});expect(f.mutations).toEqual([]);
 });
});
function followupFixture(overrides:Partial<FollowupRepository>={}){
 const mutations:FollowupMutation[]=[];const repository:FollowupRepository={async rows(){return [{id:'followup',order_id:'order',owner_id:'assigned',owner_eligible:1,status:'open',due_date:'2026-10-06'}];},async evidence(){return null;},async recipients(){return [];},async history(){return [];},async order(){return {id:'order',kind:'sales',branch_id:'branch',department_id:null};},async replay(){return null;},async commit(m){mutations.push(m);return m.result;},...overrides};
 const ports={actorId:'manager',canRead:true,canSell:true,canManage:true,authorize(){},clock:{now:()=>42},async repository(){return repository;}};return {repository,ports,mutations,app:createFollowupApplication(ports)};
}
describe('Sales follow-up and note semantics',()=>{
 test('a sales manager can reassign but cannot complete another assigned owner contact',async()=>{
  const f=followupFixture();await expect(f.app.change('followup',{action:'complete',expectedVersion:1,reason:'Customer contacted'},'key')).rejects.toMatchObject({code:'CUSTOMER_FOLLOWUP_OWNER_REQUIRED'});
  await f.app.change('followup',{action:'reassign',expectedVersion:1,ownerId:'replacement',dueDate:'2026-10-08',reason:'Current owner is unavailable'},'other');expect(f.mutations[0]).toMatchObject({owner:'replacement',due:'2026-10-08',status:'open'});
 });
 test('revoked recipient facts make work wait for reassignment',async()=>{
  const f=followupFixture({async rows(){return [{id:'followup',owner_id:'manager',owner_eligible:0,status:'open',due_date:'2026-10-06',updated_at:42}];}});
  expect(await f.app.work()).toEqual([expect.objectContaining({canAct:false,queue:'waiting',blocker:'Assigned owner is no longer eligible; a sales manager must reassign with a reason'})]);
 });
 test('a future contact time is rejected before receipt lookup or source access',async()=>{
  const repository:CustomerNoteRepository={async order(){throw new Error('source unavailable');},async timeline(){throw new Error('source unavailable');},async customer(){return null;},async notes(){return [];},async history(){return [];},async note(){return null;},async replay(){throw new Error('must not replay');},async commit(){throw new Error('must not commit');}};
  const app=createCustomerNoteApplication({canSell:true,canManage:false,branchId:null,departmentId:null,authorize(){},isSamePerson:()=>false,clock:{now:()=>42},async repository(){return repository;}});
  await expect(app.create({orderId:'order',expectedVersion:1,channel:'phone',occurredAt:60043,body:'Discussed the delivery'},'key')).rejects.toMatchObject({code:'CONTACT_TIME_INVALID'});
 });
 test('standalone customer notes stay hidden from Branch and Department actors',async()=>{
  const repository:CustomerNoteRepository={async order(){return {};},async timeline(){throw new Error('hidden source must not be read');},async customer(){return null;},async notes(){return [];},async history(){return [];},async note(){return {order_id:null,customer_id:'customer',created_by:'author',body:'Contact record'};},async replay(){return null;},async commit(){throw new Error('must not commit');}};
  const app=createCustomerNoteApplication({canSell:true,canManage:true,branchId:'branch',departmentId:null,authorize(){},isSamePerson:()=>true,clock:{now:()=>42},async repository(){return repository;}});
  await expect(app.change('note',{action:'archive',expectedVersion:1,reason:'Recorded separately'},'key')).rejects.toMatchObject({status:404,code:'CUSTOMER_NOTE_NOT_FOUND'});
 });
});
function hubFixture(overrides:Partial<SalesCustomerRepository>={}){
 const repository:SalesCustomerRepository={async orderCounts(){return [];},async followupHeads(){return [];},async opportunityCounts(){return [];},async supportCounts(){return [];},async directory(){return {items:[],nextCursor:null,previousCursor:null,hasMore:false};},async customer(){return null;},async orders(){return [];},async followups(){return [];},async opportunities(){return [];},async cases(){return [];},async supportSource(){return null;},async timeline(){throw new Error('missing timeline');},async notes(){throw new Error('missing notes');},async reviews(){return [];},async crmWork(){return [];},async followupWork(){return [];},async orderWork(){return [];},async followupIds(){return [];},...overrides};
 const access={today:true,pipeline:true,documents:true,pricing:false,reports:true,directory:true,contacts:true,support:true,reviews:false};const ports={authorize(){},async access(){return access;},canReadSupport:false,canCreateSupport:false,canSell:true,timelineUnavailable:()=>false,clock:{now:()=>42},async repository(){return repository;}};
 return {access,ports,repository,app:createSalesCustomerApplication(ports)};
}
describe('Sales customer projections',()=>{
 test('restricted foreign projections remain null and are never loaded',async()=>{
  const f=hubFixture({async opportunityCounts(){throw new Error('CRM denied');},async supportCounts(){throw new Error('Support denied');}});f.access.pipeline=false;
  expect((await f.app.summaries(['customer'])).get('customer')).toEqual({nextFollowup:null,openOrders:0,openOpportunities:null,openSupport:null});
 });
 test('source failure is reported while available work remains sorted and visible',async()=>{
  const work=(id:string,dueAt:number):WorkItem=>({id,type:'customer_followup',title:id,detail:'Contact customer',href:'/',status:'open',updatedAt:42,canAct:true,queue:'todo',dueAt,relationships:['assigned']});
  const f=hubFixture({async crmWork(){throw new Error('CRM temporarily unavailable');},async followupWork(){return [work('later',100),work('earlier',10)];}});
  const result=await f.app.today({});expect(result.unavailable).toEqual(['CRM activities']);expect(result.items.map(x=>x.id)).toEqual(['earlier','later']);expect(result.observedAt).toBe(42);
 });
 test('a specifically denied task kind fails before loading its source',async()=>{
  const f=hubFixture({async crmWork(){throw new Error('source denied');}});f.access.pipeline=false;
  await expect(f.app.today({kind:'crm'})).rejects.toMatchObject({code:'SALES_TASK_ACCESS_DENIED'});
 });
});
const mail:CustomerMail={id:'mail',company_id:'company',fulfillment_id:'fulfillment',order_id:'order',party_id:'customer',party_version:1,recipient_email:'customer@example.invalid',subject:'Delivery recorded',body_text:'Immutable delivery snapshot',adapter:'resend',attempts:1};
function deliveryFixture(overrides:Partial<CustomerDeliveryRepository>={}){
 const calls:string[]=[],failures:{retryAt:number;code:string}[]=[];const repository:CustomerDeliveryRepository={async lease(){return mail;},async current(){return {status:'active',notification_email:mail.recipient_email,notification_consent:'granted',adapter:'resend'};},async cancel(_mail,_lease,code){calls.push(code);},async sent(){calls.push('sent');},async failed(_mail,_lease,retryAt,code){failures.push({retryAt,code});},async next(){return null;},...overrides};
 const ports={repository,async send(message:CustomerMail){calls.push('send');expect(message.body_text).toBe(mail.body_text);return 'provider-id';}};return {repository,ports,calls,failures,app:createCustomerDeliveryApplication(ports)};
}
describe('Sales customer notification orchestration',()=>{
 test('current consent revocation cancels a leased message before contacting its provider',async()=>{
  const f=deliveryFixture({async current(){return {status:'active',notification_email:mail.recipient_email,notification_consent:'revoked',adapter:'resend'};}});
  await f.app.deliver('mail',100);expect(f.calls).toEqual(['CUSTOMER_CONSENT_OR_EMAIL_CHANGED']);expect(f.failures).toEqual([]);
 });
 test('provider failures schedule the immutable message with capped deterministic backoff',async()=>{
  const f=deliveryFixture();f.ports.send=async()=>{throw new Error('CUSTOMER_EMAIL_DELIVERY_FAILED');};await expect(f.app.deliver('mail',100)).rejects.toThrow('CUSTOMER_EMAIL_DELIVERY_FAILED');
  expect(f.failures).toEqual([{retryAt:2100,code:'CUSTOMER_EMAIL_DELIVERY_FAILED'}]);expect(customerNotificationRetryAt(100,20)).toBe(1024100);
 });
 test('drain bounds repeated provider failures to ten leased jobs',async()=>{
  let leased=0;const f=deliveryFixture({async next(){return 'mail';},async lease(){leased++;return mail;}});f.ports.send=async()=>{throw new Error('provider unavailable');};await f.app.drain(100);expect(leased).toBe(10);expect(f.failures).toHaveLength(10);
 });
 test('queue replay returns before reading a customer whose consent has changed',async()=>{
  const mutations:CustomerNotificationMutation[]=[];const repository:CustomerNotificationRepository={async productionEnabled(){},async settings(){return null;},async delivery(){throw new Error('changed customer must not be read');},async existing(){return null;},async lines(){return [];},async notification(){return null;},async replay(){return {id:'saved',status:'pending',version:1};},async commit(m){mutations.push(m);return m.result;}};
  const app=createCustomerNotificationApplication({companyId:'company',authorize(){},clock:{now:()=>42},identifiers:{next:()=>{throw new Error('must not allocate another id');}},async repository(){return repository;}});
  expect(await app.queue('delivery',{expectedVersion:1,note:'Record the carrier confirmation'},'key')).toEqual({id:'saved',status:'pending',version:1});expect(mutations).toEqual([]);
 });
});
