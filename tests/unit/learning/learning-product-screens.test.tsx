import {afterEach,expect,it,vi} from 'vitest';
import type {ReactElement} from 'react';

const ports=vi.hoisted(()=>({allowed:true,handbook:vi.fn(()=>{throw Error('WORK_PACKAGE_REQUIRED');}),learning:vi.fn<(...args:[unknown,boolean])=>Promise<unknown>>(async()=>({courses:[],assignments:[],people:[],canWrite:false,canReview:false,actorId:'learner'}))}));
vi.mock('@/server/modules/identity_access/public/authorization',()=>({requireWorkContext:async()=>({holdingId:'H001',modules:['learning']}),publicScope:()=>({kind:'holding',holdingSlug:'test',holdingId:'H001',holdingName:'Test',companyId:'company'})}));
vi.mock('@/server/modules/collaboration/public/presentation-queries',()=>({collaborationPresentationQueries:async()=>({handbookAccess:ports.handbook})}));
vi.mock('@/server/modules/learning/public/learning',()=>({learningWorkspace:ports.learning}));
vi.mock('@/data/module-access',()=>({hasModuleAccess:()=>ports.allowed}));
vi.mock('@/lib/identity_access/permissions',()=>({hasPermission:()=>ports.allowed}));
vi.mock('next/navigation',()=>({forbidden:()=>{throw Error('FORBIDDEN');},notFound:()=>{throw Error('NOT_FOUND');}}));
vi.mock('@/server/modules/work_projection/public/page-error',()=>({pageError:(error:unknown)=>{throw error;}}));
vi.mock('@/components/workplace/section-workspace',()=>({SectionWorkspace:()=>null}));
vi.mock('@/components/learning/learning-console',()=>({LearningConsole:()=>null}));
vi.mock('@/components/records/record-catalog',()=>({RecordCatalog:()=>null}));
vi.mock('@/components/records/record-link',()=>({RecordLink:()=>null}));
vi.mock('@/components/ui/list-filters',()=>({ListFilters:()=>null}));

const screens=async()=>[
 (await import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/courses/_screen')).default,
 (await import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/assignments/_screen')).default,
 (await import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/mine/_screen')).default,
];
afterEach(()=>{ports.allowed=true;ports.handbook.mockClear();ports.learning.mockClear();});

it('renders all three original Learning screens without acquiring another product handbook entitlement',async()=>{
 const selected=['learning:courses','learning:assignments','learning:mine'];
 for(const [index,Screen] of (await screens()).entries()){
  const result=await Screen({params:Promise.resolve({holdingSlug:'test'}),searchParams:Promise.resolve({q:'retained filter'})}) as ReactElement<{selected:string;items:{id:string}[]}>;
  expect(result.props.selected).toBe(selected[index]);expect(result.props.items.map(item=>item.id)).toEqual(selected);
 }
 expect(ports.handbook).not.toHaveBeenCalled();expect(ports.learning.mock.calls.map(call=>call[1])).toEqual([false,false,true]);
});

it('retains the original module and record authority checks before loading Learning data',async()=>{
 ports.allowed=false;
 for(const Screen of await screens())await expect(Screen({params:Promise.resolve({holdingSlug:'test'}),searchParams:Promise.resolve({})})).rejects.toThrow('FORBIDDEN');
 expect(ports.learning).not.toHaveBeenCalled();expect(ports.handbook).not.toHaveBeenCalled();
});

// Exercise the actual canonical wrappers and their actual Learning presenters.
// Only layout/catalog/filter slots are rendered as small semantic test ports.
import {createElement,Fragment,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import * as learningWorkspaceFrame from '@/components/workplace/section-workspace';
import * as learningRecordCatalog from '@/components/records/record-catalog';
import * as learningListFilters from '@/components/ui/list-filters';
vi.mock('@/components/workplace/destination-index',()=>({DestinationIndex:()=>null}));
vi.mock('@/components/workplace/handbook-controls',()=>({HandbookEditors:()=>null}));
vi.mock('@/server/modules/organization/public/member-directory',()=>({companyMembers:vi.fn(()=>{throw Error('UNRELATED_COMPANY_DIRECTORY_READ');})}));
vi.mock('@/server/modules/collaboration/public/handbook-purchases',()=>({handbookPresentationContext:()=>{throw Error('UNRELATED_HANDBOOK_CONTEXT');}}));

const learningDeepLinks=[
 {name:'assignment root',kind:'learning_assignment',section:'assignments',explicitSection:false,screen:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/assignments/_screen'),page:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/page')},
 {name:'assignment section',kind:'learning_assignment',section:'assignments',explicitSection:true,screen:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/assignments/_screen'),page:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/assignments/[id]/[recordSection]/page')},
 {name:'course root',kind:'learning_course',section:'courses',explicitSection:false,screen:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/courses/_screen'),page:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/page')},
 {name:'course section',kind:'learning_course',section:'courses',explicitSection:true,screen:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/courses/_screen'),page:()=>import('../../../src/app/holdings/[holdingSlug]/knowledge/learning/records/learning_course/[id]/[recordSection]/page')},
];

it.each(learningDeepLinks)('server-renders the actual $name deep link with its Learning background and retained record/query context',async route=>{
 const screen=await route.screen(),original=screen.default,received:Record<string,string|string[]|undefined>[]=[],frame=vi.spyOn(learningWorkspaceFrame,'SectionWorkspace').mockImplementation(({children,selected}:{children:ReactNode;selected:string})=>createElement('section',{'data-learning-section':selected},children)),
  catalog=vi.spyOn(learningRecordCatalog,'RecordCatalog').mockImplementation(({rows}:{rows:Record<string,unknown>[]})=>createElement('ul',{'data-course-catalog':true},rows.map(row=>createElement('li',{key:String(row.id)},String(row.title))))),
  filters=vi.spyOn(learningListFilters,'ListFilters').mockImplementation(()=>createElement(Fragment)),
  actualScreen=vi.spyOn(screen,'default').mockImplementation(async props=>{const query=await props.searchParams;received.push(query);return original({...props,searchParams:Promise.resolve(query)});});
 try{
  ports.learning.mockResolvedValueOnce({courses:[{id:'actual-course',title:'Retained course',version:2,status:'published',created_by:'reviewer'}],assignments:[{id:'actual-assignment',course_title:'Retained course',learner_name:'Learner',status:'assigned'}],people:[],canWrite:false,canReview:false,actorId:'learner'});
  const Page=(await route.page()).default,params={holdingSlug:'test',id:'actual-source',recordSection:'overview'},query={q:'Retained',cursor:'original-page',tag:['one','two'],returnTo:'/holdings/test/knowledge/learning/mine',view:'handbook',section:'drafts',detailKind:'wrong-kind',detailId:'wrong-source'},
   result=await Page({params:Promise.resolve(params),searchParams:Promise.resolve(query)}) as ReactElement<{selected:string}>;
  expect(result.props.selected).toBe(`learning:${route.section}`);
  expect(received).toEqual([{...query,view:'learning',section:route.section,detailKind:route.kind,detailId:'actual-source'}]);
  const html=renderToStaticMarkup(result);
  expect(html).toContain(`data-learning-section="learning:${route.section}"`);
  expect(html).toContain(route.kind==='learning_course'?'Retained course':'assigned');
  expect(ports.handbook).not.toHaveBeenCalled();expect(ports.learning.mock.calls).toHaveLength(1);expect(ports.learning.mock.calls[0]?.[1]).toBe(false);
  expect(filters).toHaveBeenCalledWith(expect.objectContaining({query:'Retained',preserve:expect.objectContaining({tag:['one','two'],returnTo:query.returnTo,view:'learning',section:route.section})}),undefined);
  if(route.explicitSection){const reads=ports.learning.mock.calls.length;await expect(Page({params:Promise.resolve({...params,recordSection:'not-a-record-section'}),searchParams:Promise.resolve(query)})).rejects.toThrow('NOT_FOUND');expect(ports.learning.mock.calls).toHaveLength(reads);}
  ports.allowed=false;await expect(Page({params:Promise.resolve(params),searchParams:Promise.resolve(query)})).rejects.toThrow('FORBIDDEN');expect(ports.handbook).not.toHaveBeenCalled();
 }finally{actualScreen.mockRestore();filters.mockRestore();catalog.mockRestore();frame.mockRestore();ports.allowed=true;}
});
