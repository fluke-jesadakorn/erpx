import {describe,it,expect,vi} from 'vitest';
import {boundedJsonBody} from '../../../packages/workplace/src/lib/bounded-json-body';

describe('bounded JSON ingress',()=>{
 it('returns parsed data and the original bytes without normalizing unknown fields',async()=>{
  const source=' { "filename": "a", "ignored": [1,2] } ';const request=new Request('https://example.test',{method:'POST',body:source});
  const result=await boundedJsonBody(request,65536);expect(result.value).toEqual({filename:'a',ignored:[1,2]});expect(new TextDecoder().decode(result.raw)).toBe(source);
 });
 it.each([64*1024,2*1024*1024])('cancels chunked input as soon as the %i byte limit is crossed',async limit=>{
  const cancel=vi.fn();let pulls=0;const stream=new ReadableStream<Uint8Array>({pull(controller){pulls++;controller.enqueue(new TextEncoder().encode(pulls===1?'{"ignored":"': 'x'.repeat(limit)));},cancel},{highWaterMark:0});
  const request=new Request('https://example.test',{method:'POST',body:stream,duplex:'half'} as RequestInit);
  await expect(boundedJsonBody(request,limit)).rejects.toMatchObject({status:413,code:'JSON_BODY_LIMIT'});expect(cancel).toHaveBeenCalledOnce();expect(pulls).toBe(2);
 });
 it('retains malformed JSON parse behavior',async()=>{
  await expect(boundedJsonBody(new Request('https://example.test',{method:'POST',body:'{'}),65536)).rejects.toBeInstanceOf(SyntaxError);
 });
});
