import { beforeEach, expect, it, vi } from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('react',()=>({cache:(fn:unknown)=>fn}));
const mocks=vi.hoisted(()=>({first:vi.fn(),platform:vi.fn(),options:vi.fn()}));
vi.mock('../src/server/authorization',()=>({firstAuthorizedWorkspace:mocks.first,requirePlatformContext:mocks.platform}));
vi.mock('../src/server/runtime',()=>({activeHoldingBindings:async()=>[{slug:'acme',displayName:'Acme'},{slug:'northstar',displayName:'Northstar'}]}));
vi.mock('../src/server/work-role-options',()=>({workRoleOptions:mocks.options}));
import { workspaceChoices } from '../src/server/workspace-choices';
import {moduleRegistry,moduleAvailability,capabilitySupport,liveDestination} from '../src/data/module-registry';
beforeEach(()=>{mocks.first.mockReset();mocks.platform.mockReset();mocks.options.mockReset().mockResolvedValue({options:[],selection:null});});
it('platform authority alone creates no business choices',async()=>{
 mocks.first.mockResolvedValue(null);mocks.platform.mockResolvedValue({});
 expect(await workspaceChoices()).toEqual([{kind:'platform',label:'Platform administration',destination:'/platform/home'}]);
});
it('uses resolved business scope and hides platform without its own grant',async()=>{
 mocks.first.mockImplementation(async(slug:string)=>slug==='acme'?{kind:'department',holdingSlug:slug,branchSlug:'bangkok',departmentSlug:'operations'}:null);
 mocks.platform.mockRejectedValue(new Error('FORBIDDEN'));
 expect(await workspaceChoices()).toEqual([{kind:'business',label:'Acme',destination:'/holdings/acme/branches/bangkok/departments/operations/home'}]);
});
it('does not mask unexpected platform authorization failures',async()=>{
 mocks.first.mockResolvedValue(null);mocks.platform.mockRejectedValue(new Error('DATABASE_UNAVAILABLE'));
 await expect(workspaceChoices()).rejects.toThrow('DATABASE_UNAVAILABLE');
});
it('all platform sections report previews with no live action destinations',()=>{
 for(const entry of moduleRegistry){
 expect(moduleAvailability(entry,'/platform')).toBe('preview');
 for(const section of entry.sections){
 expect(liveDestination(entry.id,section.id,'/platform')).toBeUndefined();
 const support=capabilitySupport(entry.id,section.id,'/platform');
 expect(support.availability).toBe('preview');
 expect(support.actions.every(action=>action.availability==='preview'&&!action.destination)).toBe(true);
 }
 }
 expect(liveDestination('finance','receivables','/holdings/acme')).toBe('finance/invoices?kind=receivable');
});

it('resumes a valid selected role in its authorized scope',async()=>{
 mocks.first.mockImplementation(async(slug:string)=>slug==='acme'?{kind:'holding',holdingSlug:slug}:null);
 mocks.platform.mockRejectedValue(new Error('FORBIDDEN'));
 mocks.options.mockResolvedValue({options:[{id:'employee',version:2,href:'/holdings/acme/branches/bangkok/home'}],selection:{assignment_id:'employee',assignment_version:2,expires_at:Date.now()+60000}});
 expect((await workspaceChoices())[0].destination).toBe('/holdings/acme/branches/bangkok/home');
 mocks.options.mockResolvedValue({options:[],selection:{assignment_id:'revoked',assignment_version:2,expires_at:Date.now()+60000}});
 expect((await workspaceChoices())[0].destination).toBe('/holdings/acme/home');
});
