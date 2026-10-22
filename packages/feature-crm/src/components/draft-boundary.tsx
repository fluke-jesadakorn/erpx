"use client";
import {Children,isValidElement,createContext,useContext,useEffect,useId,useRef,useState,type ComponentPropsWithRef,type ReactNode} from "react";
import {Dialog} from "@erpx/ui/components/dialog";
import {DraftFormDrawer} from "@erpx/ui/components/draft-form-drawer";
import {Button,InlineNotice} from "@erpx/ui/components/foundation";
import {contextIsSwitching,registerUnsavedWork,setContextSwitching} from "@erpx/feature-work_projection/lib/context-switch";
import {usePathname,useSearchParams} from "next/navigation";
const DraftScope=createContext<string|undefined>(undefined),InsideEditor=createContext(false);
type Messages={error:string;notice:string;busy:boolean;uncertain:boolean;retry:()=>Promise<void>};
const MessageContext=createContext<Messages|undefined>(undefined);
export function CRMCommandState({children,...value}:Messages&{children:ReactNode}){return <MessageContext.Provider value={value}>{children}</MessageContext.Provider>;}
export function CRMCommandFeedback(){const value=useContext(MessageContext);return value?<>{value.error&&<InlineNotice tone="danger" role="alert">{value.error}</InlineNotice>}{value.notice&&<InlineNotice tone="info" role="status">{value.notice}</InlineNotice>}{value.uncertain&&<Button appearance="standard" type="button" disabled={value.busy} onClick={()=>void value.retry()}>Retry submitted change</Button>}</>:null;}
function labelText(node:ReactNode):string{return typeof node==='string'||typeof node==='number'?String(node):isValidElement<{children?:ReactNode}>(node)?labelText(node.props.children):Children.toArray(node).map(labelText).join('');}
function submitLabel(node:ReactNode):string{for(const child of Children.toArray(node)){if(isValidElement<{type?:string;children?:ReactNode}>(child)){if(child.props.type==='submit')return labelText(child.props.children);const nested=submitLabel(child.props.children);if(nested)return nested;}}return '';}
export function CRMEditor({children,title,label,initialOpen=false,primary=false,disabled,onDiscard,onAccepted}:{children:ReactNode;title:string;label?:string;initialOpen?:boolean;primary?:boolean;disabled?:boolean;onDiscard?:()=>void;onAccepted?:()=>void}){
 const [open,setOpen]=useState(initialOpen),[dirty,setDirty]=useState(false),[generation,setGeneration]=useState(0),root=useRef<HTMLDivElement>(null),messages=useContext(MessageContext),dirtyForms=useRef(new Set<HTMLFormElement>());
 useEffect(()=>registerUnsavedWork(()=>open&&dirty),[open,dirty]);
 useEffect(()=>{const accepted=(event:Event)=>{const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;if(detail?.form&&root.current?.contains(detail.form)&&detail.form.getAttribute('data-erpx-edit-version')===detail.formEditVersion){dirtyForms.current.delete(detail.form);setDirty([...dirtyForms.current].some(form=>form.isConnected));onAccepted?.();setOpen(false);setGeneration(value=>value+1);}};window.addEventListener('erpx:crm-command-accepted',accepted);return()=>window.removeEventListener('erpx:crm-command-accepted',accepted);},[onAccepted]);
 const edit=(event:React.SyntheticEvent<HTMLDivElement>)=>{const form=event.target instanceof Element?event.target.closest<HTMLFormElement>('form[data-crm-draft]'):null;if(form){dirtyForms.current.add(form);setDirty(true);}};
 return <><Button appearance="standard" type="button" variant={primary?'primary':'secondary'} disabled={disabled} onClick={()=>setOpen(true)}>{label??title}</Button><DraftFormDrawer appearance="standard" title={title} open={open} dirty={dirty} dismissDisabled={messages?.busy} onClose={()=>setOpen(false)} onDiscard={()=>{window.dispatchEvent(new CustomEvent('erpx:record-saved',{detail:{root:root.current}}));onDiscard?.();dirtyForms.current.clear();setDirty(false);setOpen(false);setGeneration(value=>value+1);}}><InsideEditor.Provider value><div key={generation} ref={root} data-work-dirty={dirty?'true':undefined} onInputCapture={edit} onChangeCapture={edit} onClickCapture={event=>{if(event.target instanceof Element&&event.target.closest('button[type="button"]'))edit(event);}}><CRMCommandFeedback/>{children}</div></InsideEditor.Provider></DraftFormDrawer></>;
}
type FormProps=ComponentPropsWithRef<'form'>&{layout?:'grid'|'stack';inline?:boolean;editorTitle?:string;sourceVersion?:string|number;allowed?:boolean;onDiscard?:()=>void;onAccepted?:()=>void};
function InnerForm({children,sourceVersion,allowed=true,onSubmit,onInputCapture,onChangeCapture,onClickCapture,layout='grid',...props}:Omit<FormProps,'inline'|'editorTitle'|'onDiscard'|'onAccepted'>){const scope=useContext(DraftScope)??'customer-editor',[captured]=useState(sourceVersion),stale=captured!==sourceVersion,edited=(event:React.SyntheticEvent<HTMLFormElement>)=>event.currentTarget.setAttribute('data-erpx-edit-version',crypto.randomUUID());return <form {...props} className={props.className??(layout==='grid'?'ui-form-grid':'ui-stack')} data-slot="form-layout" data-layout={layout} data-crm-draft={scope} onSubmit={event=>{if(stale||!allowed){event.preventDefault();return;}onSubmit?.(event);}} onInputCapture={event=>{edited(event);onInputCapture?.(event);}} onChangeCapture={event=>{edited(event);onChangeCapture?.(event);}} onClickCapture={event=>{if(event.target instanceof Element&&event.target.closest('button[type="button"]'))edited(event);onClickCapture?.(event);}}>{stale&&<p role="alert">The source revision changed. Keep this draft for reference and reopen with the current record before submitting.</p>}{!allowed&&<p role="status">This action is unavailable under current access. Your draft is retained.</p>}<fieldset disabled={stale||!allowed} style={{display:'contents'}}>{Children.toArray(children)}</fieldset></form>;}
export function CRMForm({inline=false,editorTitle,allowed=true,onDiscard,onAccepted,...props}:FormProps){const inside=useContext(InsideEditor),form=<InnerForm {...props} allowed={allowed}/>;return inline||inside?form:<CRMEditor disabled={!allowed} title={editorTitle??(submitLabel(props.children)||'Customer action')} onDiscard={onDiscard} onAccepted={onAccepted}>{form}</CRMEditor>;}
export function CRMDisclosure({children,onDiscard,drawerTitle,open=false,disabled}:{children:ReactNode;appearance?:'standard';className?:string;dismissDisabled?:boolean;onDiscard?:()=>void;drawerTitle?:string;open?:boolean;disabled?:boolean}){const nodes=Children.toArray(children),summary=nodes.find(node=>isValidElement(node)&&node.type==='summary');return <CRMEditor title={drawerTitle??labelText(summary)} initialOpen={open} disabled={disabled} onDiscard={onDiscard} onAccepted={onDiscard}>{nodes.filter(node=>node!==summary)}</CRMEditor>;}

/** Keep owner choices and event-handler sources while any editor has newer
 * local work. Fresh purchase flags and source-version props still update. */
export function useCRMSnapshot<T>(value:T){const root=useRef<HTMLDivElement>(null),forms=useRef(new Set<HTMLFormElement>()),[held,setHeld]=useState<{value:T}|null>(null);
 useEffect(()=>{const clean=()=>{for(const form of forms.current)if(!form.isConnected)forms.current.delete(form);if(forms.current.size===0)setHeld(null);},saved=(event:Event)=>{const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null;crmAcknowledged?:boolean}>).detail;if(detail?.crmAcknowledged&&detail.form&&detail.form.getAttribute('data-erpx-edit-version')===detail.formEditVersion){forms.current.delete(detail.form);clean();}},discarded=(event:Event)=>{const discardedRoot=(event as CustomEvent<{root?:Element}>).detail?.root;if(discardedRoot){for(const form of forms.current)if(discardedRoot.contains(form))forms.current.delete(form);clean();}};window.addEventListener('erpx:record-command-saved',saved);window.addEventListener('erpx:record-saved',discarded);return()=>{window.removeEventListener('erpx:record-command-saved',saved);window.removeEventListener('erpx:record-saved',discarded);};},[]);
 const edit=(event:React.SyntheticEvent<HTMLDivElement>)=>{const form=event.target instanceof Element?event.target.closest<HTMLFormElement>('form[data-crm-draft]'):null;if(form&&root.current?.contains(form)){if(forms.current.size===0)setHeld({value});forms.current.add(form);}};
 return {snapshot:held?.value??value,rootProps:{ref:root,onInputCapture:edit,onChangeCapture:edit,onClickCapture:(event:React.MouseEvent<HTMLDivElement>)=>{if(event.target instanceof Element&&event.target.closest('button[type="button"]'))edit(event);}}};
}

/** Only marked mutation forms participate; route filters remain ordinary UI.
 * Matching edit-version acknowledgement preserves fields typed during a save. */
export function CRMDraftBoundary({children}: {children: ReactNode}) {
  const scope = useId(), forms = useRef(new Set<HTMLFormElement>()), trigger = useRef<HTMLAnchorElement | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const pathname = usePathname(), query = useSearchParams().toString();
  useEffect(() => {
    const current = forms.current, source = window.location.href, historyState = window.history.state;
    const dirty = () => {for (const form of current) if (!form.isConnected) current.delete(form); return current.size > 0;};
    const unregister = registerUnsavedWork(dirty);
    const edited = (event: Event) => {
      const form = event.target instanceof Element ? event.target.closest<HTMLFormElement>("form[data-crm-draft]") : null;
      if (!form || form.getAttribute("data-crm-draft") !== scope) return;
      current.add(form);
    };
    const saved = (event: Event) => {
      const detail = (event as CustomEvent<{form?: HTMLFormElement; formEditVersion?: string | null; crmAcknowledged?: boolean}>).detail;
      if (detail?.crmAcknowledged && detail.form && detail.formEditVersion === detail.form.getAttribute("data-erpx-edit-version")) current.delete(detail.form);
    };
    const discarded = (event: Event) => {
      const root = (event as CustomEvent<{root?: Element}>).detail?.root;
      if (root) for (const form of current) if (root.contains(form)) current.delete(form);
    };
    const click = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || !dirty()) return;
      const anchor = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!anchor || anchor.hasAttribute("download") || anchor.target && anchor.target !== "_self") return;
      const url = new URL(anchor.href);
      if (!["https:", "http:"].includes(url.protocol) || url.href === window.location.href || url.origin === window.location.origin && url.pathname === window.location.pathname && url.search === window.location.search) return;
      event.preventDefault(); event.stopImmediatePropagation(); trigger.current = anchor; setDestination(url.href);
    };
    const unload = (event: BeforeUnloadEvent) => {if (dirty() && !contextIsSwitching()) event.preventDefault();};
    const navigation = (window as Window & {navigation?: EventTarget}).navigation;
    const traverse = (event: Event) => {
      const next = event as Event & {navigationType?: string; destination?: {url: string}};
      if (next.navigationType === "traverse" && next.cancelable && next.destination?.url !== source && dirty()) {next.preventDefault(); setDestination(next.destination?.url ?? null);}
    };
    const back = (event: PopStateEvent) => {
      if (window.location.href !== source && dirty()) {event.stopImmediatePropagation(); const next = window.location.href; window.history.pushState(historyState, "", source); setDestination(next);}
    };
    const buttonEdit = (event: MouseEvent) => {if (event.target instanceof Element && event.target.closest('button[type="button"]')) edited(event);};
    document.addEventListener("input", edited); document.addEventListener("change", edited); document.addEventListener("click", buttonEdit); document.addEventListener("click", click, true);
    window.addEventListener("erpx:record-command-saved", saved); window.addEventListener("erpx:record-saved", discarded);
    window.addEventListener("beforeunload", unload); window.addEventListener("popstate", back, true); navigation?.addEventListener("navigate", traverse);
    return () => {
      unregister(); document.removeEventListener("input", edited); document.removeEventListener("change", edited); document.removeEventListener("click", buttonEdit); document.removeEventListener("click", click, true);
      window.removeEventListener("erpx:record-command-saved", saved); window.removeEventListener("erpx:record-saved", discarded);
      window.removeEventListener("beforeunload", unload); window.removeEventListener("popstate", back, true); navigation?.removeEventListener("navigate", traverse);
    };
  }, [scope, pathname, query]);
  return <DraftScope.Provider value={scope}>{children}<Dialog appearance="standard" open={destination !== null} title="Leave unsaved customer work?" onClose={() => setDestination(null)} finalFocus={() => trigger.current}>
    <p>Your changes have not been saved. Leaving will discard this local draft.</p><div className="ui-form-actions">
      <Button appearance="standard" data-autofocus onClick={() => setDestination(null)}>Keep editing</Button>
      <Button appearance="standard" variant="danger" onClick={() => {if (destination) {setContextSwitching(true); window.location.assign(destination);}}}>Discard and leave</Button>
    </div></Dialog></DraftScope.Provider>;
}
