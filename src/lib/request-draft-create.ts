import { workFetch } from "./work-fetch";
/** Persist the exact creation command before any upload, including across reloads. */
export function createRequestDraftOnce() {
 let pending: Promise<string> | undefined;
 let attempt: {body: string; key: string} | undefined;
 return (base: string, storageKey: string, input: unknown) => {
  if (pending) return pending;
  try {
   const id = sessionStorage.getItem(`${storageKey}:created`);
   if (id) return Promise.resolve(id);
   const saved = sessionStorage.getItem(`${storageKey}:creation`);
   if (saved) attempt = JSON.parse(saved);
  } catch {}
  attempt ??= {body:JSON.stringify(input),key:crypto.randomUUID()};
  const command = attempt;
  try { sessionStorage.setItem(`${storageKey}:creation`,JSON.stringify(command)); } catch {}
  pending = (async () => {
   const response = await workFetch(`/api/v1${base}/requests`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":command.key},body:command.body});
   const result = await response.json();
   if (!response.ok) {
    if (response.status >= 400 && response.status < 500 && ![408,429].includes(response.status)) {
     attempt = undefined; try { sessionStorage.removeItem(`${storageKey}:creation`); } catch {}
    }
    throw Error(result.error?.message ?? "Unable to save draft. Retry the same request.");
   }
   const id = String(result.data.id);
   try { sessionStorage.setItem(`${storageKey}:created`,id); sessionStorage.removeItem(`${storageKey}:creation`); } catch {}
   return id;
  })().finally(() => {pending = undefined;});
  return pending;
 };
}
export function createRequestUploader() {
 const attempts = new WeakMap<File,{key:string;url?:string;done?:boolean}>();
 return async (base:string,ownerId:string,file:File) => {
  if (!file.size || file.size > 10*1024*1024 || !["image/jpeg","image/png","image/webp","application/pdf"].includes(file.type)) throw Error("Choose a JPG, PNG, WebP or PDF up to 10 MB.");
  let attempt = attempts.get(file);
  if (!attempt) {attempt = {key:crypto.randomUUID()}; attempts.set(file,attempt);}
  if (attempt.done) return;
  if (!attempt.url) {
   const response = await workFetch(`/api/v1${base}/uploads`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:JSON.stringify({ownerType:"employee_request",ownerId,purpose:"request_evidence",filename:file.name,mediaType:file.type,bytes:file.size})});
   const result = await response.json(); if (!response.ok) throw Error(result.error?.message ?? "Unable to prepare upload."); attempt.url = result.data.uploadUrl;
  }
  const response = await workFetch(attempt.url!,{method:"PUT",headers:{"Content-Type":file.type},body:file});
  if (!response.ok) {const result = await response.json();throw Error(result.error?.message ?? "Upload failed. Your draft is saved.");}
  attempt.done = true;
 };
}
