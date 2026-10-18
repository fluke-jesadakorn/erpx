import { beforeEach,afterEach,describe, expect, it,vi } from "vitest";
import { boundarySegment, calculateLeaveDays, dateRange } from "@/lib/people_payroll/leave-self-service";

describe("leave working-day calculation", () => {
  it("uses the policy workweek and excludes holidays across a year boundary", () => {
    const days = calculateLeaveDays({
      startDate: "2026-12-30", endDate: "2027-01-04", startSegment: "full", endSegment: "full",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(["2027-01-01"]), rosterDates: new Set(), rosterPublishedDates: new Set(),
    });
    expect(days.map((item) => item.date)).toEqual(["2026-12-30", "2026-12-31", "2027-01-04"]);
    expect(days.reduce((sum, item) => sum + item.units, 0)).toBe(3);
  });

  it("lets a published roster override the fallback workweek", () => {
    const days = calculateLeaveDays({
      startDate: "2026-09-19", endDate: "2026-09-21", startSegment: "full", endSegment: "full",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(),
      rosterPublishedDates: new Set(["2026-09-19", "2026-09-20", "2026-09-21"]),
      rosterDates: new Set(["2026-09-20"]),
    });
    expect(days).toEqual([{ date: "2026-09-20", segment: "full", units: 1, source: "roster" }]);
  });

  it("calculates half-day boundaries without changing intermediate days", () => {
    const days = calculateLeaveDays({
      startDate: "2026-09-14", endDate: "2026-09-16", startSegment: "second_half", endSegment: "first_half",
      workweek: [1, 2, 3, 4, 5], holidays: new Set(), rosterDates: new Set(), rosterPublishedDates: new Set(),
    });
    expect(days.map((item) => item.units)).toEqual([0.5, 1, 0.5]);
    expect(days.reduce((sum, item) => sum + item.units, 0)).toBe(2);
  });

  it("rejects contradictory halves for a single date", () => {
    expect(() => boundarySegment("2026-09-17", "2026-09-17", "2026-09-17", "first_half", "second_half")).toThrow("INVALID_SINGLE_DAY_SEGMENTS");
  });

  it("caps an accidental oversized range", () => {
    expect(() => dateRange("2026-01-01", "2028-01-01")).toThrow("DATE_RANGE_TOO_LONG");
  });
});

const {workFetch}=vi.hoisted(()=>({workFetch:vi.fn()}));
vi.mock('../../../src/lib/work_projection/work-fetch',()=>({workFetch}));
import {createWorkforceCommandClient,validWorkforceReceipt,type WorkforceCommand} from '../../../src/lib/people_payroll/workforce-command-client';
import {workforceActionAllowed,type WorkforcePurchase} from '../../../src/lib/people_payroll/workforce-controls';
describe('Workforce current purpose and original command recovery',()=>{
 let storage:Map<string,string>;
 beforeEach(()=>{storage=new Map();vi.stubGlobal('sessionStorage',{getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)});workFetch.mockReset();});
 afterEach(()=>vi.unstubAllGlobals());
 it('separates new Workforce drafts from existing actuals/reviews and keeps legacy self-service free of full People or Payroll module assumptions',()=>{
  const draining:WorkforcePurchase={newWork:false,settleExisting:true,history:true,revision:'actual-draining'},inactive={...draining,settleExisting:false};for(const action of ['create','save','submit','resubmit'])expect(workforceActionAllowed(draining,action)).toBe(false);for(const action of ['actuals','authorize','verify','approve','return','reject','cancel','approve_cancellation','reject_cancellation','withdraw'])expect(workforceActionAllowed(draining,action)).toBe(true);for(const action of ['create','actuals','approve']){expect(workforceActionAllowed(inactive,action)).toBe(false);expect(workforceActionAllowed(undefined,action)).toBe(true);}
 });
 it.each(['time','leave','overtime'] as const)('recovers the original %s creation across refresh without replacing its body/key after later draft edits',async kind=>{
  const client=createWorkforceCommandClient('/holdings/test','rendered-original-context'),request:WorkforceCommand={kind,action:'create',input:{reason:'Original submitted work'}};workFetch.mockRejectedValueOnce(new TypeError('lost committed response'));await expect(client.send(request)).rejects.toThrow('lost');const original=workFetch.mock.calls[0],commandKey=original[1].headers['Idempotency-Key'];expect(original[3]).toBe('caller');await expect(client.send({...request,input:{reason:'Later unsent work'}})).rejects.toThrow('uncertain result');expect(workFetch).toHaveBeenCalledTimes(1);expect(createWorkforceCommandClient('/holdings/test','different-account-or-role').pending()).toBeNull();const refreshed=createWorkforceCommandClient('/holdings/test','rendered-original-context'),id=`${kind==='overtime'?'ot':kind}-${commandKey}`;workFetch.mockResolvedValueOnce(Response.json({data:{id,version:1,...(kind==='time'?{status:'submitted'}:{state:'draft'})}}));expect(await refreshed.recover()).toMatchObject({request,receipt:{id,version:1}});expect(workFetch.mock.calls[1]).toEqual(original);expect(refreshed.pending()).toBeNull();
 });
 it.each([{id:'other-record',version:8,state:'approved'},{id:'original-record',version:7,state:'approved'},{id:'original-record',version:8}])('retains a leave decision whose acknowledgement does not match its original source/version/state (%j)',async data=>{
  const client=createWorkforceCommandClient('/holdings/test','rendered-original-context'),request:WorkforceCommand={kind:'leave',action:'approve',id:'original-record',input:{expectedVersion:7,note:'Reviewed actual request'}};workFetch.mockResolvedValueOnce(Response.json({data}));await expect(client.send(request)).rejects.toThrow('original Workforce record');expect(client.pending()).toEqual({kind:'leave',action:'approve',id:'original-record'});workFetch.mockResolvedValueOnce(Response.json({data:{id:'original-record',version:8,state:'approved'}}));await client.recover();expect(workFetch.mock.calls[1][1]).toEqual(workFetch.mock.calls[0][1]);expect(client.pending()).toBeNull();
 });
 it('keeps original work after an unreadable success and a later current-authority denial, publishing only the original form revision after a parsed receipt',async()=>{
  let revision='submitted-edit';const form={getAttribute:()=>revision},dispatchEvent=vi.fn();vi.stubGlobal('document',{activeElement:{closest:()=>form}});vi.stubGlobal('window',{dispatchEvent});const client=createWorkforceCommandClient('/holdings/test','rendered-original-context'),request:WorkforceCommand={kind:'overtime',action:'actuals',id:'actual-ot',input:{expectedVersion:7,note:'Original actuals'}};workFetch.mockResolvedValueOnce(new Response('truncated',{status:200}));await expect(client.send(request)).rejects.toThrow('could not be read');expect(dispatchEvent).not.toHaveBeenCalled();revision='later-unsent-edit';workFetch.mockResolvedValueOnce(Response.json({error:{message:'Current assignment revoked'}},{status:403}));await expect(client.recover()).rejects.toThrow('Current assignment revoked');expect(client.pending()).toBeTruthy();workFetch.mockResolvedValueOnce(Response.json({data:{id:'actual-ot',version:8,state:'pending_verification'}}));await client.recover();const event=dispatchEvent.mock.calls.map(([event])=>event as CustomEvent).find(event=>event.type==='erpx:record-command-saved');expect(event?.detail).toEqual({form,formEditVersion:'submitted-edit',workforceAcknowledged:true});expect(revision).toBe('later-unsent-edit');
 });
 it('retains the exact original action when the context transport masks a committed response after an account or role change',async()=>{
  const client=createWorkforceCommandClient('/holdings/test','original-rendered-context'),request:WorkforceCommand={kind:'time',action:'approve',id:'time-original',input:{expectedVersion:7}};workFetch.mockResolvedValueOnce(Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Your work role changed'}},{status:409}));await expect(client.send(request)).rejects.toThrow('role changed');expect(client.pending()).toEqual({kind:'time',action:'approve',id:'time-original'});workFetch.mockResolvedValueOnce(Response.json({data:{id:'time-original',version:8,status:'approved'}}));await client.recover();expect(workFetch.mock.calls[1][1]).toEqual(workFetch.mock.calls[0][1]);
 });
 it('requires the actual owner action state and cancellation receipt, not just an arbitrary versioned success',()=>{
  const time:WorkforceCommand={kind:'time',action:'approve',id:'time',input:{expectedVersion:7}},leave:WorkforceCommand={kind:'leave',action:'submit',id:'leave',input:{expectedVersion:7,saveOnly:false}},ot:WorkforceCommand={kind:'overtime',action:'actuals',id:'ot',input:{expectedVersion:7}};
  expect(validWorkforceReceipt(time,'key',{id:'time',version:8,status:'rejected'})).toBe(false);expect(validWorkforceReceipt(leave,'key',{id:'leave',version:8,state:'draft'})).toBe(false);expect(validWorkforceReceipt(ot,'key',{id:'ot',version:8,state:'approved'})).toBe(false);
  const cancel:WorkforceCommand={kind:'leave',action:'cancel',id:'leave',input:{expectedVersion:7}};expect(validWorkforceReceipt(cancel,'key',{id:'leave',version:8,state:'approved'})).toBe(false);expect(validWorkforceReceipt(cancel,'key',{id:'leave',version:8,state:'approved',cancellationState:'pending_review'})).toBe(true);
 });
 it('blocks concurrent submits and permits a corrected draft after a definitive initial validation rejection',async()=>{
  let finish!:(response:Response)=>void;const client=createWorkforceCommandClient('/holdings/test','rendered-original-context'),request:WorkforceCommand={kind:'time',action:'save',id:'time-original',input:{expectedVersion:7,regularHours:25}};workFetch.mockReturnValueOnce(new Promise<Response>(resolve=>{finish=resolve;}));const pending=client.send(request);await expect(client.send(request)).rejects.toThrow('Wait for the current');finish(Response.json({error:{message:'Maximum 24 hours'}},{status:422}));await expect(pending).rejects.toThrow('Maximum 24 hours');expect(client.pending()).toBeNull();workFetch.mockResolvedValueOnce(Response.json({data:{id:'time-original',version:8,status:'submitted'}}));await client.send({...request,input:{expectedVersion:7,regularHours:8}});expect(workFetch.mock.calls[1][1].headers['Idempotency-Key']).not.toBe(workFetch.mock.calls[0][1].headers['Idempotency-Key']);
 });
});

import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {LeaveEditor,LeaveDecisions} from '../../../src/components/people_payroll/self-service/leave-editor-content';
import {OvertimeForm} from '../../../src/components/people_payroll/self-service/overtime';
import {TimeEntryEditor} from '../../../src/components/people_payroll/self-service/time-entry-editor';
import {LeaveControls} from '../../../src/components/people_payroll/leave-controls';
import type {getLeave} from '../../../src/server/modules/people_payroll/public/leave';
import type {OvertimeDetail} from '../../../src/server/modules/people_payroll/public/overtime';
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>'/holdings/test/leave',useSearchParams:()=>new URLSearchParams()}));
const activeWorkforce:WorkforcePurchase={newWork:true,settleExisting:true,history:true,revision:'minimal-workforce'};
const drainingWorkforce:WorkforcePurchase={...activeWorkforce,newWork:false};
const historyWorkforce:WorkforcePurchase={...drainingWorkforce,settleExisting:false};
function buttonMarkup(html:string,label:string){const match=html.match(new RegExp('<button[^>]*>'+label+'</button>'))?.[0];expect(match,`Actual button: ${label}`).toBeTruthy();return match!;}
const baseProps={base:'/holdings/test',workContext:'opaque-rendered-account-role'};
describe('Workforce actual self-service presentation',()=>{
 beforeEach(()=>{workFetch.mockReset();});
 it('renders the Leave draft from its server snapshot, disables new work while retaining values and does not make a mount request during server rendering',()=>{
  const html=(purchase:WorkforcePurchase|undefined)=>renderToStaticMarkup(createElement(LeaveEditor,{...baseProps,purchase,types:[],initialDate:'2026-10-16'}));
  const active=html(activeWorkforce),draining=html(drainingWorkforce),legacy=html(undefined);expect(active).toContain('2026-10-16');expect(draining).toContain('2026-10-16');expect(active.match(/<input[^>]*type="date"[^>]*>/)?.[0]).not.toContain('disabled=""');expect(draining.match(/<input[^>]*type="date"[^>]*>/)?.[0]).toContain('disabled=""');expect(legacy.match(/<input[^>]*type="date"[^>]*>/)?.[0]).not.toContain('disabled=""');expect(draining).not.toContain('company_package_grants');expect(draining).not.toContain('principalId');expect(workFetch).not.toHaveBeenCalled();
 });
 it('keeps original Leave role actions, allows an existing draining review and disables history-only review',()=>{
  const record={id:'leave',version:7,state:'pending_approval',permittedActions:['approve','reject']} as Awaited<ReturnType<typeof getLeave>>;
  const html=(purchase:WorkforcePurchase)=>renderToStaticMarkup(createElement(LeaveDecisions,{...baseProps,record,purchase}));
  expect(buttonMarkup(html(drainingWorkforce),'Approve')).not.toContain('disabled=""');expect(buttonMarkup(html(historyWorkforce),'Approve')).toContain('disabled=""');expect(renderToStaticMarkup(createElement(LeaveDecisions,{...baseProps,record:{...record,permittedActions:[]},purchase:activeWorkforce}))).toBe('');
 });
 it('keeps original Overtime authorization independent of new drafts and preserves the employee details while draining',()=>{
  const record:OvertimeDetail={id:'ot',company_id:'company',engagement_id:'engagement',person_id:'person',branch_id:null,department_id:null,created_by:'employee',version:7,state:'pending_authorization',mode:'advance',timezone:'Asia/Bangkok',start_local:'2026-10-16T18:00',end_local:'2026-10-16T20:00',starts_at:Date.UTC(2026,9,16,11),ends_at:Date.UTC(2026,9,16,13),break_minutes:0,minutes:120,reason:'Original worked assignment',late_explanation:'',roster_shift_id:null,planned_snapshot:null,actuals_snapshot:null,decision_note:'',created_at:1,updated_at:1,permittedActions:['authorize','reject'],details:{mode:'advance',startLocal:'2026-10-16T18:00',endLocal:'2026-10-16T20:00',timezone:'Asia/Bangkok',breakMinutes:0,reason:'Original worked assignment',lateExplanation:'',rosterShiftId:null},approval:{currentApprover:null,blockedReason:null,history:[],steps:[],instance:null,canDecide:true,canOverride:false},handoff:null};
  const html=(purchase:WorkforcePurchase)=>renderToStaticMarkup(createElement(OvertimeForm,{...baseProps,record,purchase}));
  const draining=html(drainingWorkforce),history=html(historyWorkforce);expect(draining).toContain('Original worked assignment');expect(buttonMarkup(draining,'Authorize planned work')).not.toContain('disabled=""');expect(buttonMarkup(history,'Authorize planned work')).toContain('disabled=""');
 });
 it('shows independent Time review without Payroll controls, disables history-only writes and keeps requester self-approval unavailable',()=>{
  const entry={id:'time',version:7,work_date:'2026-10-16',regular_hours:8,overtime_hours:0,status:'submitted',submitted_by:'employee'},props={...baseProps,person:{engagement_id:'engagement',user_id:'employee'},entries:[entry],canEdit:false,canApprove:true,userId:'independent-manager'};
  const draining=renderToStaticMarkup(createElement(TimeEntryEditor,{...props,purchase:drainingWorkforce}));expect(buttonMarkup(draining,'Approve time')).not.toContain('disabled=""');expect(buttonMarkup(renderToStaticMarkup(createElement(TimeEntryEditor,{...props,purchase:historyWorkforce})),'Approve time')).toContain('disabled=""');expect(renderToStaticMarkup(createElement(TimeEntryEditor,{...props,purchase:activeWorkforce,userId:'employee'}))).not.toContain('Approve time');expect(draining).not.toContain('Payroll run');expect(draining).toContain('Time entries');
 });
 it('retains the original legacy Leave action branch and its full-page draft entry with current Workforce controls',()=>{
  const decisions=renderToStaticMarkup(createElement(LeaveControls,{...baseProps,purchase:drainingWorkforce,recordId:'leave',version:7,canReject:true,canCancel:true}));expect(decisions).toContain('Reject request');expect(decisions).toContain('Cancel request');expect(decisions).not.toContain('Requested working days');const draft=renderToStaticMarkup(createElement(LeaveControls,{...baseProps,purchase:historyWorkforce,initialOpen:true}));expect(draft).toContain('Requested working days');expect(buttonMarkup(draft,'Save leave draft')).toContain('disabled=""');
 });
});

import {createLeaveEvidenceClient} from '../../../src/lib/people_payroll/workforce-evidence-client';
describe('Leave private evidence original upload acknowledgement',()=>{
 beforeEach(()=>{const storage=new Map<string,string>();vi.stubGlobal('sessionStorage',{getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)});workFetch.mockReset();});
 afterEach(()=>vi.unstubAllGlobals());
 const reservation={uploadIntentId:'original-intent',attachmentLinkId:'original-link',attachmentSetId:'original-set',uploadUrl:'/api/v1/holdings/test/uploads/original-intent/content'},completion={uploadIntentId:'original-intent',attachmentLinkId:'original-link',attachmentSetId:'original-set',id:'attachment-original-intent',sourceId:'attachment-original-intent',state:'available'};
 it('recovers the exact private bytes upload after refresh, refuses changed bytes with identical file metadata and never stores bytes',async()=>{
  const original=new File(['original-private-content'],'proof.pdf',{type:'application/pdf'}),changed=new File(['different-private-bytes!'],'proof.pdf',{type:'application/pdf'}),client=createLeaveEvidenceClient('/holdings/test','original-role');workFetch.mockResolvedValueOnce(Response.json({data:reservation}));workFetch.mockRejectedValueOnce(Error('lost upload response'));await expect(client.upload('leave',original,'medical_certificate')).rejects.toThrow('lost upload');const raw=sessionStorage.getItem('erpx:workforce-evidence:original-role')!;expect(raw).not.toContain('original-private-content');expect(JSON.parse(raw).digest).toMatch(/^[a-f0-9]{64}$/);const put=workFetch.mock.calls[1];expect(put[3]).toBe('caller');const refreshed=createLeaveEvidenceClient('/holdings/test','original-role');expect(refreshed.pending()).toBe(true);await expect(refreshed.upload('leave',changed,'medical_certificate')).rejects.toThrow('same file');expect(workFetch).toHaveBeenCalledTimes(2);workFetch.mockResolvedValueOnce(Response.json({data:completion}));expect(await refreshed.upload('leave',original,'medical_certificate')).toEqual({sourceId:completion.sourceId});expect(workFetch.mock.calls[2]).toEqual(put);expect(refreshed.pending()).toBe(false);
 });
 it('does not send private bytes to an unrelated reservation URL and retries the original metadata key',async()=>{
  const file=new File(['private'],'proof.pdf',{type:'application/pdf'}),client=createLeaveEvidenceClient('/holdings/test','original-role');workFetch.mockResolvedValueOnce(Response.json({data:{...reservation,uploadUrl:'https://foreign.invalid/upload'}}));await expect(client.upload('leave',file,'medical_certificate')).rejects.toThrow('reservation was not acknowledged');expect(workFetch).toHaveBeenCalledTimes(1);workFetch.mockResolvedValueOnce(Response.json({data:reservation}));workFetch.mockResolvedValueOnce(Response.json({data:completion}));await client.upload('leave',file,'medical_certificate');expect(workFetch.mock.calls[1][1]).toEqual(workFetch.mock.calls[0][1]);
 });
 it.each([{...completion,state:'quarantined'},{...completion,sourceId:'another-private-source'},null])('keeps the original upload pending when clean private evidence was not acknowledged (%j)',async data=>{
  const file=new File(['private'],'proof.pdf',{type:'application/pdf'}),client=createLeaveEvidenceClient('/holdings/test','original-role');workFetch.mockResolvedValueOnce(Response.json({data:reservation}));workFetch.mockResolvedValueOnce(Response.json({data}));await expect(client.upload('leave',file,'medical_certificate')).rejects.toThrow('Clean evidence was not acknowledged');expect(client.pending()).toBe(true);
 });
});

import {workforcePresentationContext} from '../../../src/server/modules/people_payroll/public/workforce-presentation';
import type {WorkContext} from '../../../src/server/modules/identity_access/public/authorization';
it('scopes the opaque Workforce draft identity to the original employee account, Company, role, assignment and current authority revision without requiring management modules',()=>{
 const actor:WorkContext={holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',companyId:'company',branchId:null,departmentId:null,userId:'employee',name:'Employee',email:'employee@example.test',assignmentId:'assignment',role:'employee',scope:{kind:'holding',holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',companyId:'company'},assignmentScope:'holding',inherited:false,assignments:[],workspaces:[],modules:[],authority:{sessionId:'private-session-id',assignmentVersion:1,revision:1}},scope=workforcePresentationContext(actor);
 expect(scope).toMatch(/^[a-f0-9]{64}$/);expect(scope).not.toContain('private-session-id');expect(workforcePresentationContext({...actor,modules:['people_payroll']})).toBe(scope);
 for(const change of [{companyId:'other-company'},{userId:'other-employee'},{role:'people_manager'},{assignmentId:'other-assignment'},{authority:{...actor.authority!,revision:2}}])expect(workforcePresentationContext({...actor,...change})).not.toBe(scope);
});


const pageReads=vi.hoisted(()=>({summary:vi.fn(),month:vi.fn(),list:vi.fn(),calendar:vi.fn(),resources:vi.fn()}));
vi.mock('../../../src/server/modules/people_payroll/adapters/leave-self-service',async()=>({leaveListFilterSchema:(await import('../../../src/server/modules/people_payroll/domain/leave-self-service')).leaveListFilterSchema,leaveSummary:pageReads.summary,leaveCalendarMonth:pageReads.month,listOwnLeavePage:pageReads.list}));
vi.mock('../../../src/server/modules/work_projection/public/calendar/index',()=>({calendarData:pageReads.calendar}));
vi.mock('../../../src/server/modules/runtime/public/runtime',()=>({holdingResources:pageReads.resources}));
import {leavePageSnapshot} from '../../../src/server/modules/people_payroll/public/leave-page-snapshot';
import {LeaveSelfService} from '../../../src/components/people_payroll/leave/leave-self-service';
import type {CalendarItem} from '../../../src/lib/work_projection/calendar';
const ownPageRow={id:'original-leave',number:'LV-1',start_date:'2026-10-16',end_date:'2026-10-17',start_segment:'full',end_segment:'full',calculated_units:2,days:2,state:'approved',cancellation_state:null,version:7,created_at:1,updated_at:2,decision_note:'PRIVATE_DECISION_NOTE',leave_type_name:'Annual leave'};
const teamPageRow:CalendarItem={id:'approved-busy',source:'leave',sourceId:'coworker-leave',kind:'leave',title:'Team member away',description:'PRIVATE_MEDICAL_DESCRIPTION',allDay:true,startDate:'2026-10-16',endDate:'2026-10-18',startsAt:null,endsAt:null,timezone:'Asia/Bangkok',departmentId:'private-department',branchId:'private-branch',participants:[{id:'private-user',name:'PRIVATE_PERSON'}],owner:'private-owner',status:'approved',version:3,permittedActions:['approve'],warnings:[],history:[{action:'PRIVATE_REVIEW',actor:'private-reviewer',at:1}],redacted:true,details:{private:true,location:'PRIVATE_LOCATION'}};
const snapshotActor:WorkContext={holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',companyId:'company',branchId:null,departmentId:null,userId:'employee',name:'Employee',email:'employee@example.test',assignmentId:'assignment',role:'employee',scope:{kind:'holding',holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',companyId:'company'},assignmentScope:'holding',inherited:false,assignments:[],workspaces:[],modules:[]};
describe('Leave initial server content and private view projection',()=>{
 beforeEach(()=>{for(const read of Object.values(pageReads))read.mockReset();pageReads.resources.mockResolvedValue({write:'original-db'});pageReads.summary.mockResolvedValue({asOf:1,setupRequired:false,setupMessage:null,policy:{id:'private-policy',name:'Annual policy',version:1,timezone:'Asia/Bangkok'},balances:[{type:{id:'annual',name:'Annual leave',tracked:true,evidence_note:'PRIVATE_POLICY_DETAILS'},available:10,reserved:2,used:3,accrued:15}],upcoming:[],pending:[],history:[{decision_note:'PRIVATE_SUMMARY_HISTORY'}],holidays:[],purchase:drainingWorkforce});pageReads.month.mockResolvedValue({month:'2026-10',requests:[ownPageRow],holidays:[{holiday_date:'2026-10-23',name:'Assigned holiday'}]});pageReads.list.mockResolvedValue({items:[ownPageRow],hasMore:true,nextCursor:'original-cursor'});pageReads.calendar.mockResolvedValue({items:[teamPageRow,{...teamPageRow,id:'not-redacted',redacted:false},{...teamPageRow,id:'pending',status:'pending_approval'}],sources:[{source:'leave',status:'ready'}],nextCursor:null});workFetch.mockReset();});
 it('renders authorized own requests, holidays and redacted team dates in initial HTML, without private reader snapshots or mount loading placeholders',async()=>{
  const snapshot=await leavePageSnapshot(snapshotActor,{month:'2026-10',filter:'approved'}),html=renderToStaticMarkup(createElement(LeaveSelfService,{base:'/holdings/test',initial:snapshot.summary,initialMonth:snapshot.month,initialFilter:snapshot.filter,snapshot}));
  expect(html).toContain('Annual leave');expect(html).toContain('Assigned holiday');expect(html).toContain('Team member away');expect(html).toContain('Load more requests');expect(html).not.toContain('Loading this month');expect(html).not.toContain('Loading your requests');expect(html).not.toContain('Request leave');expect(workFetch).not.toHaveBeenCalled();
  expect(snapshot.team.map(item=>item.id)).toEqual(['approved-busy']);expect(JSON.stringify(snapshot)).not.toContain('PRIVATE_');expect(snapshot.summary).not.toHaveProperty('history');expect(snapshot.summary).not.toHaveProperty('asOf');expect(snapshot.requests[0]).not.toHaveProperty('decision_note');expect(snapshot.team[0]).not.toHaveProperty('details');expect(pageReads.month).toHaveBeenCalledExactlyOnceWith(snapshotActor,'2026-10');expect(pageReads.list).toHaveBeenCalledExactlyOnceWith(snapshotActor,{filter:'approved',limit:10});expect(pageReads.calendar).toHaveBeenCalledExactlyOnceWith('original-db',snapshotActor,{date:'2026-10-01',view:'month',mode:'team',layers:'leave',cursor:'0',limit:'200'});
 });
 it('reads every original Calendar page while keeping its partial-source status explicit',async()=>{
  pageReads.calendar.mockResolvedValueOnce({items:[teamPageRow],sources:[{source:'leave',status:'ready'}],nextCursor:200}).mockResolvedValueOnce({items:[{...teamPageRow,id:'second-busy'}],sources:[{source:'leave',status:'error',message:'PRIVATE_FAILURE'}],nextCursor:null});
  const snapshot=await leavePageSnapshot(snapshotActor,{month:'2026-10',filter:'all'});expect(snapshot.team.map(item=>item.id)).toEqual(['approved-busy','second-busy']);expect(snapshot.teamError).toContain('Some approved team absences');expect(snapshot.calendarError).toBe('');expect(JSON.stringify(snapshot)).not.toContain('PRIVATE_');expect(pageReads.calendar.mock.calls[1][2].cursor).toBe('200');
 });
 it('keeps independent reader failures retryable without serializing database errors or pretending the missing list is empty history',async()=>{
  pageReads.month.mockRejectedValue(Error('PRIVATE_DATABASE_SECRET'));pageReads.list.mockRejectedValue(Error('PRIVATE_QUERY'));const snapshot=await leavePageSnapshot(snapshotActor,{month:'2026-10',filter:'all'}),html=renderToStaticMarkup(createElement(LeaveSelfService,{base:'/holdings/test',initial:snapshot.summary,initialMonth:snapshot.month,initialFilter:snapshot.filter,snapshot}));expect(snapshot.calendar).toBeNull();expect(snapshot.calendarError).toContain('Retry');expect(snapshot.listError).toContain('Retry');expect(html).toContain('Your requests could not be loaded');expect(html).not.toContain('No leave requests yet');expect(JSON.stringify(snapshot)).not.toContain('PRIVATE_');
 });
});
