import {afterEach,expect,it,vi} from 'vitest';
import {createElement,isValidElement,type ComponentProps} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import type {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';

const lifecycle=vi.hoisted(()=>({active:false,dirty:false,refIndex:0,refs:[] as {current:unknown}[],effect:undefined as (()=>(()=>void)|void)|undefined,cleanup:undefined as (()=>void)|undefined}));
vi.mock('react',async original=>{
 const react=await original<typeof import('react')>();
 return {...react,useState:(initial:boolean)=>lifecycle.active?[lifecycle.dirty,(next:boolean)=>{lifecycle.dirty=next;}]:react.useState(initial),useRef:(initial:unknown)=>{if(!lifecycle.active)return react.useRef(initial);const index=lifecycle.refIndex++;return lifecycle.refs[index]??(lifecycle.refs[index]={current:initial});},useEffect:(effect:()=>void|(()=>void),dependencies:readonly unknown[])=>{if(!lifecycle.active)return react.useEffect(effect,dependencies);lifecycle.effect??=effect;}};
});
// Only the portal primitive is replaced. The actual People wrapper owns the
// forwarded appearance, form ancestry and original dirty/acknowledgement logic.
vi.mock("@erpx/ui/components/draft-form-drawer",()=>({DraftFormDrawer:(props:ComponentProps<typeof DraftFormDrawer>)=>createElement('section',{'data-test-drawer-appearance':props.appearance??'standard'},props.children)}));
import {PeopleFormDrawer} from '@erpx/feature-people_payroll/components/people-form-drawer';
import {hasUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';

afterEach(()=>{lifecycle.cleanup?.();lifecycle.active=false;lifecycle.dirty=false;lifecycle.refIndex=0;lifecycle.refs=[];lifecycle.effect=undefined;lifecycle.cleanup=undefined;vi.unstubAllGlobals();});
const close=()=>{};
it('retains the legacy appearance default at the actual People drawer boundary and keeps standard an explicit opt-in',()=>{
 const child=createElement('form',{className:'retained-specialized-form'},createElement('input',{name:'original',defaultValue:'Original employee draft'}));
 const render=(appearance?:'legacy'|'standard')=>{const props={open:true,onClose:close,title:'Original People editor',...(appearance?{appearance}:{}),children:child};return renderToStaticMarkup(createElement(PeopleFormDrawer,props));};
 expect(render()).toBe(render('legacy'));expect(render()).toContain('data-test-drawer-appearance="legacy"');expect(render('standard')).toContain('data-test-drawer-appearance="standard"');for(const html of [render(),render('standard')]){expect(html).toContain('class="retained-specialized-form"');expect(html).toContain('value="Original employee draft"');expect(html.match(/<form\b/g)).toHaveLength(1);}
});
it('preserves the full-page inline form ancestry without adding drawer markup or replacing caller classes',()=>{
 const props={inline:true,open:true,onClose:close,title:'Dedicated People page',children:createElement('form',{className:'retained-full-page'},createElement('input',{name:'original',defaultValue:'Retained full-page draft'}))},html=renderToStaticMarkup(createElement(PeopleFormDrawer,props));expect(html).not.toContain('data-test-drawer');expect(html).toMatch(/^<div data-people-draft="true"><form class="retained-full-page">/);expect(html).toContain('Retained full-page draft');
});
it('retains a later People edit after a captured acknowledgement and clears only the matching original edit or explicit discard',()=>{
 const windowEvents=new EventTarget();vi.stubGlobal('window',windowEvents);vi.stubGlobal('document',{querySelector:()=>null});lifecycle.active=true;const onClose=vi.fn(),onDiscard=vi.fn(),props={open:true,onClose,onDiscard,title:'People dirty draft',children:createElement('form',{className:'retained-form'})};
 const render=()=>{lifecycle.refIndex=0;const tree=PeopleFormDrawer(props);if(!isValidElement<ComponentProps<typeof DraftFormDrawer>>(tree))throw Error('Actual People drawer element required');return tree;};
 let tree=render();lifecycle.cleanup=lifecycle.effect?.()??undefined;const content=tree.props.children;if(!isValidElement<ComponentProps<'div'>>(content)||!content.props.onInputCapture)throw Error('Actual People input boundary required');let edit:string|null=null;const form={isConnected:true,setAttribute:(name:string,value:string)=>{if(name==='data-erpx-edit-version')edit=value;},getAttribute:()=>edit};
 Reflect.apply(content.props.onInputCapture,undefined,[{target:{closest:()=>form}}]);const submitted=edit;expect(submitted).toBeTruthy();expect(hasUnsavedWork()).toBe(true);Reflect.apply(content.props.onInputCapture,undefined,[{target:{closest:()=>form}}]);expect(edit).not.toBe(submitted);windowEvents.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:submitted}}));tree=render();expect(tree.props.dirty).toBe(true);expect(hasUnsavedWork()).toBe(true);expect(onClose).not.toHaveBeenCalled();
 windowEvents.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:edit}}));tree=render();expect(tree.props.dirty).toBe(false);expect(hasUnsavedWork()).toBe(false);Reflect.apply(content.props.onInputCapture,undefined,[{target:{closest:()=>form}}]);tree=render();expect(tree.props.dirty).toBe(true);tree.props.onDiscard?.();expect(onDiscard).toHaveBeenCalledTimes(1);expect(onClose).not.toHaveBeenCalled();expect(render().props.dirty).toBe(false);expect(hasUnsavedWork()).toBe(false);
});
