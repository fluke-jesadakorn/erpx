import {describe,it,expect,vi} from 'vitest';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {createHash} from 'node:crypto';
import {createFieldFamilyBridge,nativeFieldFamilyVisible} from '../../../packages/workplace/src/lib/native-field-family';
const actor={holdingSlug:'acme',holdingId:'h',companyId:'c',assignmentId:'a',authority:{revision:1,assignmentVersion:2},iam:{},scope:{kind:'holding'}} as unknown as WorkContext;
const config=JSON.stringify({privateBaseUrl:'http://127.0.0.1:44817',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h',fieldFamily:true}]});
const req=()=>new Request('https://acme.example.com/api',{headers:{host:'acme.example.com',cookie:'test=one','x-erpx-context-company':'c','x-erpx-context-assignment':'a','x-erpx-context-revision':'1','x-erpx-context-assignment-version':'2'}});
const json=(body:unknown,status=201)=>({status,headers:new Headers({'content-type':'application/json'}),body:new TextEncoder().encode(JSON.stringify(body))});
describe('Field family bridge',()=>{
 it('forwards a reservation and rewrites only its matching private path',async()=>{
  const transport=vi.fn().mockResolvedValue(json({data:{uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'/api/v1/holdings/acme/field-service/uploads/i/content',expiresAt:100}}));
  const response=await createFieldFamilyBridge(config,transport)(req(),actor,{route:{kind:'reserve',setId:'s'},body:new TextEncoder().encode('{}'),contentType:'application/json',key:'original'});
  expect((await response.json()).data.uploadUrl).toBe('/api/v1/holdings/acme/uploads/i/content');expect(transport.mock.calls[0][0].pathname).toBe('/api/v1/holdings/acme/field-service/attachment-sets/s/files');expect(transport.mock.calls[0][2].get('idempotency-key')).toBe('original');
 });
 it('fails closed for missing family configuration and external upload paths',async()=>{
  const bridge=createFieldFamilyBridge(config,vi.fn().mockResolvedValue(json({data:{uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'https://evil.invalid/',expiresAt:100}})));
  await expect(bridge(req(),actor,{route:{kind:'reserve',setId:'s'}})).rejects.toMatchObject({status:503});
  await expect(createFieldFamilyBridge(config.replace('"fieldFamily":true','"fieldFamily":false'))(req(),actor,{route:{kind:'status',linkId:'l'}})).rejects.toMatchObject({status:503});
 });
 it('validates content bytes against the owner checksum and rejects a stale context',async()=>{
  const bytes=new TextEncoder().encode('safe'),hash=createHash('sha256').update(bytes).digest('hex');
  const transport=vi.fn().mockResolvedValue({status:200,headers:new Headers({'content-type':'image/webp',"content-disposition":"inline; filename*=UTF-8''safe.png",'x-erpx-checksum-sha256':hash}),body:bytes});
  const bridge=createFieldFamilyBridge(config,transport),response=await bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'preview'}});
  expect(await response.text()).toBe('safe');expect(response.headers.get('content-security-policy')).toContain('sandbox');
  transport.mockResolvedValueOnce({status:200,headers:new Headers({'content-type':'image/webp','content-disposition':'inline','x-erpx-checksum-sha256':'0'.repeat(64)}),body:bytes});
  await expect(bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'preview'}})).rejects.toMatchObject({status:503});
  const stale=req();stale.headers.set('x-erpx-context-company','foreign');await expect(bridge(stale,actor,{route:{kind:'status',linkId:'l'}})).rejects.toMatchObject({status:409});
 });
 it('rejects malformed status, unsafe integers and unrelated set receipts',async()=>{
  const status={linkId:'l',setId:'s',sourceId:null,sourceState:'available',linkState:'draft',scan:{attempts:1,lastResult:null,lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null},previewAvailable:false,bindable:true,expiresAt:100,boundAt:null,withdrawnAt:null};
  const transport=vi.fn().mockResolvedValue(json({data:status},200)),bridge=createFieldFamilyBridge(config,transport);
  expect((await bridge(req(),actor,{route:{kind:'status',linkId:'l'}})).status).toBe(200);
  for(const bad of [{...status,sourceState:'unknown'},{...status,expiresAt:Number.MAX_SAFE_INTEGER+1},{...status,scan:{attempts:1}}]){transport.mockResolvedValueOnce(json({data:bad},200));await expect(bridge(req(),actor,{route:{kind:'status',linkId:'l'}})).rejects.toMatchObject({status:503});}
  const set={id:'s',targetType:'field_service_visit',targetId:'order',componentKind:'record',componentRef:null,purposeKey:'site_photo',expectedTargetVersion:1,policyVersion:1,classification:'internal',requiredCount:0,maxFiles:10,state:'draft',createdAt:1,updatedAt:1,links:[]};
  const input=new TextEncoder().encode(JSON.stringify({targetType:'field_service_visit',targetId:'order',componentKind:'record',purposeKey:'site_photo',expectedTargetVersion:1}));
  transport.mockResolvedValueOnce(json({data:set},201));expect((await bridge(req(),actor,{route:{kind:'create'},body:input})).status).toBe(201);
  for(const bad of [{...set,targetId:'another'},{...set,purposeKey:'unregistered'},{...set,expectedTargetVersion:Number.MAX_SAFE_INTEGER+1}]){transport.mockResolvedValueOnce(json({data:bad},201));await expect(bridge(req(),actor,{route:{kind:'create'},body:input})).rejects.toMatchObject({status:503});}
 });
 it('rejects unsafe content types and dispositions even with correct checksum',async()=>{
  const bytes=new TextEncoder().encode('safe'),hash=createHash('sha256').update(bytes).digest('hex'),transport=vi.fn();const bridge=createFieldFamilyBridge(config,transport);
  for(const [type,disposition] of [['text/html','inline; filename="a.html"'],['image/webp','attachment; filename="a.webp"'],['image/webp','inline; filename="a"; extra=yes']]){transport.mockResolvedValueOnce({status:200,headers:new Headers({'content-type':type,'content-disposition':disposition,'x-erpx-checksum-sha256':hash}),body:bytes});await expect(bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'preview'}})).rejects.toMatchObject({status:503});}
 });
 it('rejects invalid reservation and upload receipts',async()=>{
  const transport=vi.fn(),bridge=createFieldFamilyBridge(config,transport);
  transport.mockResolvedValueOnce(json({data:{uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'/api/v1/holdings/acme/field-service/uploads/i/content',expiresAt:Number.MAX_SAFE_INTEGER+1}},201));
  await expect(bridge(req(),actor,{route:{kind:'reserve',setId:'s'}})).rejects.toMatchObject({status:503});
  for(const data of [{uploadIntentId:'other',attachmentLinkId:'l',sourceId:'s',state:'quarantined',status:'scan_pending'},{uploadIntentId:'i',attachmentLinkId:'l',sourceId:'s',state:'unknown',status:'scan_pending'}]){transport.mockResolvedValueOnce(json({data},202));await expect(bridge(req(),actor,{route:{kind:'upload',intentId:'i'}})).rejects.toMatchObject({status:503});}
 });
 it('accepts UTF-8 filenames and rejects invalid or dangerous percent encoding',async()=>{
  const bytes=new TextEncoder().encode('safe'),hash=createHash('sha256').update(bytes).digest('hex'),transport=vi.fn();const bridge=createFieldFamilyBridge(config,transport);
  const reply=(disposition:string)=>({status:200,headers:new Headers({'content-type':'application/pdf','content-disposition':disposition,'x-erpx-checksum-sha256':hash}),body:bytes});
  transport.mockResolvedValueOnce(reply("attachment; filename*=UTF-8''%E0%B8%87%E0%B8%B2%E0%B8%99.pdf"));expect((await bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'original'}})).status).toBe(200);
  transport.mockResolvedValueOnce(reply("attachment; filename*=UTF-8''dir%2Ffile%5Cname.pdf"));expect((await bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'original'}})).status).toBe(200);
  const longName='งาน'.repeat(85);transport.mockResolvedValueOnce(reply(`attachment; filename*=UTF-8''${encodeURIComponent(longName)}`));expect((await bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'original'}})).status).toBe(200);
  const previewName=`preview-${longName}`;transport.mockResolvedValueOnce({status:200,headers:new Headers({'content-type':'image/webp','content-disposition':`inline; filename*=UTF-8''${encodeURIComponent(previewName)}`,'x-erpx-checksum-sha256':hash}),body:bytes});expect((await bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'preview'}})).status).toBe(200);
  for(const value of ["attachment; filename*=UTF-8''bad%ZZ.pdf","attachment; filename*=UTF-8''bad%0Dname.pdf","attachment; filename*=UTF-8''%FF.pdf","attachment; filename*=UTF-8''file.pdf; extra=yes"]){transport.mockResolvedValueOnce(reply(value));await expect(bridge(req(),actor,{route:{kind:'content',linkId:'l',variant:'original'}})).rejects.toMatchObject({status:503});}
 });
 it('matches first list parameters, legacy numeric truthiness and trimmed create component refs',async()=>{
  const set={id:'s',targetType:'field_service_visit',targetId:'order',componentKind:'record',componentRef:null,purposeKey:'site_photo',expectedTargetVersion:3,policyVersion:1,classification:'internal',requiredCount:0,maxFiles:10,state:'draft',createdAt:1,updatedAt:1,links:[]};
  const transport=vi.fn(),bridge=createFieldFamilyBridge(config,transport);
  for(const query of ['?targetId=order&targetId=other&expectedTargetVersion=3&expectedTargetVersion=4','?targetId=order&expectedTargetVersion=0','?targetId=order&expectedTargetVersion=junk','?targetId=order&expectedTargetVersion=0x3']){transport.mockResolvedValueOnce(json({data:[set]},200));expect((await bridge(req(),actor,{route:{kind:'list',query}})).status).toBe(200);}
  const body=new TextEncoder().encode(JSON.stringify({targetType:'field_service_visit',targetId:'order',componentKind:'record',componentRef:'  ',purposeKey:'site_photo',expectedTargetVersion:3}));transport.mockResolvedValueOnce(json({data:set},201));expect((await bridge(req(),actor,{route:{kind:'create'},body})).status).toBe(201);
 });
});

it('shows the native Field surface only for its exact configured Holding and host',()=>{
 const local=JSON.stringify({privateBaseUrl:'http://127.0.0.1:44817',timeoutMs:1000,holdings:[{origin:'https://acme.erpx.localhost:3443',holdingSlug:'acme',holdingId:'h001',fieldFamily:true}]});
 try {vi.stubEnv('NODE_ENV','development');vi.stubEnv('NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE','container-localhost');vi.stubEnv('ERPX_NATIVE_FIELD_ATTACHMENT_CONFIG',local);expect(nativeFieldFamilyVisible('acme','h001','acme.erpx.localhost:3443')).toBe(true);expect(nativeFieldFamilyVisible('acme','h002','acme.erpx.localhost:3443')).toBe(false);expect(nativeFieldFamilyVisible('acme','h001','acme.erpx.localhost:3444')).toBe(false);vi.stubEnv('NODE_ENV','production');expect(nativeFieldFamilyVisible('acme','h001','acme.erpx.localhost:3443')).toBe(false);}finally{vi.unstubAllEnvs();}
});
