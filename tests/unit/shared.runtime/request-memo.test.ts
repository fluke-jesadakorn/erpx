import { expect, it } from 'vitest';
import { memoizeInRequest, memoizeSyncInRequest, runWithRequestMemo } from "../../../src/server/modules/runtime/public/runtime/request-memo";

it('deduplicates concurrent session reads only within the same request', async () => {
  const key = Symbol('session'); let reads = 0;
  await runWithRequestMemo(async () => {
    const load = async () => ({ revision: ++reads });
    const [a,b] = await Promise.all([memoizeInRequest(key,load),memoizeInRequest(key,load)]);
    expect(a).toBe(b); expect(reads).toBe(1);
  });
  expect(await runWithRequestMemo(() => memoizeInRequest(key,async () => ++reads))).toBe(2);
});
it('keeps concurrent actors and nested request invocations isolated', async () => {
  const key = Symbol('session');
  const actor = (name:string) => runWithRequestMemo(async () => {
    const first = await memoizeInRequest(key,async () => name);
    await Promise.resolve();
    expect(await runWithRequestMemo(() => memoizeInRequest(key,async () => 'nested'))).toBe('nested');
    expect(await memoizeInRequest(key,async () => 'wrong')).toBe(first);
    return first;
  });
  expect(await Promise.all([actor('front desk'),actor('housekeeper')])).toEqual(['front desk','housekeeper']);
});
it('does not cache unscoped calls or independently selected authority facts', async () => {
  const key = Symbol('session'); let reads = 0;
  await memoizeInRequest(key,async () => ++reads); await memoizeInRequest(key,async () => ++reads);
  expect(reads).toBe(2);
  await runWithRequestMemo(async () => {
    await memoizeInRequest(Symbol('authority'),async () => ++reads);
    await memoizeInRequest(Symbol('authority'),async () => ++reads);
  });
  expect(reads).toBe(4);
});
it('retains a failed session read within its request and reads afresh on the next request', async () => {
  const key = Symbol('session'); let reads = 0;
  await runWithRequestMemo(async () => {
    const fail = async () => { reads++; throw new Error('session expired'); };
    const result = await Promise.allSettled([memoizeInRequest(key,fail),memoizeInRequest(key,fail)]);
    expect(result.every(value => value.status === 'rejected')).toBe(true); expect(reads).toBe(1);
  });
  expect(await runWithRequestMemo(() => memoizeInRequest(key,async () => ++reads))).toBe(2);
});

it('keeps synchronous digest work local to one request, including nested and concurrent invocations', async () => {
  const key=Symbol.for('test.manifest.digest'); let reads=0;
  const load=()=>({generation:++reads});
  await runWithRequestMemo(async()=>{
    const first=memoizeSyncInRequest(key,load);
    expect(memoizeSyncInRequest(key,load)).toBe(first);
    await runWithRequestMemo(async()=>expect(memoizeSyncInRequest(key,load)).not.toBe(first));
    expect(memoizeSyncInRequest(key,load)).toBe(first);
  });
  expect(reads).toBe(2);
  const [a,b]=await Promise.all([runWithRequestMemo(async()=>memoizeSyncInRequest(key,load)),runWithRequestMemo(async()=>memoizeSyncInRequest(key,load))]);
  expect(a).not.toBe(b); expect(reads).toBe(4);
  expect(memoizeSyncInRequest(key,load)).not.toBe(memoizeSyncInRequest(key,load)); expect(reads).toBe(6);
});

it('returns synchronous Proxy responses immediately and leaves no scope after an exception',()=>{
  const key=Symbol('proxy.digest');
  const response={status:200};
  const actual=runWithRequestMemo(()=>{
    const first=memoizeSyncInRequest(key,()=>({digest:'unchanged'}));
    expect(memoizeSyncInRequest(key,()=>({digest:'wrong'}))).toBe(first);
    return response;
  });
  expect(actual).toBe(response);
  expect(()=>runWithRequestMemo(()=>{memoizeSyncInRequest(key,()=>({digest:'temporary'}));throw Error('origin refused');})).toThrow('origin refused');
  expect(memoizeSyncInRequest(key,()=>({digest:'fresh'}))).toEqual({digest:'fresh'});
});
