import {expect,it,vi} from "vitest";
import {createHandbookService,type HandbookPorts} from "@erpx/server-collaboration/application/handbook";
import type {Article} from "@erpx/server-collaboration/domain/handbook";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError} from "@erpx/server-runtime/public/http";
import {createHandbookCommands} from '@erpx/feature-collaboration/lib/handbook-command';
import {handbookCreateHref,handbookCreateReturn} from '@erpx/feature-collaboration/lib/handbook-navigation';
function only<T>(values:Partial<T>):T{return new Proxy(values,{get(target,name){if(name in target)return Reflect.get(target,name);throw Error(`Unexpected port ${String(name)}`);}}) as T;}
type Ports=HandbookPorts;
const actor={userId:"editor",companyId:"company",branchId:null,departmentId:null} as WorkContext;
const article:Article={id:"article",company_id:"company",version:2,draft_revision:2,published_revision:1,archived:0,created_by:"editor",updated_at:10};
function service(override:Partial<Ports>={}){return createHandbookService(only<Ports>({purchaseAccess:async()=>undefined,error:(s,c,m)=>new ApiError(s,c,m),companyMember:async()=>true,serviceOwner:async()=>true,requireServiceOwner:async()=>{},...override}));}
it("denies removed membership before owner or editor facts",async()=>{await expect(service({companyMember:async()=>false}).handbookArticle(actor,"article")).rejects.toMatchObject({code:"MEMBERSHIP_REQUIRED"});});
it("does not reveal unpublished or archived content",async()=>{await expect(service({repository:only<Ports["repository"]>({article:async()=>({...article,published_revision:null})})}).handbookArticle(actor,"article")).rejects.toMatchObject({code:"ARTICLE_NOT_FOUND"});await expect(service({repository:only<Ports["repository"]>({article:async()=>({...article,archived:1})})}).handbookArticle(actor,"article")).rejects.toMatchObject({code:"ARTICLE_NOT_FOUND"});});
it("prevents non-editors from loading the draft search or revision",async()=>{const s=service({serviceOwner:async()=>false,repository:only<Ports["repository"]>({editor:async()=>null})});await expect(s.handbookList(actor,"secret",true)).rejects.toMatchObject({code:"HANDBOOK_EDITOR_REQUIRED"});await expect(s.handbookArticle(actor,"article",true)).rejects.toMatchObject({code:"HANDBOOK_EDITOR_REQUIRED"});});
it("keeps published revision number separate from mutable draft version in list results",async()=>{const page={items:[{...article,page_key:"Guide",revision:1,title:"Guide",category:"policy",excerpt:"Guide",published_at:10}],nextCursor:null,previousCursor:null,hasMore:false};expect((await service({page:async()=>page}).handbookList(actor)).articles[0].version).toBe(1);expect((await service({page:async()=>page}).handbookList(actor,"",true)).articles[0].version).toBe(2);});
it("validates historical revision identity before loading any revision",async()=>{await expect(service({repository:only<Ports["repository"]>({article:async()=>article})}).handbookArticle(actor,"article",false,0)).rejects.toMatchObject({code:"ARTICLE_NOT_FOUND"});});
it('retains the exact submitted handbook key/body through incomplete acknowledgements and refuses replacement drafts',async()=>{
 const requests:RequestInit[]=[],fetcher=vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{requests.push(init!);return requests.length===1?new Response('{"data":'):Response.json({data:{id:'article-key',version:1}});}),commands=createHandbookCommands(fetcher,()=> 'key'),attempt=commands.prepare('handbook',{title:'Exact original',expectedVersion:1},'original');
 await expect(commands.execute('/holdings/test',attempt,()=> 'original')).rejects.toThrow();expect(commands.pending()).toEqual(attempt);expect(()=>commands.prepare('handbook',{title:'Newer local draft',expectedVersion:1},'original')).toThrow(/submitted/);expect(await commands.execute('/holdings/test',attempt,()=> 'original')).toEqual({result:{id:'article-key',version:1},unchanged:true});expect(requests[0].body).toBe(requests[1].body);expect(new Headers(requests[0].headers).get('Idempotency-Key')).toBe(new Headers(requests[1].headers).get('Idempotency-Key'));expect(commands.pending()).toBeNull();
});
it.each([{id:'another-article',version:3},{id:'article',version:99},{id:'article'},{}])('refuses an unrelated or incomplete handbook result %j without acknowledging the command',async response=>{
 const commands=createHandbookCommands(async()=>Response.json({data:response}),()=> 'key'),attempt=commands.prepare('handbook/article/actions',{action:'archive',expectedVersion:2},'original',undefined,true);await expect(commands.execute('/holdings/test',attempt,()=> 'original')).rejects.toThrow(/acknowledgement/);expect(commands.pending()).toEqual(attempt);
});
it('does not send or consume a handbook result after the original account/context changes',async()=>{
 let context='original';const fetcher=vi.fn(async()=>{context='replacement';return Response.json({data:{id:'editor',version:2}});}),commands=createHandbookCommands(fetcher,()=> 'key'),attempt=commands.prepare('handbook/editors',{userId:'editor',active:false,expectedVersion:1},'original',undefined,true);
 await expect(commands.execute('/holdings/test',attempt,()=> 'replacement')).rejects.toThrow(/context/);expect(fetcher).not.toHaveBeenCalled();await expect(commands.execute('/holdings/test',attempt,()=>context)).rejects.toThrow(/context/);expect(commands.pending()).toEqual(attempt);
});

it('retains the actual Handbook view and repeated filters when leaving its creation drawer',()=>{
 const base='/holdings/acme',href=handbookCreateHref(base,'drafts',{q:'review',tag:['one','two'],cursor:'next',detailKind:'handbook',detailId:'new',returnTo:'https://outside.invalid'}),url=new URL(href,'https://handbook.invalid'),back=handbookCreateReturn(base,url.searchParams.get('returnTo')!);
 expect(url.pathname).toBe('/holdings/acme/knowledge/handbook/published/new');
 expect(back).toBe('/holdings/acme/knowledge/handbook/drafts?q=review&tag=one&tag=two&cursor=next');
});
it.each(['https://outside.invalid/holdings/acme/knowledge/handbook/editors','/holdings/other/knowledge/handbook/drafts','/holdings/acme/knowledge/handbook/published/new','/holdings/acme/knowledge/handbook/../../administration'])('does not trust a foreign or non-view Handbook creation return %s',returnTo=>{
 expect(handbookCreateReturn('/holdings/acme',returnTo)).toBe('/holdings/acme/knowledge/handbook/published');
});
