import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { createNativeJournalWorkspaceClient, NativeFinanceReadError, type NativeFinanceReadConfiguration } from './native-journal-detail';
import type { JournalReviewResult } from '@erpx/contracts/finance-journal-post';

type Posted = Extract<JournalReviewResult, { id: string }>;
const limit = 2 * 1024 * 1024;
const closed = (status: number, code: string, message: string) => Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });
function config(operation: 'post' | 'reverse'): NativeFinanceReadConfiguration | undefined {
  const raw = operation === 'post' ? process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG : process.env.ERPX_NATIVE_FINANCE_JOURNAL_REVERSE_CONFIG;
  if (raw === undefined) return undefined;
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
  const v = value as Record<string, unknown>;
  if (typeof v.privateBaseUrl !== 'string' || typeof v.timeoutMs !== 'number' || !Array.isArray(v.holdings) || !v.holdings.every((h: unknown) => !!h && typeof h === 'object' && !Array.isArray(h) && typeof (h as Record<string, unknown>).origin === 'string' && typeof (h as Record<string, unknown>).holdingSlug === 'string' && typeof (h as Record<string, unknown>).holdingId === 'string')) throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
  const result = v as NativeFinanceReadConfiguration;
  createNativeJournalWorkspaceClient(result); // Reuse the exact existing loopback/Holding validation.
  return result;
}
async function bytes(body: ReadableStream<Uint8Array> | null, max: number): Promise<Uint8Array> {
  if (!body) return new Uint8Array();
  const reader = body.getReader(), chunks: Uint8Array[] = []; let size = 0;
  try { for (;;) { const next = await reader.read(); if (next.done) break; size += next.value.length; if (size > max) { await reader.cancel(); throw new Error('BODY_LIMIT'); } chunks.push(next.value); } }
  finally { reader.releaseLock(); }
  const result = new Uint8Array(size); let offset = 0; for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.length; } return result;
}
function direct(url: URL, headers: Headers, body: Uint8Array, signal: AbortSignal): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(url, { method: 'POST', agent, maxHeaderSize: 8192, headers: Object.fromEntries(headers), signal }, incoming => {
      incoming.once('close', () => agent.destroy());
      try { const responseHeaders = new Headers(); for (let i=0;i<incoming.rawHeaders.length;i+=2) responseHeaders.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!); resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {status: incoming.statusCode ?? 502, headers: responseHeaders})); }
      catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
    });
    outgoing.once('error', error => { agent.destroy(); reject(error); }); outgoing.end(body);
  });
}
/** Exact native POST only. An absent config selects the pre-existing legacy route. */
async function nativeJournalCommand(request: Request, holdingSlug: string, journalId: string, operation: 'post' | 'reverse'): Promise<Response | undefined> {
  const settings = config(operation); if (!settings) return undefined;
  const host = request.headers.get('host'); const target = settings.holdings.find(h => new URL(h.origin).host === host && h.holdingSlug === holdingSlug);
  if (!target || !journalId || journalId.includes('/') || journalId.includes('\\')) return closed(404,'NOT_FOUND','Journal not found.');
  const origin = request.headers.get('origin'); if (origin !== null && origin !== target.origin) return closed(403,'ORIGIN_FORBIDDEN','Use the application origin.');
  const cookie = request.headers.get('cookie'); if (cookie && new TextEncoder().encode(cookie).length > 8192) return closed(413,'BODY_TOO_LARGE','Use a smaller request.');
  let body: Uint8Array; try { body = await bytes(request.body, limit); } catch { return closed(413,'BODY_TOO_LARGE','Use a smaller request.'); }
  const headers = new Headers({ host: host!, accept: 'application/json' });
  for (const name of ['cookie','origin','content-type','idempotency-key','x-erpx-context-company','x-erpx-context-revision','x-erpx-context-assignment','x-erpx-context-assignment-version','x-erpx-workspace-branch','x-erpx-workspace-department']) {const value=request.headers.get(name);if(value!==null)headers.set(name,value);}
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/journals/${encodeURIComponent(journalId)}/${operation}`, settings.privateBaseUrl);
  const controller=new AbortController(), cancel=()=>controller.abort();request.signal.addEventListener('abort',cancel,{once:true});if(request.signal.aborted)controller.abort();const timer=setTimeout(cancel,settings.timeoutMs);
  try {
    const response=await direct(url,headers,body,controller.signal);
    if(response.status>=300&&response.status<400) {void response.body?.cancel();return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');}
    if (response.status === 404) { void response.body?.cancel(); return closed(404,'NOT_FOUND','Journal not found.'); }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') { void response.body?.cancel(); return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.'); }
    const payload=await bytes(response.body,32768);let value:unknown;try {value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(payload));}catch{return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');}
    if (operation === 'reverse' && response.status === 201) {
      const data=(value as {data?:unknown})?.data;
      if (!data || typeof data !== 'object' || Array.isArray(data) || Object.keys(data).sort().join(',') !== 'reversalJournalId,reversedJournalId') return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal reversal.');
      const result=data as Extract<JournalReviewResult, { reversalJournalId: string }>;
      const key=request.headers.get('idempotency-key');
      if (!key || result.reversedJournalId !== journalId || result.reversalJournalId !== `reversal-${key}` || result.reversalJournalId === journalId) return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal reversal.');
      return Response.json({data:result},{status:201,headers:{'Cache-Control':'private, no-store'}});
    }
    if (operation === 'post' && response.status === 200) {
      const data=(value as {data?:unknown})?.data;
      if(!data || typeof data!=='object' || Array.isArray(data) || Object.keys(data).sort().join(',')!=='id,status,version') return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');
      const result=data as Posted;
      if(result.id!==journalId || result.status!=='posted' || !Number.isSafeInteger(result.version) || result.version<1) return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');
      return Response.json({data:result},{headers:{'Cache-Control':'private, no-store'}});
    }
    const error=(value as {error?:{code?:unknown;message?:unknown}})?.error;
    if(response.status<400||response.status>599||!error||typeof error.code!=='string'||typeof error.message!=='string'||error.code.length>100||error.message.length>1000)return closed(502,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');
    return closed(response.status,error.code,error.message);
  } catch { return closed(503,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.'); }
  finally {clearTimeout(timer);request.signal.removeEventListener('abort',cancel);}
}
export const nativeJournalPost = (request: Request, holdingSlug: string, journalId: string) => nativeJournalCommand(request, holdingSlug, journalId, 'post');
export const nativeJournalReverse = (request: Request, holdingSlug: string, journalId: string) => nativeJournalCommand(request, holdingSlug, journalId, 'reverse');
/** Re-read through the same real client and full captured context after receipt. */
async function nativeJournalReadback(request: Request, holdingSlug: string, journalId: string, operation: 'post' | 'reverse'): Promise<Response | undefined> {
  const settings=config(operation);if(!settings)return undefined;
  const company=request.headers.get('x-erpx-context-company'),assignment=request.headers.get('x-erpx-context-assignment'),revision=request.headers.get('x-erpx-context-revision'),version=request.headers.get('x-erpx-context-assignment-version');
  if(!company||!assignment||!revision||!version||!/^\d+$/.test(revision)||!/^\d+$/.test(version)||!Number.isSafeInteger(Number(revision))||!Number.isSafeInteger(Number(version)))return closed(409,'WORK_CONTEXT_CHANGED','Your work role changed. Reload and review before submitting.');
  try {
    const data=await createNativeJournalWorkspaceClient(settings)(request.headers,{holdingSlug,journalId,branchSlug:request.headers.get('x-erpx-workspace-branch')??undefined,departmentSlug:request.headers.get('x-erpx-workspace-department')??undefined,expected:{companyId:company,assignmentId:assignment,revision:Number(revision),assignmentVersion:Number(version)}},request.signal);
    return Response.json(data,{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) {if(error instanceof NativeFinanceReadError && error.status && [401,403,404,409].includes(error.status))return closed(error.status,error.code,'Unable to confirm journal posting.');return closed(503,'NATIVE_FINANCE_UNAVAILABLE','Unable to confirm journal posting.');}
}

export const nativeJournalPostReadback = (request: Request, holdingSlug: string, journalId: string) => nativeJournalReadback(request, holdingSlug, journalId, 'post');
export const nativeJournalReverseReadback = (request: Request, holdingSlug: string, journalId: string) => nativeJournalReadback(request, holdingSlug, journalId, 'reverse');
