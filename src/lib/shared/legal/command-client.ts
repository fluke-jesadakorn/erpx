import { workFetch } from "@/lib/work_projection/work-fetch";
import {legalCommandAcknowledged} from './command-acknowledgement';
import {captureLegalCommandCompletion,acknowledgeLegalCommand} from './command-completion';
import type {WorkCommandCompletion} from '@/lib/work_projection/work-fetch';

type Attempt = { path: string; body: string; key: string; uncertain?:boolean };

/** Keep an uncertain command's exact payload and key across a refresh. */
export function createLegalCommandClient(storageKey: string,options:{strictOwnerReceipt?:boolean}={}) {
  let attempt: Attempt | null = null;
  let pending: Promise<unknown> | null = null;
  let submittedForm:WorkCommandCompletion|undefined;
  function forget() {
    attempt = null;
    submittedForm=undefined;
    try { sessionStorage.removeItem(storageKey); } catch {}
  }
  function savedAttempt() {
    try {
      const saved = sessionStorage.getItem(storageKey);
      if (saved) {
        const value = JSON.parse(saved);
        if (typeof value.path === "string" && typeof value.body === "string" && typeof value.key === "string") attempt = value;
      }
    } catch {}
    return attempt;
  }
  const send = async function (path: string, input: unknown): Promise<unknown> {
    const body = JSON.stringify(input);
    if (pending) throw new Error("Wait for the current Legal action to finish.");
    savedAttempt();
    if (attempt && (attempt.path !== path || attempt.body !== body)) {
      throw new Error("A previous action has an uncertain result. Retry its original details before sending a different action.");
    }
    if(!attempt){attempt={path,body,key:crypto.randomUUID()};if(options.strictOwnerReceipt)submittedForm=captureLegalCommandCompletion();}
    const current = attempt;
    try { sessionStorage.setItem(storageKey, JSON.stringify(current)); } catch {}
    pending = (async () => {
      const init = {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": current.key },
        body: current.body,
      },completion=submittedForm;
      const response=options.strictOwnerReceipt?await workFetch(path,init,completion,'caller'):await workFetch(path,init);
      const rejected = response.status >= 400 && response.status < 500 && ![408, 429].includes(response.status);
      if (rejected&&!(current.uncertain&&[401,403,404,409].includes(response.status))) forget();
      const result = await response.json().catch(() => {
        throw new Error(rejected ? "The request was rejected. Reload and review before submitting." : "Unable to read the result. Reconnect and recover this action.");
      });
      if (!response.ok) {
        throw new Error(result?.error?.message ?? "Unable to save. Reconnect and retry this action.");
      }
      if(!legalCommandAcknowledged(path,input,result?.data,options.strictOwnerReceipt))throw new Error("The response did not acknowledge this original Legal action. Reconnect and recover its exact request.");
      if(options.strictOwnerReceipt)acknowledgeLegalCommand(completion);
      forget();
      return result.data;
    })();
    try { return await pending; } catch(error){if(attempt){attempt.uncertain=true;try{sessionStorage.setItem(storageKey,JSON.stringify(attempt));}catch{}}throw error;} finally { pending = null; }
  };
  return Object.assign(send, {
    hasPending: () => Boolean(savedAttempt()),
    recover: async () => {
      const current = savedAttempt();
      if (!current) return null;
      const input = JSON.parse(current.body) as Record<string, unknown>;
      return { input, data: await send(current.path, input) };
    },
  });
}
