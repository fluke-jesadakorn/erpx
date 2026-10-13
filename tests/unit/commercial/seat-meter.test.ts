import { expect, it } from "vitest";
import { monthBounds } from "../../../src/lib/billing/billing";
import { canonicalSeatIntervals, deriveHoldingSeatIntervals } from "../../../src/server/modules/commercial/public/seat-meter";

const month="2026-10",{start,end}=monthBounds(month),day=86400000;
const role={event_id:1,company_id:"*",role_id:"employee",captured_at:start-1,present:true,
  state_json:JSON.stringify({status:"active",publishedRevision:1})};
const assignment=(id:string,userId:string,companyId:string,from:number,until:number,event=1)=>({
  event_id:event,assignment_id:id,captured_at:start-1,present:true,
  state_json:JSON.stringify({userId,companyId,role:"employee",source:"manual",parentId:null,
    validFrom:from,validUntil:until,revokedAt:null}),
});
const base=(assignments:ReturnType<typeof assignment>[],members: {event_id:number;company_id:string;user_id:string;captured_at:number;present:boolean;state_json:string|null}[]=[])=>({
  month,historyStartedAt:start-1,assignments,members,roles:[role],hasCutoffDependency:false,
});

it("unions overlapping Company and Branch grants into one Holding seat",()=>{
  const result=deriveHoldingSeatIntervals(base([
    assignment("one","human-1","company-a",start,start+10*day),
    assignment("two","human-1","company-b",start+5*day,start+20*day),
    assignment("three","human-1","company-b",start+19*day,end+day),
  ]));
  expect(result).toEqual({coverage:"measured",users:[{userId:"human-1",intervals:[[start,end]]}]});
});

it("ends a seat at an observed mid-month revocation and suspension",()=>{
  const revoke={...assignment("one","human-1","company-a",start,end,2),captured_at:start+12*day,
    state_json:JSON.stringify({userId:"human-1",companyId:"company-a",role:"employee",source:"manual",parentId:null,
      validFrom:start,validUntil:end,revokedAt:start+12*day})};
  const member={event_id:3,company_id:"company-a",user_id:"human-2",captured_at:start+16*day,present:true,
    state_json:JSON.stringify({status:"suspended",loginPolicy:"local"})};
  const result=deriveHoldingSeatIntervals(base([
    assignment("one","human-1","company-a",start,end),revoke,
    assignment("two","human-2","company-a",start,end),
  ],[member]));
  expect(result.users).toEqual([
    {userId:"human-1",intervals:[[start,start+12*day]]},
    {userId:"human-2",intervals:[[start,start+16*day]]},
  ]);
});

it("clips at Bangkok month boundaries and excludes service identities",()=>{
  const result=deriveHoldingSeatIntervals(base([
    assignment("human","human-1","company-a",start-10*day,end+10*day),
    assignment("bot","system:jobs","company-a",start,end),
  ]));
  expect(result.users).toEqual([{userId:"human-1",intervals:[[start,end]]}]);
  expect(canonicalSeatIntervals([[start,start+day],[start+day,start+2*day]])).toEqual([[start,start+2*day]]);
});

it("keeps a seat when a company role changes but an active global role still grants access",()=>{
  const result=deriveHoldingSeatIntervals({...base([
    assignment("one","human-1","company-a",start,end),
  ]),roles:[role,
    {event_id:2,company_id:"company-a",role_id:"employee",captured_at:start-1,present:true,
      state_json:JSON.stringify({status:"active",publishedRevision:1})},
    {event_id:3,company_id:"company-a",role_id:"employee",captured_at:start+12*day,present:true,
      state_json:JSON.stringify({status:"archived",publishedRevision:1})},
  ]});
  expect(result.users).toEqual([{userId:"human-1",intervals:[[start,end]]}]);
});

it("fails closed before source history and for delegated or dynamic grants",()=>{
  expect(deriveHoldingSeatIntervals({...base([]),historyStartedAt:start+1}).coverage).toBe("unavailable");
  const dynamic={...assignment("one","human-1","company-a",start,end),
    state_json:JSON.stringify({userId:"human-1",companyId:"company-a",role:"employee",source:"directory",parentId:null,
      validFrom:start,validUntil:end,revokedAt:null})};
  expect(deriveHoldingSeatIntervals(base([dynamic])).coverage).toBe("unavailable");
  expect(deriveHoldingSeatIntervals({...base([]),hasCutoffDependency:true}).coverage).toBe("unavailable");
});
