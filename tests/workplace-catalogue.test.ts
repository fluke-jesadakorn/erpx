import { expect, it } from "vitest";
import { moduleRegistry } from "../src/data/module-registry";
import { availableNavigationAppIds, workplaceModuleDestinations } from "../src/lib/workplace-catalogue";
import { workplaceNavigation } from "../src/lib/workplace-navigation";
import { frontWorkReturn } from "../src/lib/front-work-return";
import { workReviewCommand } from "../src/lib/work-review";

it("gives every catalogue module an objective and discovery home without opening deferred features",()=>{
  expect(Object.keys(workplaceModuleDestinations).sort()).toEqual(moduleRegistry.map(module=>module.id).sort());
  const available=availableNavigationAppIds([{id:"organization",state:"Available"},{id:"planning",state:"Coming soon"},{id:"crm",state:"Pilot",permitted:false},{id:"finance",state:"Setup required"}]);
  const model=workplaceNavigation({workspaceBase:"/holdings/acme",role:"holding_owner",scopeKind:"holding",moduleIds:["organization","planning","crm","finance"],availableAppIds:available});
  expect(model.categories.some(category=>category.id==="production")).toBe(false);
  expect(model.search.some(item=>item.id==="crm-pipeline")).toBe(false);
  expect(model.categories.some(category=>category.id==="finance")).toBe(true);
  expect(model.categories.every(category=>category.apps.length>0)).toBe(true);
});

it("retains same-holding Front location and source queries while rejecting foreign return targets",()=>{
  const target="/front/acme/company/pms/property?record=reservation&folio=folio&queue=arrivals#work";
  expect(frontWorkReturn(target,"acme")).toBe(target);
  expect(frontWorkReturn(target,"acme","company")).toBe(target);
  expect(frontWorkReturn(target,"acme","other-company")).toBeNull();
  for(const value of ["https://evil.invalid/front/acme/company/pms/property","//evil.invalid/front/acme/company/pms/property","/front/other/company/pms/property","/holdings/acme/home","/front/acme/company/pos/%2fother","/front/acme/company/pms/%5cother","/front/acme/company/pms/%ZZ"])
    expect(frontWorkReturn(value,"acme")).toBeNull();
});

it("constructs only owner-supported decision commands and carries the read version",()=>{
  expect(workReviewCommand("leave","a/b",7,"reject","Evidence checked")).toEqual({path:"/leave-requests/a%2Fb/transition",body:{action:"reject",expectedVersion:7,note:"Evidence checked",override:false}});
  expect(workReviewCommand("staff_task","task",8,"approve","Checked").body).toMatchObject({expectedVersion:8,action:"approve"});
  expect(workReviewCommand("equipment_request","request",4,"return","Details missing").path).toBe("/requests/request/actions");
  expect(()=>workReviewCommand("expense","expense",2,"return","Fix this")).toThrow();
});
