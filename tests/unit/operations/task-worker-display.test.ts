import { beforeEach, expect, it, vi } from "vitest";
import { createElement, isValidElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
const ports = vi.hoisted(() => ({ presentation: vi.fn() }));
vi.mock("@/server/modules/work_projection/public/presentation-context-queries", () => ({ staffPresentationQueries: ports.presentation }));
vi.mock("@/components/workplace/flow-panel", () => ({ documentFlow: async () => ({ canAct: true }), FlowPanel: () => null }));
vi.mock('@/server/modules/runtime/public/administration',()=>({administrationAccess:async()=>({sections:[],manageOrganization:false})}));
vi.mock("next/navigation", () => ({ notFound: () => { throw Error("NOT_FOUND"); }, forbidden: () => { throw Error("FORBIDDEN"); }, redirect: (href: string) => { throw Error("REDIRECT:" + href); }, unstable_rethrow: () => {}, useRouter: () => ({ refresh: vi.fn() }), usePathname: () => "/holdings/acme/work", useSearchParams: () => new URLSearchParams() }));
import NewTask from "@/app/holdings/[holdingSlug]/_presenters/staff-tasks/new/screen";
import Task from "@/app/holdings/[holdingSlug]/_presenters/staff-tasks/[id]/screen";
import { StaffPicker } from "@/components/work_projection/staff-work-controls";
import Routines from '@/app/holdings/[holdingSlug]/_presenters/routine-templates/screen';
import Routine from '@/app/holdings/[holdingSlug]/_presenters/routine-templates/[id]/screen';

const source = {
  people: [{ user_id: "worker", legal_name: "Worker", department_id: "engineering", level_id: "engineer", level_name: "Engineer",
    person_id: "private-person", engagement_id: "private-engagement", engagement_version: 73, manager_person_id: "private-manager", worker_type: "employee", self_service_top_level: true }],
  levels: [{ id: "engineer", name: "Engineer", rank: 4, version: 19, code: "private-level-code", active: 1 }],
  departments: [{ id: "engineering", display_name: "Engineering", person_id: "private-head", version: 61, head_name: "Private head" }],
  canManage: true,
};
const expected = { people: [{ user_id: "worker", legal_name: "Worker", department_id: "engineering", level_id: "engineer", level_name: "Engineer" }],
  levels: [{ id: "engineer", name: "Engineer", rank: 4 }], departments: [{ id: "engineering", display_name: "Engineering" }] };
function optionProps(tree: ReactNode): unknown[] {
  if (Array.isArray(tree)) return tree.flatMap(optionProps);
  if (!isValidElement<{ options?: unknown; children?: ReactNode }>(tree)) return [];
  return [...("options" in tree.props ? [tree.props.options] : []), ...optionProps(tree.props.children)];
}
function projectProps(tree:ReactNode):unknown[]{if(Array.isArray(tree))return tree.flatMap(projectProps);if(!isValidElement<{projects?:unknown;children?:ReactNode}>(tree))return [];return [...('projects'in tree.props?[tree.props.projects]:[]),...projectProps(tree.props.children)];}
function taskPresentation(){return { actor: { userId: "worker", companyId: "company", assignmentId: "assignment", beta: false }, base: "/holdings/acme", options: source, projects: [], staff: { user_id: "worker" },
    queries: { getWorkCapabilities: async () => ({ creation: [{ kind: "task" }] }), taskSocialOrigin: async () => null,
      taskDetail: async () => ({ task: { id: "task", title: "Task", state: "in_progress", description: "Instructions", due_at: null, project_id: null, creator_name: "Worker", assignee_name: "Worker", revision: 1, version: 2 }, permittedActions: ["revise", "cancel", "delegate"], approval: null }) } };}
beforeEach(() => {
  ports.presentation.mockClear();
  ports.presentation.mockResolvedValue(taskPresentation());
});
it("projects private worker identities out of the actual new Task Client props while retaining picker filters", async () => {
  const page = await NewTask({ params: Promise.resolve({ holdingSlug: "acme" }), searchParams: Promise.resolve({}) });
  expect(optionProps(page)).toEqual([expected]);
  expect(ports.presentation).toHaveBeenCalledExactlyOnceWith('acme',{projectOptions:'optional'});
  expect(source.people[0].engagement_version).toBe(73);
});
it("projects the actual Task edit revision and delegation Client options without weakening server management decisions", async () => {
  const page = await Task({ params: Promise.resolve({ holdingSlug: "acme", id: "task" }), searchParams: Promise.resolve({}) });
  expect(optionProps(page)).toEqual([expected, expected]);
  expect(ports.presentation).toHaveBeenCalledExactlyOnceWith('acme',{projectOptions:'optional'});
  const encoded = JSON.stringify(optionProps(page));
  for (const field of ["private-person", "private-engagement", "private-manager", "private-head", "private-level-code", "engagement_version", "self_service_top_level", "canManage"]) expect(encoded).not.toContain(field);
});
it.each(['new','detail'] as const)('projects only actual id/title choices into the %s Task editor while keeping server Project source data private',async kind=>{
 const context=taskPresentation();ports.presentation.mockResolvedValueOnce({...context,projects:[{id:'project',title:'Actual reviewed Project',version:57,owner_user_id:'private-project-owner',description:'private-project-description',department_id:'private-project-department'}],queries:{...context.queries,projectPurchases:async()=>({'projects.wbs':{newWork:true,settleExisting:true,history:true,revision:'opaque-revision'}})}});
 const page=kind==='new'?await NewTask({params:Promise.resolve({holdingSlug:'acme'}),searchParams:Promise.resolve({projectId:'project'})}):await Task({params:Promise.resolve({holdingSlug:'acme',id:'task'}),searchParams:Promise.resolve({})});
 expect(projectProps(page)).toEqual(kind==='new'?[[{id:'project',title:'Actual reviewed Project'}]]:[[{id:'project',title:'Actual reviewed Project'}]]);expect(JSON.stringify(projectProps(page))).not.toContain('private-');expect(optionProps(page)).toEqual(kind==='new'?[expected]:[expected,expected]);
});
it("preserves actual staff department and level picker labels using display fields only", async () => {
  const page = await NewTask({ params: Promise.resolve({ holdingSlug: "acme" }), searchParams: Promise.resolve({}) });
  const options = optionProps(page)[0] as Parameters<typeof StaffPicker>[0]["options"];
  const html = renderToStaticMarkup(createElement(StaffPicker, { appearance: "standard", label: "Assignee", value: "worker", onChange: () => {}, options }));
  for (const label of ["Department filter", "Level filter", "Engineering", "Engineer · rank 4", "Worker · Engineer"]) expect(html).toContain(label);
  for (const field of ["private-person", "private-engagement", "private-manager", "private-head", "private-level-code"]) expect(html).not.toContain(field);
});
it('opens actual standalone Routine collection with explicit optional Projects and display-only worker and Project choices',async()=>{
 ports.presentation.mockResolvedValueOnce({actor:{userId:'worker',companyId:'company',assignmentId:'assignment',beta:false,scope:{kind:'holding'}},base:'/holdings/acme',options:source,projects:[{id:'project',title:'Original project option',created_by:'private-project-creator',branch_id:'private-project-branch'}],staff:{user_id:'worker'},queries:{listTemplates:async()=>[],routinePurchases:async()=>undefined}});
 const page=await Routines({params:Promise.resolve({holdingSlug:'acme'}),searchParams:Promise.resolve({})});expect(ports.presentation).toHaveBeenCalledExactlyOnceWith('acme',{routineProjects:true});expect(optionProps(page)).toEqual([expected]);expect(projectProps(page)).toEqual([[{id:'project',title:'Original project option'}]]);expect(JSON.stringify([optionProps(page),projectProps(page)])).not.toContain('private-');
});
it('retains actual Routine detail edit and clone authority while projecting every optional picker to display fields',async()=>{
 ports.presentation.mockResolvedValueOnce({actor:{userId:'worker',companyId:'company',assignmentId:'assignment',beta:false},base:'/holdings/acme',options:source,projects:[{id:'project',title:'Original project option',version:53,created_by:'private-project-creator'}],staff:{user_id:'worker'},queries:{getTemplate:async()=>({id:'routine-source',title:'Original routine',description:'Reviewed source',creator_user_id:'worker',assignee_user_id:'worker',project_id:null,published:1,version:3,start_approvers_json:'[]',completion_approvers_json:'["reviewer"]'}),routinePurchases:async()=>undefined,canEditTemplate:async()=>true,routineTemplateSchedules:async()=>({results:[]})}});
 const page=await Routine({params:Promise.resolve({holdingSlug:'acme',id:'routine-source'})});expect(ports.presentation).toHaveBeenCalledExactlyOnceWith('acme',{routineProjects:true});expect(optionProps(page)).toEqual([expected,expected]);expect(projectProps(page)).toEqual([[{id:'project',title:'Original project option'}],[{id:'project',title:'Original project option'}]]);expect(JSON.stringify([optionProps(page),projectProps(page)])).not.toContain('private-');
});


const projectTaskPorts=vi.hoisted(()=>({resources:vi.fn(),actor:vi.fn(),project:vi.fn(),purchases:vi.fn(),staff:vi.fn(),settings:vi.fn(),calls:[] as string[]}));
vi.mock('@/server/modules/runtime/public/runtime',()=>({holdingResources:projectTaskPorts.resources}));
vi.mock('@/server/modules/work_projection/public/page-error',()=>({pageError:(error:unknown)=>{throw error;}}));
vi.mock('@/server/modules/identity_access/public/authorization',()=>({requireWorkContext:projectTaskPorts.actor,publicScope:(actor:{scope:unknown})=>actor.scope}));
vi.mock('@/server/modules/projects/adapters/staff-projects',()=>({getProject:projectTaskPorts.project}));
vi.mock('@/server/modules/projects/adapters/purchases',()=>({projectPurchaseControls:projectTaskPorts.purchases}));
vi.mock('@/server/modules/operations/public/staff-policy',()=>({activeStaff:projectTaskPorts.staff}));
vi.mock('@/server/modules/operations/public/staff-settings',()=>({staffSettings:projectTaskPorts.settings}));
import NewProjectTask from '@/app/holdings/[holdingSlug]/projects-routines/projects/[id]/tasks/new/page';
import {projectTaskCreationWorkspace} from '@/server/modules/projects/public/task-creation';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
const projectTaskActor:WorkContext={holdingId:'holding',holdingSlug:'acme',holdingName:'Actual Holding',companyId:'company',branchId:null,departmentId:null,userId:'worker',name:'Worker',email:'worker@example.test',assignmentId:'assignment',role:'holding_owner',assignmentScope:'holding',inherited:false,modules:['projects'],assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'holding',holdingSlug:'acme',holdingName:'Actual Holding',companyId:'company'}};
function projectTaskSource(){
 projectTaskPorts.calls.length=0;for(const port of [projectTaskPorts.resources,projectTaskPorts.actor,projectTaskPorts.project,projectTaskPorts.purchases,projectTaskPorts.staff,projectTaskPorts.settings])port.mockReset();projectTaskPorts.actor.mockResolvedValue(projectTaskActor);projectTaskPorts.resources.mockResolvedValue({write:{native:'server-only-database'}});projectTaskPorts.project.mockImplementation(async()=>{projectTaskPorts.calls.push('project-owner');return {id:'project',title:'Actual reviewed Project',lifecycle_status:'active'};});projectTaskPorts.purchases.mockImplementation(async()=>{projectTaskPorts.calls.push('purchase-owner');return {'projects.wbs':{newWork:true,settleExisting:true,history:true,revision:'minimal-owner-revision'},'projects.tasks':{newWork:true,settleExisting:true,history:true,revision:'minimal-owner-revision'}};});projectTaskPorts.staff.mockImplementation(async()=>{projectTaskPorts.calls.push('current-worker');return source.people[0];});projectTaskPorts.settings.mockImplementation(async()=>{projectTaskPorts.calls.push('worker-display-source');return source;});
}
it('keeps native HR source rows out of actual Project-only Task creation Client props through the original workspace reader',async()=>{
 projectTaskSource();const before=structuredClone(source),page=await NewProjectTask({params:Promise.resolve({holdingSlug:'acme',id:'project'}),searchParams:Promise.resolve({dueDate:'2026-11-01'})});
 expect(projectTaskPorts.calls).toEqual(['project-owner','purchase-owner','current-worker','worker-display-source']);expect(projectTaskPorts.project).toHaveBeenLastCalledWith({native:'server-only-database'},projectTaskActor,'project');expect(optionProps(page)).toEqual([expected]);
 expect(projectProps(page)).toEqual([[{id:'project',title:'Actual reviewed Project',status:'active'}]]);expect(JSON.stringify(optionProps(page))).not.toContain('private-');expect(source).toEqual(before);expect(projectTaskActor.modules).toEqual(['projects']);
});

it('keeps the original Project owner denial ahead of every worker display source',async()=>{
 projectTaskSource();const denied=Error('Original Project authority denied');projectTaskPorts.project.mockRejectedValueOnce(denied);await expect(projectTaskCreationWorkspace(projectTaskActor,'project')).rejects.toBe(denied);expect(projectTaskPorts.purchases).not.toHaveBeenCalled();expect(projectTaskPorts.staff).not.toHaveBeenCalled();expect(projectTaskPorts.settings).not.toHaveBeenCalled();
});
it('keeps Project draining creation unavailable without querying private picker sources',async()=>{
 projectTaskSource();projectTaskPorts.purchases.mockResolvedValueOnce({'projects.wbs':{newWork:false,settleExisting:true,history:true,revision:'retained-owner-revision'},'projects.tasks':{newWork:false,settleExisting:true,history:true,revision:'retained-owner-revision'}});const workspace=await projectTaskCreationWorkspace(projectTaskActor,'project');expect(workspace.canCreate).toBe(false);expect(workspace.options).toEqual({people:[],levels:[],departments:[]});expect(projectTaskPorts.settings).not.toHaveBeenCalled();expect(projectTaskPorts.staff).toHaveBeenCalledExactlyOnceWith({native:'server-only-database'},'company','worker');
});


import type { ComponentProps, ReactElement } from "react";
import { TaskEditor as DetailTaskEditor } from "@/components/records/loaders/modules--staff-work-controls";
import { createStaffTasksApplication, type StaffTasksPorts, type StaffTasksRepository } from "@/server/modules/operations/application/staff-tasks";
import type { TaskRow } from "@/server/modules/operations/domain/staff-tasks";
import type { ProjectEditorScope } from "@/lib/projects/project-editor-command";

// Exercise the original native action policy; any unexpected read/write port fails.
function taskDisplayPolicyPorts<Ports extends object>(facts: Partial<Ports>): Ports {
  return new Proxy(facts, { get(target, name) {
    if (name in target) return target[name as keyof Ports];
    return () => { throw Error("Unexpected Task display policy port: " + String(name)); };
  } }) as Ports;
}
const taskDisplayPolicy = createStaffTasksApplication(taskDisplayPolicyPorts<StaffTasksPorts<never>>({
  repository: taskDisplayPolicyPorts<StaffTasksRepository<never>>({}),
  approvalView: async () => ({ instance: null, steps: [], history: [], currentApprover: null, blockedReason: null, canDecide: false, canOverride: false }),
  taskRoute: async scope => ({ scope, steps: [], blockedReason: null }),
}));
function taskDisplayRow(state: "draft" | "in_progress" | "completed", projectId: string | null): TaskRow {
  return { id: "native-task-" + state, company_id: "company", branch_id: null, department_id: null,
    title: "Actual reviewed task", description: "Native task instructions", creator_user_id: "worker", creator_name: "Worker",
    assignee_user_id: "assignee", assignee_name: "Accountable worker", project_id: projectId,
    project_title: projectId ? "Actual reviewed Project" : null, project_owner_id: projectId ? "worker" : null,
    due_at: null, completion_note: "", start_approvers_json: "[]", completion_approvers_json: '["independent-reviewer"]',
    state, blocked_reason: null, version: 17, revision: 4, updated_at: 123, template_id: null, template_version: null,
    schedule_id: null, occurrence_at: null, active_delegation_id: null, active_delegate_user_id: null,
    active_delegate_name: null, active_delegate_expires_at: null };
}
function taskDisplayEditors(tree: ReactNode): ReactElement<ComponentProps<typeof DetailTaskEditor>>[] {
  if (Array.isArray(tree)) return tree.flatMap(taskDisplayEditors);
  if (!isValidElement<ComponentProps<typeof DetailTaskEditor> & { children?: ReactNode }>(tree)) return [];
  return [...(tree.type === DetailTaskEditor ? [tree] : []), ...taskDisplayEditors(tree.props.children)];
}
function taskDisplayScopes(tree: ReactNode): ProjectEditorScope[] {
  if (Array.isArray(tree)) return tree.flatMap(taskDisplayScopes);
  if (!isValidElement<{ editorSource?: ProjectEditorScope; children?: ReactNode }>(tree)) return [];
  return [...(tree.props.editorSource ? [tree.props.editorSource] : []), ...taskDisplayScopes(tree.props.children)];
}
const taskDisplayProject = { id: "project", title: "Actual reviewed Project", version: 57, owner_user_id: "private-project-owner", description: "private-project-description", department_id: "private-project-department" };
async function taskDisplayPage(row: TaskRow) {
  const work = await taskDisplayPolicy.taskWorkState(projectTaskActor, row), context = taskPresentation();
  ports.presentation.mockResolvedValueOnce({ ...context, actor: projectTaskActor, projects: [taskDisplayProject], queries: {
    ...context.queries, taskDetail: async () => ({ task: row, ...work, attachments: [], delegation: null, delegationHistory: [] }),
    projectPurchases: async () => ({ "projects.wbs": { newWork: true, settleExisting: true, history: true, revision: "actual-reviewed-purchase" }, "projects.tasks": { newWork: true, settleExisting: true, history: true, revision: "actual-reviewed-purchase" } }),
  } });
  return { work, page: await Task({ params: Promise.resolve({ holdingSlug: "acme", id: row.id }), searchParams: Promise.resolve({}) }) };
}
for (const projectId of [null, "project"] as const) {
  it.each(["draft", "in_progress", "completed"] as const)(`retains one native ${projectId ? "Project" : "standalone"} Task editor for the %s owner branch with exact display-only choices`, async state => {
    const row = taskDisplayRow(state, projectId), before = structuredClone(source), { work, page } = await taskDisplayPage(row);
    expect(work.permittedActions).toEqual(state === "draft" ? ["edit", "submit", "revise", "cancel", "delegate"] : state === "in_progress" ? ["revise", "cancel", "delegate"] : []);
    const editors = taskDisplayEditors(page);
    expect(editors).toHaveLength(state === "completed" ? 0 : 1);
    expect(optionProps(page)).toEqual(state === "completed" ? [expected] : [expected, expected]);
    expect(projectProps(page)).toEqual(state === "completed" ? [] : [[{ id: "project", title: "Actual reviewed Project" }]]);
    expect(taskDisplayScopes(page)).toEqual([{ companyId: "company", actorId: "worker", available: state !== "completed", record: { kind: "task", id: row.id, version: 17, projectId, state } }]);
    if (state !== "completed") {
      const [editor] = editors;
      expect(editor.key).toBe(projectId ? `task-editor-${row.id}` : state === "draft" ? "edit-17" : "revise-17");
      expect(editor.props.task).toBe(row);
      expect(editor.props.canRevise).toBe(state === "in_progress");
      expect(editor.props.base).toBe("/holdings/acme"); expect(editor.props.userId).toBe("worker");
      expect(editor.props.companyId).toBe("company"); expect(editor.props.assignmentId).toBe("assignment");
      expect(editor.props.options).toEqual(expected); expect(editor.props.projects).toEqual([{ id: "project", title: "Actual reviewed Project" }]);
    }
    const encoded = JSON.stringify([optionProps(page), projectProps(page)]);
    for (const field of ["private-person", "private-engagement", "private-manager", "private-head", "private-level-code", "engagement_version", "self_service_top_level", "canManage", "private-project-owner", "private-project-description", "private-project-department"]) expect(encoded).not.toContain(field);
    expect(source).toEqual(before);
  });
  it(`does not synthesize a ${projectId ? "Project" : "standalone"} Task editor when the native owner grants no configuration action`, async () => {
    const row = { ...taskDisplayRow("in_progress", projectId), creator_user_id: "other-creator", project_owner_id: projectId ? "other-project-owner" : null }, { work, page } = await taskDisplayPage(row);
    expect(work.permittedActions).toEqual([]); expect(taskDisplayEditors(page)).toEqual([]);
    expect(optionProps(page)).toEqual([expected]); expect(projectProps(page)).toEqual([]);
    expect(taskDisplayScopes(page)).toEqual([{ companyId: "company", actorId: "worker", available: false, record: { kind: "task", id: row.id, version: 17, projectId, state: "in_progress" } }]);
    expect(JSON.stringify(optionProps(page))).not.toContain("private-");
  });
}
