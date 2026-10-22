'use client';
import {useEffect,useRef,useState,type ReactNode} from 'react';
import {FormDisclosure} from '@erpx/ui/components/form-disclosure';
import {Button,InlineNotice,type UiAppearance} from '@erpx/ui/components/foundation';
import {Dialog} from '@erpx/ui/components/dialog';
import {useEmbeddedRecord} from '@erpx/ui/components/record-context';
import {useWorkspaceOverlay} from '@erpx/ui/components/workspace-overlay';
import {registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
/** Portal forms remain mounted while dirty; discard or an unchanged parsed
 * acknowledgement explicitly resets only this editor. */
export function RoutineEditor({title,busy,error,onReset,recovery,children,appearance='standard'}:{title:string;busy:boolean;error?:string;onReset?:()=>void;recovery?:ReactNode;children:ReactNode;appearance?:UiAppearance}){
 const forms=useRef(new Set<HTMLFormElement>()),[generation,setGeneration]=useState(0),[embeddedOpen,setEmbeddedOpen]=useState(false),[embeddedDirty,setEmbeddedDirty]=useState(false),embedded=useEmbeddedRecord(),workspace=useWorkspaceOverlay(),embeddedSurface=embedded&&!workspace.active;
 useEffect(()=>registerUnsavedWork(()=>embeddedSurface&&embeddedDirty),[embeddedSurface,embeddedDirty]);
 useEffect(()=>{const saved=(event:Event)=>{const detail=(event as CustomEvent<{routineAcknowledged?:boolean;form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(detail?.routineAcknowledged&&detail.form&&forms.current.has(detail.form)&&detail.formEditVersion===detail.form.getAttribute('data-erpx-edit-version')){onReset?.();forms.current.clear();setEmbeddedOpen(false);setEmbeddedDirty(false);setGeneration(value=>value+1);}};window.addEventListener('erpx:record-command-saved',saved);return()=>window.removeEventListener('erpx:record-command-saved',saved);},[onReset]);
 const capture=(target:EventTarget,edit=false)=>{if(target instanceof Element){const form=target.closest('form');if(form?.hasAttribute('data-routine-draft')){forms.current.add(form);if(embeddedSurface&&edit){form.setAttribute('data-erpx-edit-version',crypto.randomUUID());setEmbeddedDirty(true);}}}};
 const discard=()=>{forms.current.forEach(form=>form.reset());forms.current.clear();onReset?.();setEmbeddedDirty(false);setEmbeddedOpen(false);setGeneration(value=>value+1);};
 // An embedded Dialog already owns its original dirty/Keep/Discard guard.
 // Wrapping that guard in DraftFormDrawer asks for the same discard twice.
 const editor=embeddedSurface?<><Button appearance={appearance} clientManaged variant="secondary" disabled={busy} onClick={()=>setEmbeddedOpen(true)}>{title}</Button><Dialog key={generation} appearance={appearance} open={embeddedOpen} onClose={discard} title={title} presentation="form-drawer" dismissDisabled={busy}>{error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{recovery}{children}</Dialog></>:<FormDisclosure key={generation} appearance={appearance} drawerTitle={title} dismissDisabled={busy} error={error} onDiscard={discard}><summary>{title}</summary>{recovery}{children}</FormDisclosure>;
 return <div onSubmitCapture={event=>capture(event.target)} onChangeCapture={event=>capture(event.target,true)} onInputCapture={event=>capture(event.target,true)}>{editor}</div>;
}
