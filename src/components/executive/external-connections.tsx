"use client";
import { useEffect, useState } from "react";
import { Button, Field, InlineNotice, Select, TextInput } from "@/components/ui/foundation";
import { Dialog } from "@/components/ui/dialog";
import { TimeText } from "@/components/time/time-preference-context";
import { workFetch } from "@/lib/work-fetch";
import type { ExternalConnections } from "@/lib/executive/connections";
import { Plug, Plus } from "lucide-react";
import styles from "./external-connections.module.css";

export function ExecutiveExternalConnections({api}:{api:string}) {
  const [open,setOpen]=useState(false),[data,setData]=useState<ExternalConnections|null>(null),[error,setError]=useState(""),[busy,setBusy]=useState(false),[secret,setSecret]=useState(""),[copied,setCopied]=useState(false);
  async function load() {
    const r=await workFetch(`${api}/connections`),j=await r.json();
    if(!r.ok)throw Error(j.error?.message??"Could not load connections.");
    setData(j.data);
  }
  useEffect(()=>{
    if(!open)return;
    let active=true;
    workFetch(`${api}/connections`).then(async r=>{const j=await r.json();if(!r.ok)throw Error(j.error?.message??"Could not load connections.");if(active)setData(j.data);}).catch(e=>{if(active)setError(e instanceof Error?e.message:"Could not load connections.");});
    return ()=>{active=false;};
  },[open,api]);
  function close(){if(busy)return;setOpen(false);setSecret("");setCopied(false);setData(null);setError("");}
  async function command(body:unknown) {
    const r=await workFetch(`${api}/connections`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":crypto.randomUUID()},body:JSON.stringify(body)}),j=await r.json();
    if(!r.ok)throw Error(j.error?.message??"Could not update the connection.");
    return j.data;
  }
  async function create(form:HTMLFormElement) {
    const fields=new FormData(form);setBusy(true);setError("");
    try{const result=await command({name:fields.get("name"),scopeId:fields.get("scopeId"),expiresDays:Number(fields.get("expiresDays"))});setSecret(result.secret);setCopied(false);form.reset();await load();}catch(e){setError(e instanceof Error?e.message:"Could not create connection.");}finally{setBusy(false);}
  }
  async function revoke(id:string) {
    setBusy(true);setError("");
    try{await command({revokeId:id});await load();}catch(e){setError(e instanceof Error?e.message:"Could not revoke connection.");}finally{setBusy(false);}
  }
  return <>
    <Button appearance="standard" variant="secondary" className={styles.trigger} onClick={()=>setOpen(true)}><Plug size={16}/> External connections</Button>
    <Dialog appearance="standard" title="External connections" presentation="form-drawer" open={open} onClose={close} dismissDisabled={busy}>
      <div className={styles.body}>
        <p>Connect your team&apos;s agent to read Executive dashboards and KPIs using your current work role and the scope you choose.</p>
        <Field appearance="standard" label="MCP URL"><TextInput appearance="standard" readOnly value={typeof window==="undefined"?"/api/mcp/executive":`${window.location.origin}/api/mcp/executive`}/></Field>
        {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
        {!data&&!error&&<p role="status">Loading connections…</p>}
        {secret&&<section className={styles.secret} aria-label="New connection token">
          <h3>Save your token now</h3><p>This secret is shown once. Closing this drawer clears it. Keep it in your agent&apos;s secret store.</p>
          <Field appearance="standard" label="New token"><TextInput appearance="standard" readOnly value={secret} autoComplete="off" spellCheck={false}/></Field>
          <Button appearance="standard" variant="secondary" onClick={()=>void navigator.clipboard.writeText(secret).then(()=>setCopied(true)).catch(()=>setError("Copy the token from the field above."))}>{copied?"Copied":"Copy token"}</Button>
          <Button appearance="standard" variant="ghost" onClick={()=>setSecret("")}>I&apos;ve saved it</Button>
        </section>}
        {data?.canCreate&&!secret&&<form className={styles.form} onSubmit={e=>{e.preventDefault();void create(e.currentTarget);}}>
          <h3>Create a connection</h3>
          <Field appearance="standard" label="Connection name" required><TextInput appearance="standard" name="name" required maxLength={80} placeholder="Executive analysis agent" disabled={busy}/></Field>
          <Field appearance="standard" label="Data scope" hint="The token stays within this workspace and selected role."><Select appearance="standard" name="scopeId" required disabled={busy} defaultValue={data.scopes.find(s=>!s.branchId&&!s.departmentId)?.id??data.scopes[0]?.id}>{data.scopes.map(s=><option key={s.id} value={s.id}>{s.label}</option>)}</Select></Field>
          <Field appearance="standard" label="Expires after" hint="Expiry may be earlier if your work assignment ends."><Select appearance="standard" name="expiresDays" defaultValue="30" disabled={busy}><option value="7">7 days</option><option value="30">30 days</option><option value="90">90 days</option></Select></Field>
          <Button appearance="standard" type="submit" variant="primary" disabled={busy||!data.scopes.length}><Plus size={16}/>{busy?"Creating…":"Create token"}</Button>
        </form>}
        {data&&!data.canCreate&&<InlineNotice>Creating connections requires an active Executive access grant and an eligible executive work role. You can still revoke your existing connections.</InlineNotice>}
        {data&&<section className={styles.list} aria-label="Your connections"><h3>Your connections</h3>
          {!data.connections.length&&<p>No external connections yet.</p>}
          {data.connections.map(c=><article key={c.id} className={styles.connection}><div><strong>{c.name}</strong><span className={styles.status}>{c.status.replaceAll("_"," ")}</span></div><p>{c.role} · {c.scope}</p><p>Expires <TimeText appearance="standard" value={c.expiresAt}/> · {c.lastUsedAt?<>Last used <TimeText appearance="standard" value={c.lastUsedAt}/></>:"Never used"}</p>{c.status!=="revoked"&&<Button appearance="standard" variant="secondary" disabled={busy} onClick={()=>void revoke(c.id)}>Revoke {c.name}</Button>}</article>)}
        </section>}
      </div>
    </Dialog>
  </>;
}
