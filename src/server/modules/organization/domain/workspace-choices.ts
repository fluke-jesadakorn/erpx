import {buildWorkspaceHoldingChoice,type WorkspaceChoiceAssignment,type WorkspaceHoldingChoice,type WorkspaceChoiceNode} from "@/lib/work_projection/workspace-choice";
export type AssignmentRow = {
  role: string;
  company_id: string;
  company_name: string;
  scope_kind: "holding" | "branch" | "department";
  scope_node_id: string | null;
};

export type NodeRow = {
  id: string;
  kind: "branch" | "department";
  slug: string;
  display_name: string;
  company_id: string;
  branch_id: string | null;
  branch_slug: string | null;
};


export type WorkspaceHoldingIdentity={id:string;slug:string;displayName:string};
export type WorkspaceBrandFact={scope_kind:'holding'|'branch';scope_id:string;primary_publication_id:string|null};
export function workspaceChoiceAssignments(rows:AssignmentRow[],definitions:Map<string,Map<string,readonly string[]>>){const assignments:WorkspaceChoiceAssignment[]=[];for(const row of rows){const scopes=definitions.get(row.company_id)?.get(row.role);if(!scopes?.includes(row.scope_kind))continue;assignments.push({companyId:row.company_id,companyName:row.company_name,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id});}return assignments;}
export function workspaceChoicePresentation(holding:WorkspaceHoldingIdentity,assignments:WorkspaceChoiceAssignment[],nodeRows:NodeRow[],profiles:WorkspaceBrandFact[]):WorkspaceHoldingChoice|null{const nodes:WorkspaceChoiceNode[]=nodeRows.map(row=>({id:row.id,kind:row.kind,slug:row.slug,label:row.display_name,companyId:row.company_id,branchId:row.kind==='department'?row.branch_id:null,branchSlug:row.kind==='department'?row.branch_slug:null})),choice=buildWorkspaceHoldingChoice(holding.slug,holding.displayName,assignments,nodes);if(!choice)return null;const short=(label:string)=>label.trim().split(/\s+/u).filter(Boolean).slice(0,2).map(word=>Array.from(word)[0]??'').join('').toLocaleUpperCase().slice(0,3)||'—';const present=(label:string,kind:'holding'|'branch',id:string)=>{const publication=profiles.find(p=>p.scope_kind===kind&&p.scope_id===id)?.primary_publication_id;return {label,initials:short(label),...(publication?{primaryUrl:`/api/public/holdings/${encodeURIComponent(holding.slug)}/media-publications/${encodeURIComponent(publication)}/content`}:{})};};return {...choice,brand:present(choice.label,'holding',holding.id),companies:choice.companies.map(company=>({...company,branches:company.branches.map(branch=>({...branch,brand:present(branch.label,'branch',branch.id)}))}))};}
