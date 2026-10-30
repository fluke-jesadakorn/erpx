import {expect,it,vi} from 'vitest';
import {createServer} from 'node:http';
vi.mock('server-only',()=>({}));
import {nativeFieldIntakeRequest,type NativeFieldIntakeTransportInput} from '../../../packages/native-sdk/src/field-service-attachment-intake';
const set={id:'s',targetType:'field_service_visit',targetId:'v',componentKind:'record',componentRef:null,purposeKey:'site_photo',expectedTargetVersion:3,policyVersion:1,classification:'internal',requiredCount:0,maxFiles:10,state:'draft',createdAt:123,updatedAt:456,links:[{id:'l',sourceId:null,filename:'file',mediaType:'application/octet-stream',bytes:null,sourceState:null,linkState:'draft',caption:null,previewAvailable:false,createdAt:123,expiresAt:456,boundAt:null}]};
const base={baseUrl:'http://127.0.0.1:43521',slug:'acme',headers:new Headers({'idempotency-key':'exact-key','x-erpx-context-revision':'8'}),timeoutMs:1000};
it('forwards the original body, headers and raw query with typed complete responses',async()=>{
 const seen:NativeFieldIntakeTransportInput[]=[];const transport=async(input:NativeFieldIntakeTransportInput)=>{seen.push(input);return input.method==='POST'?{status:201,body:{data:set}}:{status:200,body:{data:[set]}};};
 const raw='{ "expectedTargetVersion": 3, "duplicate": 1, "duplicate": 2 }';
 const created=await nativeFieldIntakeRequest({...base,route:{kind:'create'},body:raw},transport);expect(created.body).toEqual({data:set});expect(seen[0]).toMatchObject({method:'POST',suffix:'',body:raw});expect(seen[0]!.headers.get('idempotency-key')).toBe('exact-key');
 const list=await nativeFieldIntakeRequest({...base,route:{kind:'list',query:'?targetId=v&targetId=second'}},transport);expect(list.body).toEqual({data:[set]});expect(seen[1]!.suffix).toBe('?targetId=v&targetId=second');
});
it('preserves error envelopes and rejects malformed or non-finite success responses',async()=>{
 const route={kind:'list'} as const;
 expect(await nativeFieldIntakeRequest({...base,route},async()=>({status:403,body:{error:{code:'FORBIDDEN',message:'Denied'}}}))).toEqual({status:403,body:{error:{code:'FORBIDDEN',message:'Denied'}}});
 for(const body of [{data:[{...set,links:[{...set.links[0],expiresAt:undefined}]}]},{data:[{...set,createdAt:Infinity}]},{data:'wrong'}]) await expect(nativeFieldIntakeRequest({...base,route},async()=>({status:200,body}))).rejects.toThrow('NATIVE_FIELD_INTAKE_RESPONSE_INVALID');
});
it('uses bounded loopback transport and preserves the caller host header',async()=>{
 const observed:{host?:string;body?:string}={};
 const server=createServer((req,res)=>{observed.host=req.headers.host;let body='';req.setEncoding('utf8');req.on('data',v=>body+=v);req.on('end',()=>{observed.body=body;res.setHeader('content-type','application/json');res.writeHead(201);res.end(JSON.stringify({data:set}));});});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 try {const address=server.address();if(!address||typeof address==='string')throw Error('missing address');const raw='{ "exact":true }';await nativeFieldIntakeRequest({...base,baseUrl:`http://127.0.0.1:${address.port}`,headers:new Headers({'host':'acme.example.test','idempotency-key':'key-1234'}),route:{kind:'create'},body:raw});expect(observed).toEqual({host:'acme.example.test',body:raw});await expect(nativeFieldIntakeRequest({...base,baseUrl:'http://127.0.0.1:3101',route:{kind:'list'}})).rejects.toThrow('NATIVE_FIELD_INTAKE_CONFIGURATION_INVALID');}finally{await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
it('rejects oversized bodies and times out without a fallback',async()=>{
 let received=0;const server=createServer(()=>{received++;});await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 try {const address=server.address();if(!address||typeof address==='string')throw Error('missing address');const local={...base,baseUrl:`http://127.0.0.1:${address.port}`};await expect(nativeFieldIntakeRequest({...local,route:{kind:'create'},body:'x'.repeat(2*1024*1024+1)})).rejects.toThrow('NATIVE_FIELD_INTAKE_REQUEST_INVALID');expect(received).toBe(0);await expect(nativeFieldIntakeRequest({...local,timeoutMs:250,route:{kind:'list'}})).rejects.toThrow();expect(received).toBe(1);}finally{server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
