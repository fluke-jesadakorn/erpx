'use client';
import {useCallback,useEffect,useRef,useState} from 'react';
import {useRouter} from 'next/navigation';
import {Button,Field,InlineNotice,Panel,Stack,TextArea} from '@/components/ui/foundation';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {TimeText} from '@/components/people_payroll/time/time-preference-context';
import {contextGeneration,registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {createAccountSecurityCommand} from '@/lib/identity_access/account-security-client';
import type {AccountSecurityInput,AccountSecuritySeed} from '@/lib/identity_access/account-security';
import {accountSecuritySeedSchema} from '@/lib/identity_access/account-security';

export function AccountSessions({initial}:{initial:AccountSecuritySeed}){
 const [seed,setSeed]=useState(initial),[selection,setSelection]=useState<{scope:'family'|'all';family?:string}|null>(null),[reason,setReason]=useState(''),[busy,setBusy]=useState(false),[message,setMessage]=useState(''),[error,setError]=useState(''),[invalidated,setInvalidated]=useState(false),[uncertain,setUncertain]=useState<AccountSecurityInput|null>(null);
 const [received,setReceived]=useState(initial);
 if(received!==initial){setReceived(initial);if(!selection&&!uncertain&&!busy)setSeed(initial);}
 const command=useRef(createAccountSecurityCommand()),unknown=useRef(false),lifecycle=useRef({epoch:0}),router=useRouter();
 const reload=useCallback(async()=>{const generation=contextGeneration(),current=++lifecycle.current.epoch;setBusy(true);setError('');try{const response=await fetch('/api/v1/account/security',{cache:'no-store'}),body=await response.json();if(current!==lifecycle.current.epoch||generation!==contextGeneration())return;if(!response.ok)throw Error(body.error?.message??'Could not reload your sessions.');const next=accountSecuritySeedSchema.parse(body.data);if(next.accountKey!==initial.accountKey){setInvalidated(true);return;}setSeed(next);}catch(cause){if(current===lifecycle.current.epoch)setError(cause instanceof Error?cause.message:'Could not reload your sessions.');}finally{if(current===lifecycle.current.epoch)setBusy(false);}},[initial.accountKey]);
 useEffect(()=>{const source=lifecycle.current,unregister=registerUnsavedWork(()=>unknown.current),verified=()=>{void reload();},changed=()=>{source.epoch++;setInvalidated(true);setBusy(false);};window.addEventListener('erpx:account-security-updated',verified);window.addEventListener('erpx:context-switch-started',changed);window.addEventListener('erpx:work-context-changed',changed);return()=>{source.epoch++;unregister();window.removeEventListener('erpx:account-security-updated',verified);window.removeEventListener('erpx:context-switch-started',changed);window.removeEventListener('erpx:work-context-changed',changed);};},[reload]);
 async function revoke(input:AccountSecurityInput){
  if(busy||invalidated)return;setBusy(true);setError('');const generation=contextGeneration(),current=++lifecycle.current.epoch;
  try{const result=await command.current.revoke(input);if(current!==lifecycle.current.epoch||generation!==contextGeneration())return;unknown.current=false;setUncertain(null);setSelection(null);setReason('');if(result.reauthenticate){router.replace('/login?next=%2Fsecurity');router.refresh();return;}setMessage(`${result.count} session${result.count===1?'':'s'} signed out.`);await reload();}
  catch(cause){if(current===lifecycle.current.epoch){unknown.current=command.current.hasPending();setUncertain(unknown.current?input:null);setError(cause instanceof Error?cause.message:'Could not revoke this sign-in.');}}
  finally{if(current===lifecycle.current.epoch)setBusy(false);}
 }
 return <Stack><h2>Sign-ins & devices</h2><p>Each sign-in includes its product sessions. Signing out a family also stops those products; signing out all devices ends all your account sessions. Verify an authenticator code before revoking sessions.</p>
  <div><Button appearance="standard" variant="secondary" disabled={busy||invalidated||!!uncertain} onClick={()=>void reload()}>Reload sessions</Button></div>
  {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{message&&<InlineNotice tone="success" role="status">{message}</InlineNotice>}
  {uncertain&&<div><Button appearance="standard" disabled={busy||invalidated} onClick={()=>void revoke(uncertain)}>Retry original security request</Button></div>}
  {invalidated&&<InlineNotice tone="warning">Your account changed. Reload the page before continuing.</InlineNotice>}
  {seed.families.map(family=><Panel appearance="standard" key={family.handle}><Stack><h3>{family.current?'This sign-in':'Another sign-in'}</h3><p>Signed in <TimeText appearance="standard" value={family.createdAt}/> · expires <TimeText appearance="standard" value={family.expiresAt}/></p><p>{family.userAgent??'Browser details unavailable'}</p><p>Products: {family.audiences.length?family.audiences.join(', '):'Account only'}</p><div><Button appearance="standard" variant="secondary" disabled={busy||invalidated||!!uncertain||!seed.mfaEnabled} onClick={()=>{setReason('');setSelection({scope:'family',family:family.handle});}}>Sign out this sign-in</Button></div></Stack></Panel>)}
  {seed.legacySessions>0&&<p>{seed.legacySessions} other account session{seed.legacySessions===1?'':'s'} can also be ended by signing out all devices.</p>}
  <div><Button appearance="standard" variant="danger" disabled={busy||invalidated||!!uncertain||!seed.mfaEnabled} onClick={()=>{setReason('');setSelection({scope:'all'});}}>Sign out all devices</Button></div>
  <DraftFormDrawer appearance="standard" open={!!selection} title={selection?.scope==='all'?'Sign out all devices':'Sign out this sign-in'} dirty={!!reason} dismissDisabled={busy||!!uncertain} onClose={()=>{setSelection(null);setReason('');}}><form onSubmit={event=>{event.preventDefault();if(selection)void revoke({...selection,accountKey:seed.accountKey,expectedRevision:seed.revision,reason});}}><Stack><p>{selection?.scope==='all'?'You will need to sign in again on every device.':'This ends the selected sign-in and its product sessions.'}</p><Field appearance="standard" label="Security review reason" required><TextArea appearance="standard" value={reason} minLength={3} maxLength={1000} required disabled={busy||!!uncertain} onChange={event=>setReason(event.target.value)}/></Field><Button appearance="standard" variant="danger" type="submit" disabled={busy||invalidated||!!uncertain}>Confirm sign out</Button></Stack></form></DraftFormDrawer>
 </Stack>;
}
