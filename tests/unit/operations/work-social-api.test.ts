import {beforeEach,expect,it,vi} from 'vitest';
import {workSocialBetaPathAllowed} from '@erpx/contracts/work-social-api';
import {officeApiAllowed} from '@erpx/feature-runtime/lib/product-api-policy';
const mocks=vi.hoisted(()=>({native:vi.fn()}));
vi.mock('@erpx/server-manage_erpx/public/beta-api',()=>({betaHandler:(resource:string)=>{if(resource!=='beta')throw Error('NATIVE_RESOURCE_CHANGED');return mocks.native;}}));
vi.mock('@erpx/feature-runtime/lib/product-hosts',()=>({productHostForHost:(host:string)=>host==='apps-dev-erpx.jesadakorn.com'?'apps':host==='admin-dev-erpx.jesadakorn.com'?'admin':null}));
import {workSocialRequest} from '@erpx/feature-apps/routes/api/v1/holdings/[holdingSlug]/beta/[[...path]]/route';
const host='apps-dev-erpx.jesadakorn.com',prefix='/api/v1/holdings/simulation/beta/';
beforeEach(()=>mocks.native.mockReset());

it('delegates only Work Social reads and commands to the original authority and transaction handler',async()=>{
 const allowed:[string,string[]][]=[['GET',['home']],['GET',['meta']],['GET',['suggestions']],['POST',['drafts']],['POST',['drafts','draft-1']],['POST',['drafts','draft-1','publish']],['POST',['comment-uploads']],['POST',['comment-uploads','upload-1']],['POST',['post-edit-uploads']],['POST',['post-edit-uploads','upload-1']],['POST',['agent-replies','reply-1']]];
 for(const[method,path]of allowed){
  const request=new Request('http://127.0.0.1:3252'+prefix+path.join('/')+'?preserved=1',{method,headers:{host,Origin:'https://'+host,'Idempotency-Key':'original-command-key','Content-Type':'application/json'},...(method==='POST'?{body:'{"expectedVersion":4}'}:{})});
  const context={params:Promise.resolve({holdingSlug:'simulation',path})},response=Response.json({data:{native:true}},{status:202});mocks.native.mockResolvedValue(response);
  expect(officeApiAllowed('apps',prefix+path.join('/'),method)).toBe(true);
  expect(await workSocialRequest(request,context)).toBe(response);
  expect(mocks.native).toHaveBeenLastCalledWith(request,context);
  if(method==='POST')expect(await request.text()).toBe('{"expectedVersion":4}');
 }
 expect(mocks.native).toHaveBeenCalledTimes(allowed.length);
});

it('refuses Administration actions, malformed suffixes and unrelated verbs before native dispatch',async()=>{
 const denied:[string,string[]][]=[...['settings','invitations','revoke','feedback','branches','billing','ai-drafts'].flatMap(value=>[['GET',[value]],['POST',[value]]] as [string,string[]][]),['GET',[]],['GET',['drafts']],['POST',['meta']],['DELETE',['drafts','draft-1']],['POST',['drafts','draft-1','publish','extra']],['POST',['drafts','..']],['POST',['drafts','id/child']],['POST',['drafts','id%2fchild']]];
 for(const[method,path]of denied){
  expect(workSocialBetaPathAllowed(path,method)).toBe(false);
  const response=await workSocialRequest(new Request('https://'+host+prefix+path.join('/'),{method,headers:{host}}),{params:Promise.resolve({holdingSlug:'simulation',path})});expect(response.status).toBe(404);
 }
 for(const foreign of ['admin-dev-erpx.jesadakorn.com','foreign.invalid'])expect((await workSocialRequest(new Request('https://'+foreign+prefix+'home',{headers:{host:foreign}}),{params:Promise.resolve({holdingSlug:'simulation',path:['home']})})).status).toBe(404);
 expect(officeApiAllowed('apps',prefix+'settings','GET')).toBe(false);
 expect(officeApiAllowed('admin',prefix+'settings','GET')).toBe(true);
 for(const other of ['finance','people','recruit','learn'] as const)expect(officeApiAllowed(other,prefix+'home','GET')).toBe(false);
 expect(mocks.native).not.toHaveBeenCalled();
});

it('normalizes only an allowed HEAD read before the native mutation dispatcher',async()=>{
 const request=new Request('http://127.0.0.1:3252'+prefix+'home?preserved=1',{method:'HEAD',headers:{host}}),response=Response.json({data:[]});mocks.native.mockResolvedValue(response);
 expect(await workSocialRequest(request,{params:Promise.resolve({holdingSlug:'simulation',path:['home']})})).toBe(response);
 const forwarded=mocks.native.mock.calls[0][0] as Request;expect(forwarded.method).toBe('GET');expect(forwarded.url).toBe(request.url);expect(forwarded.headers.get('host')).toBe(host);
});
