import {describe,it,expect,vi} from 'vitest';
vi.mock('next/headers',()=>({headers:async()=>new Headers()}));
import {
  createNativePmsClient,
  nativePmsConfiguration,
  NativePmsError,
  type NativePmsPageContext,
} from '../../../packages/feature-pms/src/adapters/server/native-pms';
import {nativePmsAvailable} from '../../../packages/workplace/src/lib/native-pms';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44994',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativePmsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'pms_housekeeping',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['pms'],
  iam:{permissions:['pms.read','pms.housekeeping','records.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'pms_housekeeping',assignmentScopeKind:'branch',
  modules:['pms'],permissions:['pms.read','pms.housekeeping','records.read'],
  expiresAt:9_007_199_254_740_991};
const propertyRow={id:'property-1',name:'Riverside',branchId:'branch',state:'open',businessDate:'2026-11-04'};
const room={id:'room-1',kind:'room',version:1,updatedAt:1,data:{number:'101',typeId:'type-1',building:'Main',floor:'1',active:true,cleanliness:'dirty',dnd:true}};
const listMeta={roomSummary:{total:1,active:1,dirty:1,clean:0,inspected:0,dnd:1,occupied:1},
  section:'rooms',kind:'room',ids:['room-1'],from:'2026-10-05',to:'2027-01-03',page:0,q:'',status:'',
  building:'',floor:'',typeId:'',hasNext:false};
const workspace={
  properties:[propertyRow],
  snapshot:{property:{id:'property-1',companyId:'company',branchId:'branch',name:'Riverside',
    businessDate:'2026-11-04',state:'open',version:1,config:{currency:'THB',minorUnit:2}},records:[room],listMeta},
  defaultDate:'2026-11-04',readiness:[],canReviewBookingMedia:false,
  bookingReadiness:{email:false,accounting:true,onlineAccounting:false,fees:false,connect:false},
  canManageSetup:false,permissions:['pms.read','pms.housekeeping'],availablePaymentMethods:['cash'],
  userId:'user',branches:[],staff:[{id:'staff-1',name:'Synthetic Worker'}],accounts:[],charts:[],parties:[],
  products:[],warehouses:[],documents:[],transferRequests:[],chargeCorrections:[],
  stripeConnectionReady:false,stripePayments:[],counterRefunds:[],bookingRefunds:[],bookingDelivery:[],
  ledger:null,controlBalances:null,dayControls:null,
};
const readInput={holdingSlug:'acme',section:'rooms' as const,property:'property-1',status:'dirty',
  q:'101',page:1,branchSlug:'north',expected:{companyId:'company',assignmentId:'assignment',
  revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const failed=(status:number,code:string)=>new Response(JSON.stringify({error:{code,message:'Synthetic failure'}}),{status,headers:{'content-type':'application/json'}});

describe('native PMS room-board transport',()=>{
 it('forwards the registered Holding, session, selected scope and CAS to the room board',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativePmsClient(JSON.parse(config()),transport);
  const result=await client.roomBoard(incoming(),actor,readInput);
  expect(result.snapshot?.property.id).toBe('property-1');
  expect(result.staff[0]).toEqual({id:'staff-1',name:'Synthetic Worker'});
  const [method,url,requestHeaders,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/pms/rooms?section=rooms&property=property-1&status=dirty&q=101&page=1');
  expect(requestHeaders.get('cookie')).toBe('__Host-erpx-product=session');
  expect(requestHeaders.get('x-erpx-workspace-branch')).toBe('north');
  expect(requestHeaders.get('x-erpx-context-company')).toBe('company');
  expect(requestHeaders.get('x-erpx-context-assignment')).toBe('assignment');
  expect(requestHeaders.get('x-erpx-context-revision')).toBe('7');
  expect(requestHeaders.get('x-erpx-context-assignment-version')).toBe('3');
  expect(body).toBeUndefined();
 });

 it('fails closed when authority is absent or the selected context changed',async()=>{
  const absent=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativePmsClient(JSON.parse(config()),absent);
  await expect(client.roomBoard(incoming(),{...actor,authority:undefined},readInput)).rejects.toMatchObject({code:'NATIVE_PMS_CONTEXT_CHANGED'});
  expect(absent).not.toHaveBeenCalled();
  const changed=vi.fn().mockResolvedValue(ok({data:workspace,context:{...context,revision:8}}));
  const changedClient=createNativePmsClient(JSON.parse(config()),changed);
  await expect(changedClient.roomBoard(incoming(),actor,readInput)).rejects.toMatchObject({code:'NATIVE_PMS_CONTEXT_CHANGED'});
 });

 it('rejects foreign properties, unrequested permissions and malformed workspaces',async()=>{
  const foreign=structuredClone(workspace);
  foreign.snapshot.property.companyId='other';
  const untrusted=structuredClone(workspace);
  untrusted.permissions.push('pms.reports');
  const malformed=structuredClone(workspace);
  delete (malformed as {defaultDate?:string}).defaultDate;
  const duplicate=structuredClone(workspace);
  duplicate.snapshot.records=[room,room];
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:foreign,context}))
    .mockResolvedValueOnce(ok({data:untrusted,context}))
    .mockResolvedValueOnce(ok({data:malformed,context}))
    .mockResolvedValueOnce(ok({data:duplicate,context}));
  const client=createNativePmsClient(JSON.parse(config()),transport);
  for(let index=0;index<4;index++)await expect(client.roomBoard(incoming(),actor,readInput)).rejects.toBeInstanceOf(NativePmsError);
 });

 it('maps bounded owner errors without falling back',async()=>{
  const transport=vi.fn().mockResolvedValue(failed(409,'PMS_RECORD_CHANGED'));
  const client=createNativePmsClient(JSON.parse(config()),transport);
  await expect(client.roomBoard(incoming(),actor,readInput)).rejects.toMatchObject({
    code:'PMS_RECORD_CHANGED',status:409,message:'Synthetic failure',
  });
 });

 it('keeps the Next reader fail-closed unless the native configuration is present',()=>{
  const previous=process.env.ERPX_NATIVE_PMS_CONFIG;
  delete process.env.ERPX_NATIVE_PMS_CONFIG;
  expect(nativePmsAvailable()).toBe(false);
  process.env.ERPX_NATIVE_PMS_CONFIG=config();
  expect(nativePmsAvailable()).toBe(true);
  if(previous===undefined)delete process.env.ERPX_NATIVE_PMS_CONFIG;
  else process.env.ERPX_NATIVE_PMS_CONFIG=previous;
 });

 it('rejects configuration that can escape the registered Holding origin',()=>{
  for(const raw of [
    config({privateBaseUrl:'http://127.0.0.1:3100'}),
    config({privateBaseUrl:'http://10.0.0.1:44994'}),
    config({holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}]}),
    config({holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'},{origin:'https://acme.example.com',holdingSlug:'other',holdingId:'h002'}]}),
    config({timeoutMs:100}),
  ]) {
    expect(()=>createNativePmsClient(JSON.parse(raw))).toThrowError(NativePmsError);
  }
  for(const raw of ['not-json',JSON.stringify({privateBaseUrl:'http://127.0.0.1:44994'})])
    expect(()=>nativePmsConfiguration(raw)).toThrowError(NativePmsError);
 });
});
