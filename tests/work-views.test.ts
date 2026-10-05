import { expect, it } from "vitest";
import { workInView, workItemHref, type WorkItem } from "../src/lib/work-item";
import {
 navigationAccordionStorageKey,
 navigationLauncher,
 navigationRailStorageKey,
} from "../src/lib/navigation-catalog";
import { workplaceNavigation } from "../src/lib/workplace-navigation";
const task = (id: string, data: Partial<WorkItem> = {}): WorkItem => ({ id, type:"people_lifecycle", title:id, detail:"", href:"/people?case=a", status:"pending", updatedAt:1, queue:"todo", ...data });
it("shows routing attention only to the source-authorized workflow manager", () => {
 const step = {key:'draft',action:'Prepare',role:'requester',teamName:'Requester',primary:null,candidates:[],status:'current' as const,blocker:'Former primary is no longer eligible'};
 const flow = {kind:'expense' as const,recordId:'a',recordVersion:1,version:1,defaultsRevision:0,state:'draft',steps:[step],current:step,next:null,canAct:false,canManage:true,blocker:step.blocker,history:[]};
 const rows = [task('manager',{queue:'waiting',flow}), task('requester',{queue:'waiting',flow:{...flow,canManage:false}}), task('complete',{queue:'completed',flow:{...flow,current:null}}), task('ready',{flow:{...flow,blocker:null}})];
 expect(workInView(rows,'routing').map(item=>item.id)).toEqual(['manager']);
 expect(workInView(rows,'actions').some(item=>item.id==='manager')).toBe(false);
});
it("separates assigned actionable preparation from unassigned setup without changing source queues", () => {
 const rows = [task("it", {canAct:true,relationships:["assigned"]}), task("setup", {canAct:false,category:"setup",relationships:["team"]}), task("waiting",{queue:"waiting",relationships:["requested"]})];
 expect(workInView(rows,"actions").map(i=>i.id)).toEqual(["it"]);
 expect(workInView(rows,"team").map(i=>i.id)).toEqual(["setup"]);
 expect(workInView(rows,"waiting").map(i=>i.id)).toEqual(["waiting"]);
 expect(workInView(rows,"requests").map(i=>i.id)).toEqual(["waiting"]);
});
it("keeps domain queries and return context together", () => {
 const url = new URL(workItemHref("/holdings/acme",task("a"),"view=team&type=people_lifecycle"),"https://test.invalid");
 expect(url.searchParams.get("case")).toBe("a");
 expect(url.searchParams.get("return")).toBe("view=team&type=people_lifecycle");
});
it("shows only permission-filtered app cards before launcher search", () => {
 const model = workplaceNavigation({workspaceBase:"/holdings/acme",role:"holding_owner",scopeKind:"holding"});
 const launcher = navigationLauncher(model,"");
 expect(launcher.results).toEqual([]);
 expect(launcher.categories.map(category => category.id)).toEqual(["people-organization","customers-sales","purchasing","inventory-quality","projects-routines","services","finance","pos","pms","knowledge","insights","administration"]);
 const destinations=launcher.categories.flatMap(category=>category.apps);
 expect(destinations.map(item=>item.id)).toEqual(expect.arrayContaining(["people-directory","customers-hub","sales-hub","operations-purchasing","quality-holds","pilot-features"]));
 expect(destinations.every(item=>!item.submodules?.length)).toBe(true);

});
it("searches deep destinations with owning-app context and no duplicate URLs", () => {
 const model = workplaceNavigation({workspaceBase:"/holdings/acme",role:"holding_owner",scopeKind:"holding"});
 const suppliers = navigationLauncher(model,"suppliers").results;
 expect(suppliers).toContainEqual(expect.objectContaining({item:expect.objectContaining({id:"procurement-suppliers",href:"/holdings/acme/commerce?view=parties&kind=supplier"}),context:"Purchasing",categoryId:"purchasing"}));
 const bank = navigationLauncher(model,"bank reconciliation").results;
 expect(bank).toEqual([expect.objectContaining({item:expect.objectContaining({id:"finance-bank"}),context:"Finance",categoryId:"finance"})]);
 const urls = navigationLauncher(model,"finance").results.map(result => result.item.href);
 expect(new Set(urls).size).toBe(urls.length);
});
it("omits inaccessible launcher categories and scopes rail state by workspace", () => {
 const model = workplaceNavigation({workspaceBase:"/holdings/acme",role:"employee",permissions:[]});
 expect(model.categories.map(category => category.id)).toEqual(["people-organization","projects-routines","services","knowledge"]);
 expect(navigationLauncher(model,"bank").results).toEqual([]);
 expect(navigationAccordionStorageKey("/holdings/acme")).not.toBe(navigationAccordionStorageKey("/holdings/acme/branches/main"));
 expect(navigationRailStorageKey("/holdings/acme")).not.toBe(navigationRailStorageKey("/holdings/acme/branches/main"));
});
