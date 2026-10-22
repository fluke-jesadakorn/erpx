"use client";
import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { Button, Card, Field, PageContent, TextInput } from "@erpx/ui/components/foundation";

type Completion = { portalUrl: string };
function createInvitationTokenSource(){
  let token:string|undefined;const listeners=new Set<()=>void>();
  const read=()=>{if(typeof window==="undefined")return "";return token??=new URLSearchParams(window.location.hash.slice(1)).get("token")??"";};
  const sync=()=>{if(typeof window==="undefined")return;const next=new URLSearchParams(window.location.hash.slice(1)).get("token")??"";if(next!==token){token=next;for(const listener of listeners)listener();}};
  return {
    subscribe(listener:()=>void){listeners.add(listener);if(typeof window!=="undefined"){window.addEventListener("hashchange",sync);window.addEventListener("popstate",sync);}return()=>{listeners.delete(listener);if(typeof window!=="undefined"){window.removeEventListener("hashchange",sync);window.removeEventListener("popstate",sync);}};},
    getSnapshot:read,
    getServerSnapshot:()=>"",
    scrub(){if(typeof window!=="undefined"&&read()&&window.location.hash)window.history.replaceState(null,"",window.location.pathname+window.location.search);},
  };
}
function messageFrom(body: unknown) {
  if (!body || typeof body!=="object") return "The invitation could not be completed.";
  const value=body as {error?:{message?:unknown};message?:unknown};
  return typeof value.error?.message==="string"?value.error.message:typeof value.message==="string"?value.message:"The invitation could not be completed.";
}

export function CustomerPortalEnrollment({verifiedReturn=false}:{verifiedReturn?:boolean}) {
  const tokenSource=useMemo(()=>createInvitationTokenSource(),[]),token=useSyncExternalStore(tokenSource.subscribe,tokenSource.getSnapshot,tokenSource.getServerSnapshot),[message,setMessage]=useState(""),[busy,setBusy]=useState(false),[completion,setCompletion]=useState<Completion|null>(null),[verificationRetryNeeded,setVerificationRetryNeeded]=useState(false);
  useEffect(()=>{tokenSource.scrub();},[tokenSource]);
  async function submit(action:"signup"|"accept",values:Record<string,string>={}){
    if(!token){setMessage("Open the complete invitation link from your email.");return;}
    setBusy(true);setMessage("");if(action==='signup')setVerificationRetryNeeded(false);
    try{
      const response=await fetch("/api/customer-portal/enroll",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({action,token,...values})});
      const body=await response.json().catch(()=>null);
      if(!response.ok){
        const code=body&&typeof body==='object'?(body as {error?:{code?:unknown}}).error?.code:null;
        if(action==='signup'&&code==='VERIFICATION_EMAIL_DELIVERY_FAILED')setVerificationRetryNeeded(true);
        setMessage(messageFrom(body));return;
      }
      if(action==="signup")setMessage("Check the invited email address and verify your account. Then reopen this invitation link and sign in to accept it.");
      else {const data=(body as {data?:Completion})?.data;if(data?.portalUrl){setCompletion(data);setMessage("Invitation accepted. You can now open the Customer Portal for this approved document.");}else setMessage("Invitation accepted. Reopen the Customer Portal from your account.");}
    }catch{setMessage("Unable to complete the invitation. Try again.");}
    finally{setBusy(false);}
  }
  return <PageContent as="main" className="ui-page-content">
    <h1>Accept your Customer Portal invitation</h1>
    <p>This invitation is for one approved customer document. Verify the invited email address, then accept it while signed in to that exact account.</p>
    {!token&&!completion&&<Card as="section" appearance="standard" className="ui-panel"><p>Open the complete private invitation link from your email. The link is removed from the address bar after it is read.</p></Card>}
    {verifiedReturn&&!completion&&<Card as="section" appearance="standard" className="ui-panel"><p>Your email verification step is complete. Reopen the invitation email and sign in to accept the approved document.</p></Card>}
    {token&&!completion&&<>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Create the invited account</h2>
        <p>The account email is fixed to the address that received this invitation. After email verification, reopen the invitation and sign in before accepting.</p>
        <form className="ui-form-grid" onSubmit={event=>{event.preventDefault();const values=new FormData(event.currentTarget);void submit("signup",{name:String(values.get("name")??""),password:String(values.get("password")??"")});}}>
          <Field appearance="standard">Your name<TextInput appearance="standard" name="name" autoComplete="name" required maxLength={100}/></Field>
          <Field appearance="standard">Choose a password<TextInput appearance="standard" name="password" type="password" autoComplete="new-password" required minLength={12} maxLength={200}/></Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>{busy?"Sending verification…":"Create invited account"}</Button>
        </form>
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Already have an account?</h2>
        <p>Sign in with the invited email address, then reopen the complete link in your invitation email.</p>
        <a href="/login">Sign in to ERPX Account</a>
        <form onSubmit={event=>{event.preventDefault();void submit("accept");}}>
          <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>{busy?"Accepting…":"Accept while signed in"}</Button>
        </form>
      </Card>
    </>}
    {completion&&<Card as="section" appearance="standard" className="ui-panel"><a href={completion.portalUrl}>Open Customer Portal</a></Card>}
    {message&&<p role="status" aria-live="polite">{message}</p>}
    {verificationRetryNeeded&&<p>If an account was created, request a fresh verification email for the invited address from <a href="/verify-email">Resend verification email</a>.</p>}
  </PageContent>;
}
