import {expect,it,vi} from 'vitest';
import {attachmentPolicyRegistry} from '@erpx/server-files/domain/attachments/policy';
import {attachmentContent} from '@erpx/server-files/public/attachments/service';
import type {SqlDatabase,SqlStatement,FileStore} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {bufferedAttachmentRead} from '@erpx/server-files/adapters/private-attachment-reads';

const actor:WorkContext={userId:'reader',name:'Reader',email:'reader@example.test',assignmentId:'assignment',role:'employee',companyId:'company',branchId:null,departmentId:null,holdingId:'H001',holdingSlug:'example',holdingName:'Example',scope:{kind:'holding',holdingId:'H001',holdingSlug:'example',holdingName:'Example',companyId:'company'},assignmentScope:'holding',inherited:false,modules:[],assignments:[],workspaces:[]};
const targets=[...new Map(attachmentPolicyRegistry.map(policy=>[policy.targetType,policy])).values()];

/** A mutable routing hint is not an owner transaction. Every registered target
 * must fail before policy/source queries, audit writes or any private storage.
 * Native owner suites separately prove authorized reads and complete parents. */
it.each(targets)('cannot claim an owner read unit from an arbitrary snapshot for $targetType',async policy=>{
 const hint={setId:'set',targetType:policy.targetType,targetId:'target',purposeKey:policy.purposeKey,componentKind:policy.componentKinds[0],componentRef:null};
 const statement:SqlStatement={bind(){return statement;},async first<T>(){return hint as unknown as T;},async all(){throw Error('UNLEASED_SOURCE_READ');},async run(){throw Error('UNLEASED_MUTATION');},async raw(){throw Error('UNLEASED_SOURCE_READ');}};
 const prepare=vi.fn((sql:string)=>{if(!sql)throw Error('QUERY_REQUIRED');return statement;}),batch=vi.fn(async()=>{throw Error('UNLEASED_MUTATION');}),exec=vi.fn(async()=>{throw Error('UNLEASED_MUTATION');}),database:SqlDatabase={prepare,batch,exec};
 const get=vi.fn(async()=>null),put=vi.fn(async()=>null),head=vi.fn(async()=>null),remove=vi.fn(async()=>{}),list=vi.fn(async()=>({objects:[],truncated:false})),files:FileStore={get,put,head,delete:remove,list};
 await expect(attachmentContent(database,files,actor,'link','original')).rejects.toMatchObject({status:503});
 expect(prepare).toHaveBeenCalledTimes(1);expect(prepare.mock.calls[0][0]).toContain('"setId"');
 for(const operation of [batch,exec,get,put,head,remove,list])expect(operation).not.toHaveBeenCalled();
});

it('returns verified buffered bytes and metadata without opening a second lazy storage body',async()=>{
 const bytes=new TextEncoder().encode('Verified owner bytes').buffer,uploaded=new Date(0),httpMetadata={contentType:'text/plain'},customMetadata={sha256:'verified-checksum'},range={offset:2,length:bytes.byteLength};
 const stored={key:'original-source',size:bytes.byteLength,etag:'checksum',httpEtag:'"checksum"',uploaded,httpMetadata,customMetadata,range,get body():ReadableStream<Uint8Array>{throw Error('UNUSED_STORAGE_BODY_OPENED');},async arrayBuffer(){throw Error('UNUSED_STORAGE_READ');},async text(){throw Error('UNUSED_STORAGE_READ');}};
 const result=bufferedAttachmentRead({object:stored,ownerRecord:'retained'},bytes);
 expect(result.ownerRecord).toBe('retained');expect(result.object).toMatchObject({key:'original-source',size:bytes.byteLength,etag:'checksum',httpEtag:'"checksum"',uploaded,httpMetadata,customMetadata,range});
 const copy=await result.object.arrayBuffer();expect(copy).toEqual(bytes);expect(copy).not.toBe(bytes);new Uint8Array(copy).fill(0);expect(await result.object.text()).toBe('Verified owner bytes');expect(await new Response(result.object.body).text()).toBe('Verified owner bytes');
});
