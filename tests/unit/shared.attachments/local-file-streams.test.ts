import {afterEach,describe,expect,it,vi} from 'vitest';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

const reads=vi.hoisted(()=>({opened:0,closed:0}));
vi.mock('node:fs',async()=>{
 const actual=await vi.importActual<typeof import('node:fs')>('node:fs');
 return {...actual,createReadStream:(...args:Parameters<typeof actual.createReadStream>)=>{
  reads.opened++;const stream=actual.createReadStream(...args);stream.once('close',()=>reads.closed++);return stream;
 }};
});
vi.resetModules();
const {LocalFileStore}=await import("../../../src/server/modules/runtime/adapters/runtime/filesystem");

describe('Private local file readers',()=>{
 let directory:string|undefined;
 afterEach(async()=>{if(directory)await rm(directory,{recursive:true,force:true});directory=undefined;});
 it('repeated buffered reads close their actual streams without opening an unused response body',async()=>{
  directory=await mkdtemp(join(tmpdir(),'erpx-file-readers-'));const files=new LocalFileStore(directory),content='Thai legal original ภาษาไทย';
  await files.put('original',content);const opened=reads.opened,closed=reads.closed;
  for(let i=0;i<30;i++){
   const object=await files.get('original');expect(object).toBeTruthy();expect(reads.opened-opened).toBe(i);
   expect(Buffer.from(await object!.arrayBuffer()).toString('utf8')).toBe(content);
  }
  await vi.waitFor(()=>expect(reads.closed-closed).toBe(30));expect(reads.opened-opened).toBe(30);
  const object=await files.get('original');expect(reads.opened-opened).toBe(30);
  expect(object!.body).toBe(object!.body);expect(reads.opened-opened).toBe(31);
  expect(await new Response(object!.body).text()).toBe(content);
  await vi.waitFor(()=>expect(reads.closed-closed).toBe(31));
  expect(await object!.text()).toBe(content);await vi.waitFor(()=>expect(reads.closed-closed).toBe(32));
 });
 it('keeps buffered, streaming and empty range reads consistent',async()=>{
  directory=await mkdtemp(join(tmpdir(),'erpx-file-ranges-'));const files=new LocalFileStore(directory);
  await files.put('range','abcdef');const range=await files.get('range',{range:{offset:2,length:3}});
  expect(range!.range).toEqual({offset:2,length:3});expect(await range!.text()).toBe('cde');
  expect(await new Response(range!.body).text()).toBe('cde');
  await files.put('empty','');const before=reads.opened,empty=await files.get('empty');
  expect((await empty!.arrayBuffer()).byteLength).toBe(0);expect(await new Response(empty!.body).text()).toBe('');expect(reads.opened).toBe(before);
 });
});
