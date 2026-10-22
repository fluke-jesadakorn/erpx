"use client";
import {useEffect,useRef,useState,type ComponentProps,type ReactNode} from "react";
import {DraftFormDrawer} from "@erpx/ui/components/draft-form-drawer";
import {registerUnsavedWork} from "@erpx/feature-work_projection/lib/context-switch";

/** The current page keeps the edited form alive until dismissal is confirmed.
 * Only the submitted edit version acknowledges a draft; later typing survives. */
export function PeopleFormDrawer({inline=false,saved=false,appearance="legacy",children,...props}:Omit<ComponentProps<typeof DraftFormDrawer>,"dirty">&{inline?:boolean;saved?:boolean;children:ReactNode}){
 const root=useRef<HTMLDivElement>(null),forms=useRef(new Set<HTMLFormElement>()),[dirty,setDirty]=useState(false);
 useEffect(()=>{
  const active=()=>{for(const form of forms.current)if(!form.isConnected)forms.current.delete(form);return forms.current.size>0;},unregister=registerUnsavedWork(active);
  const saved=(event:Event)=>{const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(detail?.form&&detail.formEditVersion===detail.form.getAttribute("data-erpx-edit-version")){forms.current.delete(detail.form);setDirty(active());}};
  const discarded=(event:Event)=>{const container=(event as CustomEvent<{root?:Element}>).detail?.root;if(container&&root.current&&container.contains(root.current)){forms.current.clear();setDirty(false);}};
  window.addEventListener("erpx:record-saved",discarded);window.addEventListener("erpx:record-command-saved",saved);return()=>{unregister();window.removeEventListener("erpx:record-saved",discarded);window.removeEventListener("erpx:record-command-saved",saved);};
 },[]);
 const content=<div ref={root} data-people-draft data-work-dirty={dirty&&!saved?"true":undefined} onInputCapture={event=>{const form=(event.target as Element).closest("form");if(form){form.setAttribute("data-erpx-edit-version",crypto.randomUUID());forms.current.add(form);setDirty(true);}}} onChangeCapture={event=>{const form=(event.target as Element).closest("form");if(form){form.setAttribute("data-erpx-edit-version",crypto.randomUUID());forms.current.add(form);setDirty(true);}}}>{children}</div>;
 if(inline)return content;
 return <DraftFormDrawer {...props} appearance={appearance} dirty={dirty&&!saved} onDiscard={()=>{if(props.dismissDisabled)return;window.dispatchEvent(new CustomEvent("erpx:record-saved",{detail:{root:root.current}}));forms.current.clear();setDirty(false);(props.onDiscard??props.onClose)();}}>{content}</DraftFormDrawer>;
}
