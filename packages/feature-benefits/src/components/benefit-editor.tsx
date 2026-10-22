"use client";
import {useEffect,useRef,useState,type ReactNode} from 'react';
import {FormDisclosure} from '@erpx/ui/components/form-disclosure';
/** Discard genuinely resets this editor. Keep editing retains its mounted form. */
export function BenefitEditor({title,busy,error,onReset,recovery,children}:{title:string;busy:boolean;error?:string;onReset?:()=>void;recovery?:ReactNode;children:ReactNode}){
 const forms=useRef(new Set<HTMLFormElement>()),[generation,setGeneration]=useState(0);
 useEffect(()=>{const saved=(event:Event)=>{const detail=(event as CustomEvent<{benefitAcknowledged?:boolean;form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(detail?.benefitAcknowledged&&detail.form&&forms.current.has(detail.form)&&detail.formEditVersion===detail.form.getAttribute('data-erpx-edit-version'))setGeneration(value=>value+1);};window.addEventListener('erpx:record-command-saved',saved);return()=>window.removeEventListener('erpx:record-command-saved',saved);},[]);
 const capture=(target:EventTarget)=>{if(target instanceof Element){const form=target.closest('form');if(form?.hasAttribute('data-benefit-draft'))forms.current.add(form);}};
 return <div onSubmitCapture={event=>capture(event.target)} onChangeCapture={event=>capture(event.target)} onInputCapture={event=>capture(event.target)}><FormDisclosure key={generation} appearance="standard" drawerTitle={title} dismissDisabled={busy} error={error} onDiscard={()=>{forms.current.forEach(form=>{form.reset();window.dispatchEvent(new CustomEvent('erpx:benefit-draft-discarded',{detail:{form}}));});forms.current.clear();onReset?.();setGeneration(value=>value+1);}}><summary>{title}</summary>{recovery}{children}</FormDisclosure></div>;
}
