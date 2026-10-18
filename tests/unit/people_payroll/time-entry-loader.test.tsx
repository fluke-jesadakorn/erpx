import {expect,it,vi} from 'vitest';
import {createElement,type ComponentProps,type ComponentType} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
const pending=vi.hoisted(()=>new Map<unknown,()=>Promise<unknown>>());
// The real loader callbacks and real owner component execute. Only Next's
// scheduling wrapper is made explicit so SSR is observed after the load.
vi.mock('next/dynamic',()=>({default:<Props extends object>(load:()=>Promise<ComponentType<Props>>)=>{
 let Owner:ComponentType<Props>|undefined;function Lazy(props:Props){if(!Owner)throw Error('Expected deferred owner load before SSR');return createElement(Owner,props);}pending.set(Lazy,async()=>{Owner=await load();return Owner;});return Lazy;
}}));
vi.mock('@/components/people_payroll/payroll-controls',()=>{throw Error('Unrelated Payroll controls must stay deferred while loading Time entry');});
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>'/holdings/test/people-organization/people/person',useSearchParams:()=>new URLSearchParams('tab=pay')}));
import {TimeEntryEditor as LoadedTimeEntryEditor} from '@/components/records/loaders/modules--payroll-controls';
import {TimeEntryEditor} from '@/components/people_payroll/self-service/time-entry-editor';
import type {WorkforcePurchase} from '@/lib/people_payroll/workforce-controls';

it('loads only the actual Time owner from its existing lazy entry and preserves its props defaults markup and review controls',async()=>{
 const load=pending.get(LoadedTimeEntryEditor);if(!load)throw Error('Actual deferred Time entry loader required');expect(await load()).toBe(TimeEntryEditor);
 const active:WorkforcePurchase={newWork:true,settleExisting:true,history:true,revision:'opaque-original-workforce-revision'},draining={...active,newWork:false},history={...draining,settleExisting:false},props:ComponentProps<typeof TimeEntryEditor>={base:'/holdings/test',person:{engagement_id:'original-engagement',user_id:'employee'},entries:[{id:'original-time',version:7,work_date:'2026-10-16',regular_hours:8,overtime_hours:0,status:'submitted',submitted_by:'employee',note:'Original time record'}],canEdit:false,canApprove:true,userId:'independent-manager',workContext:'opaque-original-account-role',purchase:undefined};
 for(const purchase of [undefined,active,draining,history])for(const appearance of [undefined,'legacy','standard']as const){const input={...props,purchase,...(appearance?{appearance}:{})},actual=renderToStaticMarkup(createElement(LoadedTimeEntryEditor,input)),expected=renderToStaticMarkup(createElement(TimeEntryEditor,input));expect(actual).toBe(expected);expect(actual).toContain('Original time record');expect(actual).toContain('2026-10-16');expect(actual).toContain(`data-ui-appearance="${appearance??'standard'}"`);expect(actual).not.toContain('Payroll run');expect(actual).not.toContain('opaque-original-workforce-revision');expect(actual).not.toContain('opaque-original-account-role');const approve=actual.match(/<button[^>]*>Approve time<\/button>/)?.[0];expect(approve).toBeTruthy();if(purchase===history)expect(approve).toContain('disabled=""');else expect(approve).not.toContain('disabled=""');}
});
