"use client";

import { useState, type HTMLAttributes, type ReactNode } from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {workspacePathParts} from '@erpx/feature-work_projection/lib/destination-registry';
import { BriefcaseBusiness, ChevronRight } from "lucide-react";
import { Button } from "@erpx/ui/components/foundation";
import { PersonAvatar } from "@erpx/ui/components/person-avatar";
import { WorkspaceFrame, WorkspaceNavigation, WorkspaceToolbar, WorkspaceContent, WorkspaceContext, WorkspaceDetail, useWorkspaceDetailOverlay } from "@erpx/ui/components/workspace";
import { OrganizationUnitsTree, PositionPeopleGroup, PositionPeopleRow } from "@erpx/feature-organization/components/organization-structure";
import { organizationContextFor, organizationLabel, organizationLocationPath } from "@erpx/feature-organization/lib/organization-presentation";
import type { OrganizationExplorerContext, OrganizationSelection } from "@erpx/feature-people_payroll/components/organization-explorer";
import type { HoldingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import styles from "@erpx/feature-organization/components/organization-structure.module.css";

export function OrganizationStructureSurface({ data, initialCompanyId, initialSelection, headerActions, feedback, renderDetail, onSelect, renderActions, dropProps, positionStatus, controlledTab,onTabChange,detailSuppressed = false, testIdPrefix = "org" }: {
  data: HoldingPeopleDirectory; initialCompanyId: string; initialSelection: OrganizationSelection;
  headerActions?: ReactNode;
  feedback?: ReactNode;
  renderDetail: (selection: OrganizationSelection, tab: string, context: OrganizationExplorerContext) => ReactNode;
  onSelect: (selection: OrganizationSelection) => void;
  renderActions?: (selection: OrganizationSelection) => ReactNode;
  dropProps?: (selection: OrganizationSelection) => Omit<HTMLAttributes<HTMLElement>, "onToggle">;
  positionStatus?: (id: string) => ReactNode;
  detailSuppressed?: boolean;
  testIdPrefix?: string;
  controlledTab?:string;
  onTabChange?:(tab:string)=>void;
}) {
  const query = useSearchParams();
  const router=useRouter();
  const overlay = useWorkspaceDetailOverlay();
  const selection = initialSelection;
  const path = organizationLocationPath(data, selection);
  const rawUnit = query.get("unit"), split = rawUnit?.indexOf(":") ?? -1;
  const requested = rawUnit && split > 0 ? { kind: rawUnit.slice(0, split), id: rawUnit.slice(split + 1) } : null;
  const validUnits = ["company", "branch", "department", "team", "level", "unassigned"];
  const unit: OrganizationSelection = requested && validUnits.includes(requested.kind)
    ? requested as OrganizationSelection
    : validUnits.includes(selection.kind) ? selection : { kind: "company", id: initialCompanyId };
  const context = organizationContextFor(data, unit);
  const term = (query.get("q") ?? "").trim().toLocaleLowerCase();
  const open = !detailSuppressed && (query.get("detail") === "1" || ["person", "employee", "position", "department", "branch", "team", "level"].some(key => query.has(key)));
  const [expanded, setExpanded] = useState(() => new Set([initialCompanyId, ...path.map(item => item.id), ...data.nodes.filter(node => node.kind === "branch" || node.kind === "headquarters").map(node => node.id)]));
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const tabs = ["Policies", "Directory", "Delivery"];
  const tab = tabs.find(item => item.toLowerCase() === (controlledTab??query.get("tab"))?.toLowerCase()) ?? "Policies";
  function update(patch: Record<string, string | null>, replace = false) {
    const params = new URLSearchParams(query);
    for (const [key, value] of Object.entries(patch)) if (value === null) params.delete(key); else params.set(key, value);
    const base=workspacePathParts(window.location.pathname)?.base;
    router[replace?'replace':'push'](`${base?base+'/technology':window.location.pathname}?${params}`,{scroll:false});
  }
  function close() { update(Object.fromEntries(["person", "employee", "position", "department", "branch", "team", "company", "level", "detail", "tab","section"].map(key => [key, null]))); }
  function view(next: OrganizationSelection) {
    const params = Object.fromEntries(["person", "employee", "position", "department", "branch", "team", "level", "detail", "tab"].map(key => [key, null])) as Record<string, string | null>;
    params.unit = `${next.kind}:${next.id}`;
    update(params);
    setExpanded(old => new Set([...old, ...organizationLocationPath(data, next).map(item => item.id)]));
  }
  const positions = (term ? data.positions : context.positions).filter(position => !term || `${position.title} ${data.nodes.find(node => node.id === position.departmentId)?.name ?? ""} ${data.companies.find(company => company.id === position.companyId)?.name ?? ""}`.toLocaleLowerCase().includes(term) || data.people.some(person => person.positionId === position.id && person.legalName.toLocaleLowerCase().includes(term)));
  const holders = (id: string) => data.people.filter(person => person.positionId === id);
  const occupancy = query.get("occupancy") ?? "all";
  const filtered = positions.filter(position => occupancy === "all" || (occupancy === "filled" ? holders(position.id).length > 0 : holders(position.id).length === 0));
  const groups = Map.groupBy(filtered, position => `${position.companyId}:${position.branchId ?? ""}:${position.departmentId ?? ""}`);
  const loose = (term ? data.people : context.people).filter(person => !person.positionId && (!term || person.legalName.toLocaleLowerCase().includes(term)));
  const personRow = (person: HoldingPeopleDirectory["people"][number]) => <div key={person.engagementId} className={styles.person} data-testid={`${testIdPrefix}-person-${person.engagementId}`} data-selected={open && selection.kind === "person" && selection.id === person.engagementId || undefined}>
    <Button appearance="standard" variant="plain" type="button" className={styles.personName} aria-expanded={open && selection.kind === "person" && selection.id === person.engagementId} onClick={() => onSelect({kind:"person",id:person.engagementId})}><PersonAvatar name={person.legalName} identity={person.personId} small/><span className={styles.recordText}>{person.legalName}</span><ChevronRight size={16}/></Button>{renderActions?.({kind:"person",id:person.engagementId})}
  </div>;
  return <section className={styles.workspace} data-testid="organization-structure-surface">
    {(!overlay || !open) && feedback}
    <WorkspaceToolbar inert={overlay && open} aria-hidden={overlay && open || undefined} search={query.get("q") ?? ""} onSearch={q => update({q:q || null}, true)} searchLabel="Search people, positions, and units" actions={headerActions}/>
    <WorkspaceFrame data-detail-open={open || undefined}>
      <WorkspaceNavigation title="Units" label={organizationLabel(data, unit)} aria-label="Organization units" data-testid={`${testIdPrefix}-organization-rail`}>
        <OrganizationUnitsTree testIdPrefix={testIdPrefix} data={data} unit={unit} selection={selection} expanded={expanded} term={term} activePath={path} detailOpen={open} selectUnit={view} toggle={id => setExpanded(old => {const next = new Set(old); if(next.has(id)) next.delete(id); else next.add(id); return next;})} targetProps={dropProps}/>
        {data.levels.length > 0 && <details className={styles.section}><summary>Company levels</summary>{data.levels.map(level => <Button appearance="standard" data-testid={`${testIdPrefix}-level-${level.id}`} key={level.id} variant="plain" type="button" aria-pressed={unit.kind === "level" && unit.id === level.id} {...dropProps?.({kind:"level",id:level.id})} onClick={() => view({kind:"level",id:level.id})}>L{level.number} · {level.name}</Button>)}</details>}
      </WorkspaceNavigation>
      <WorkspaceContent>
        <WorkspaceContext label={term ? "Searching" : "Viewing"}><strong>{term ? "Search results" : organizationLocationPath(data, unit).map(item => organizationLabel(data, item)).join(" › ")}</strong></WorkspaceContext>
        <div className={styles.mainHeading}><div><span className={styles.eyebrow}>{organizationLabel(data, unit)}</span><h2>Positions and people</h2><p>{filtered.length} positions · {filtered.reduce((sum, position) => sum + holders(position.id).length, loose.length)} people</p></div><Button appearance="standard" variant="plain" type="button" onClick={() => onSelect(unit)}>Unit details</Button></div>
        {unit.kind !== "unassigned" && <div className={styles.occupancy} role="group" aria-label="Position occupancy">{["all", "filled", "vacant"].map(filter => <Button appearance="standard" key={filter} variant="plain" type="button" aria-pressed={occupancy === filter} onClick={() => update({occupancy:filter === "all" ? null : filter})}>{filter === "all" ? "All" : filter === "filled" ? "Filled" : "Vacant"}<span>{filter === "all" ? positions.length : positions.filter(position => (holders(position.id).length > 0) === (filter === "filled")).length}</span></Button>)}</div>}
        {[...groups].sort(([,left],[,right]) => (data.nodes.find(node => node.id === left[0].departmentId)?.name ?? "").localeCompare(data.nodes.find(node => node.id === right[0].departmentId)?.name ?? "")).map(([key, list]) => {
          const first = list[0], department = data.nodes.find(node => node.id === first.departmentId);
          const groupSelection: OrganizationSelection = department ? {kind:department.kind === "team" ? "team" : "department",id:department.id} : first.branchId ? {kind:"branch",id:first.branchId} : {kind:"company",id:first.companyId};
          return <PositionPeopleGroup key={key} title={department?.name ?? "Unit-wide"} subtitle={[data.companies.find(company => company.id === first.companyId)?.name, data.nodes.find(node => node.id === first.branchId)?.name].filter(Boolean).join(" · ")} count={<>{list.length} positions · {list.reduce((sum, position) => sum + holders(position.id).length, 0)} people</>} expanded={!collapsed.has(key)} onToggle={() => setCollapsed(old => { const next = new Set(old); if(next.has(key)) next.delete(key); else next.add(key); return next; })} onDetails={() => onSelect(groupSelection)} {...dropProps?.(groupSelection)}>
            {list.toSorted((left,right) => left.title.localeCompare(right.title) || left.id.localeCompare(right.id)).map(position => <PositionPeopleRow key={position.id} data-testid={`${testIdPrefix}-position-${position.id}`} data-selected={open && selection.kind === "position" && selection.id === position.id || undefined} positionSelected={open && selection.kind === "position" && selection.id === position.id} {...dropProps?.({kind:"position",id:position.id})} position={<><Button appearance="standard" variant="plain" type="button" className={styles.positionName} onClick={() => onSelect({kind:"position",id:position.id})} aria-expanded={open && selection.kind === "position" && selection.id === position.id}><BriefcaseBusiness size={18}/><span className={styles.recordText}>{position.title}<small>Reports to {data.positions.find(item => item.id === position.reportsToPositionId)?.title ?? "Company leadership"}</small>{positionStatus?.(position.id)}</span><ChevronRight size={16}/></Button>{renderActions?.({kind:"position",id:position.id})}</>} people={holders(position.id).length ? holders(position.id).map(personRow) : <span className={styles.vacant}>Vacant position</span>}/>)}</PositionPeopleGroup>;
        })}
        {!!loose.length && occupancy === "all" && <section className={styles.group}><div className={styles.groupHeading}><h3>Without a position</h3><span>{loose.length} people</span></div><div className={styles.loosePeople}>{loose.map(personRow)}</div></section>}
        {!filtered.length && !loose.length && <p className={styles.empty}>No matching positions or people.</p>}
      </WorkspaceContent>
      <WorkspaceDetail open={open} title={organizationLabel(data, selection)} onClose={close} data-testid={`${testIdPrefix}-detail`}>
        {overlay && <>{feedback}<WorkspaceToolbar actions={headerActions}/></>}
        <nav className={styles.occupancy} aria-label="Technology detail sections">{tabs.map(item => <Button appearance="standard" key={item} type="button" variant="plain" aria-pressed={tab === item} onClick={() => onTabChange?onTabChange(item):update({tab:item.toLowerCase()})}>{item}</Button>)}</nav>
        {renderDetail(selection, tab, organizationContextFor(data, selection))}
      </WorkspaceDetail>
    </WorkspaceFrame>
  </section>;
}
