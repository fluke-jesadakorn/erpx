"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

export function usePeopleIntegrationData<T>(endpoint: string, pagination?: "calendar" | "hierarchy", initial?: T) {
  const [result, setResult] = useState<{endpoint:string;data:T} | null>(initial === undefined ? null : {endpoint,data:initial}), [error, setError] = useState<{endpoint:string;message:string}|null>(null), [loading, setLoading] = useState(initial === undefined);
  const initialEndpoint=useRef(initial === undefined ? null : endpoint);
  const sequence = useRef(0);
  const load = useCallback(async () => {
    const attempt = ++sequence.current;
    setLoading(true); setError(null);
    try {
      const response = await workFetch(endpoint, {cache:"no-store"});
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error?.message ?? "This section could not be loaded.");
      if (pagination) {
        const aggregate = payload.data as {nextCursor:number|null;items?:unknown[];nodes?:unknown[];people?:unknown[]};
        while (aggregate.nextCursor!==null && Number.isInteger(aggregate.nextCursor)) {
          const url = new URL(endpoint,window.location.origin);
          url.searchParams.set("cursor",String(aggregate.nextCursor));
          const pageResponse = await workFetch(`${url.pathname}${url.search}`,{cache:"no-store"});
          const page = await pageResponse.json();
          if (!pageResponse.ok) throw new Error(page.error?.message ?? "The remaining calendar items could not be loaded.");
          for (const key of pagination==="calendar" ? ["items"] as const : ["nodes","people"] as const) {
            aggregate[key] = [...new Map([...(aggregate[key]??[]),...(page.data[key]??[])].map(item=>[(item as {id:string}).id,item])).values()];
          }
          const previous=aggregate.nextCursor;
          aggregate.nextCursor=page.data.nextCursor;
          if (aggregate.nextCursor!==null && aggregate.nextCursor<=previous) throw new Error("Calendar pagination could not continue. Refresh this section.");
          if(attempt!==sequence.current)return;
        }
      }
      if (attempt === sequence.current) setResult({endpoint,data:payload.data});
    } catch (cause) { if (attempt === sequence.current) setError({endpoint,message:cause instanceof Error ? cause.message : "This section could not be loaded."}); }
    finally { if (attempt === sequence.current) setLoading(false); }
  }, [endpoint,pagination]);
  useEffect(() => {
    if(initialEndpoint.current===endpoint){return;}
    let active = true;
    const requestSequence = sequence;
    queueMicrotask(() => { if (active) void load(); });
    return () => { active = false; requestSequence.current++; };
  }, [load]);
  return {data:result?.endpoint===endpoint?result.data:null,error:error?.endpoint===endpoint?error.message:"",loading:loading || result?.endpoint!==endpoint && error?.endpoint!==endpoint,reload:load};
}

export function usePeopleIntegrationCommand(base: string, onSaved: () => Promise<void>) {
  const [busy, setBusy] = useState(false), [error, setError] = useState(""), [message, setMessage] = useState("");
  const receipt = useRef<{body:string;key:string} | null>(null);
  async function send(path: string, input: unknown, success = "Saved.") {
    const submittedForm = document.activeElement?.closest("form");
    const formEditVersion = submittedForm?.getAttribute("data-erpx-edit-version");
    const body = JSON.stringify(input), signature = path + body;
    if (receipt.current?.body !== signature) receipt.current = {body:signature,key:crypto.randomUUID()};
    setBusy(true); setError(""); setMessage("");
    try {
      const response = await workFetch(`/api/v1${base}/${path}`, {method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":receipt.current.key},body});
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "This change could not be saved.");
      receipt.current = null;
      await onSaved(); setMessage(success);
      if(submittedForm && formEditVersion) window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail:{form:submittedForm,formEditVersion}}));
      return result.data;
    } catch (cause) { setError(cause instanceof Error ? cause.message : "This change could not be saved."); return null; }
    finally { setBusy(false); }
  }
  return {busy,error,message,send};
}
