"use client";

import { useState, type DragEvent } from "react";
import {
  ArrowRight,
  BriefcaseBusiness,
  Building2,
  ChevronRight,
  Folder,
  GripVertical,
  Layers3,
  Search,
  UserRound,
} from "lucide-react";
import type { HoldingPeopleDirectory, HoldingDirectoryNode } from "@/server/holding-people-directory";
import { organizationMoveDenial, type OrganizationSelection } from "./organization-explorer";
import styles from "./people-move-picker.module.css";

type PeopleMoveKind = "person" | "position" | "department";
type TargetKind = "position" | "department" | "level" | "branch";
type Target = { kind: TargetKind; id: string };
type Category = "position" | "department" | "level" | "branch";
type FolderItem = { id: string; name: string; kind: HoldingDirectoryNode["kind"]; path: string };
type TargetItem = Target & { name: string; path: string; meta: string; searchText: string };

const dragMime = "application/erpx-people-move";

function countLabel(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function PeopleMovePicker({
  base,
  directory,
  kind,
  sourceId,
  initialTarget,
}: {
  base: string;
  directory: HoldingPeopleDirectory;
  kind: PeopleMoveKind;
  sourceId: string;
  initialTarget?: Target | null;
}) {
  const person = kind === "person" ? directory.people.find(item => item.id === sourceId) : undefined;
  const position = kind === "position" ? directory.positions.find(item => item.id === sourceId) : undefined;
  const department = kind === "department" ? directory.nodes.find(item => item.id === sourceId && item.kind === "department") : undefined;
  const companyId = person?.companyId ?? position?.companyId ?? department?.companyId ?? "";
  const company = directory.companies.find(item => item.id === companyId);
  const nodes = directory.nodes.filter(item => item.companyId === companyId);
  const companyNodes = new Map(nodes.map(item => [item.id, item]));
  const positions = directory.positions.filter(item => item.companyId === companyId);
  const people = directory.people.filter(item => item.companyId === companyId);
  const levels = directory.levels.filter(item => item.companyId === companyId);
  const counts = (() => {
    const byPosition = new Map<string, number>();
    const peopleByDepartment = new Map<string, number>();
    const peopleByBranch = new Map<string, number>();
    const positionsByDepartment = new Map<string, number>();
    const positionsByBranch = new Map<string, number>();
    const positionsByLevel = new Map<string, number>();
    const add = (map: Map<string, number>, id: string | null) => { if (id) map.set(id, (map.get(id) ?? 0) + 1); };
    for (const item of people) { add(byPosition, item.positionId); add(peopleByDepartment, item.departmentId); add(peopleByBranch, item.branchId); }
    for (const item of positions) { add(positionsByDepartment, item.departmentId); add(positionsByBranch, item.branchId); add(positionsByLevel, item.levelId); }
    return { byPosition, peopleByDepartment, peopleByBranch, positionsByDepartment, positionsByBranch, positionsByLevel };
  })();
  const initialCategory: Category = kind === "person" ? "position" : kind === "department" ? "branch" : "department";
  const [category, setCategory] = useState<Category>(initialTarget?.kind && kind === "position" && initialTarget.kind === "level" ? "level" : initialCategory);
  const [folderId, setFolderId] = useState(() => {
    if (kind === "person") return person?.departmentId ?? person?.branchId ?? "root";
    if (kind === "position") return position?.branchId ?? "root";
    return "root";
  });
  const [search, setSearch] = useState("");
  const [notice, setNotice] = useState("");

  function nodePath(id: string | null | undefined): string {
    if (!id) return company?.name ?? "Company";
    const trail: HoldingDirectoryNode[] = [];
    const seen = new Set<string>();
    let current = companyNodes.get(id);
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      if (current.kind !== "company") trail.push(current);
      current = current.parentId ? companyNodes.get(current.parentId) : undefined;
    }
    const branch = companyNodes.get(companyNodes.get(id)?.branchId ?? "");
    if (branch && !seen.has(branch.id) && branch.kind !== "company") trail.push(branch);
    return [company?.name ?? "Company", ...trail.reverse().map(item => item.name)].join(" › ");
  }

  function positionPath(target: HoldingPeopleDirectory["positions"][number]): string {
    return `${nodePath(target.departmentId ?? target.branchId)} › ${target.title}`;
  }

  const sourceName = person?.legalName ?? position?.title ?? department?.name ?? "Selected item";
  const sourcePosition = person ? positions.find(item => item.id === person.positionId) : undefined;
  const sourcePath = person
    ? sourcePosition ? positionPath(sourcePosition) : `${nodePath(person.departmentId ?? person.branchId)} › Unassigned`
    : position ? positionPath(position) : nodePath(department?.id);
  const sourceSubtitle = person
    ? sourcePosition?.title ?? person.jobTitle ?? "No position"
    : position ? countLabel(counts.byPosition.get(position.id) ?? 0, "current person")
      : `${countLabel(counts.positionsByDepartment.get(department?.id ?? "") ?? 0, "position")} · ${countLabel(counts.peopleByDepartment.get(department?.id ?? "") ?? 0, "person")}`;

  const allTargets: TargetItem[] = (() => {
    if (category === "position") return positions.map(item => {
      const path = positionPath(item);
      const count = counts.byPosition.get(item.id) ?? 0;
      return { kind: "position" as const, id: item.id, name: item.title, path, meta: count ? countLabel(count, "current person") : "Vacant", searchText: path };
    });
    if (category === "department") return nodes.filter(item => item.kind === "department").map(item => {
      const path = nodePath(item.id);
      return {
        kind: "department" as const, id: item.id, name: item.name, path,
        meta: `${countLabel(counts.positionsByDepartment.get(item.id) ?? 0, "position")} · ${countLabel(counts.peopleByDepartment.get(item.id) ?? 0, "person")}`,
        searchText: path,
      };
    });
    if (category === "level") return levels.filter(item => item.number !== null && Number.isInteger(item.number) && item.number >= 1 && item.number <= 6).map(item => {
      const path = `${company?.name ?? "Company"} › Company levels › ${item.name}`;
      return {
        kind: "level" as const, id: item.id, name: item.name, path,
        meta: `${item.code} · ${countLabel(counts.positionsByLevel.get(item.id) ?? 0, "position")}`,
        searchText: `${path} ${item.code}`,
      };
    });
    return nodes.filter(item => item.kind === "branch").map(item => {
      const path = nodePath(item.id);
      return {
        kind: "branch" as const, id: item.id, name: item.name, path,
        meta: `${countLabel(counts.positionsByBranch.get(item.id) ?? 0, "position")} · ${countLabel(counts.peopleByBranch.get(item.id) ?? 0, "person")}`,
        searchText: path,
      };
    });
  })();

  const source = { kind, id: sourceId };
  const deny = (target: Target): string | null => {
    const commonReason = organizationMoveDenial(directory, source, target as OrganizationSelection, id => id === companyId);
    if (commonReason) return commonReason;
    if (kind === "position" && target.kind === "level") {
      const number = levels.find(item => item.id === target.id)?.number;
      if (number == null || !Number.isInteger(number) || number < 1 || number > 6)
        return "Choose an active Company Level from L1 to L6.";
    }
    if (kind === "position" && target.kind === "department") {
      const node = companyNodes.get(target.id);
      if (!node?.branchId || companyNodes.get(node.branchId)?.kind !== "branch") return "Choose a department under an active Branch.";
    }
    if (kind === "department" && target.kind === "branch" && department) {
      if (companyNodes.get(target.id)?.kind !== "branch") return "Choose an active Branch.";
      if (nodes.some(item => item.id !== department.id && item.kind === "department" && item.branchId === target.id && item.slug === department.slug))
        return "A department with this slug already exists in that branch.";
    }
    return null;
  };
  const [selected, setSelected] = useState<Target | null>(() => initialTarget && !deny(initialTarget) ? initialTarget : null);
  const selectedItem = selected ? allTargets.find(item => item.kind === selected.kind && item.id === selected.id) : null;

  const activeFolder = companyNodes.get(folderId);
  const folder = activeFolder && ["branch", "headquarters", "department", "team"].includes(activeFolder.kind) ? activeFolder : undefined;
  const folderTrail: HoldingDirectoryNode[] = [];
  if (folder) {
    const seen = new Set<string>();
    let current: HoldingDirectoryNode | undefined = folder;
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      if (current.kind !== "company") folderTrail.unshift(current);
      current = current.parentId ? companyNodes.get(current.parentId) : undefined;
    }
    const branch = companyNodes.get(folder.branchId ?? "");
    if (branch && !seen.has(branch.id) && branch.kind !== "company") folderTrail.unshift(branch);
  }
  const isFolderNode = (item: HoldingDirectoryNode) => item.kind === "branch" || item.kind === "headquarters" || item.kind === "department" || item.kind === "team";
  const folders: FolderItem[] = (category === "position" || category === "department") ? nodes.filter(item => {
    if (!isFolderNode(item)) return false;
    if (folder?.kind === "department" || folder?.kind === "team") return item.parentId === folder.id;
    if (folder) return (item.kind === "department" || item.kind === "team") &&
      (item.parentId === folder.id || (item.branchId === folder.id && (!item.parentId || item.parentId === companyId)));
    if (item.kind === "branch" || item.kind === "headquarters") return true;
    return !item.branchId && (!item.parentId || item.parentId === companyId);
  }).map(item => ({ id: item.id, name: item.name, kind: item.kind, path: nodePath(item.id) })) : [];

  const visibleTargets = search.trim()
    ? allTargets.filter(item => item.searchText.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()))
    : category === "level" || category === "branch" ? allTargets
      : allTargets.filter(item => {
        if (category === "department" && folders.some(folderItem => folderItem.id === item.id)) return false;
        const target = category === "position" ? positions.find(value => value.id === item.id) : companyNodes.get(item.id);
        if (!target) return false;
        if (category === "position") {
          const value = target as HoldingPeopleDirectory["positions"][number];
          if (!folder) return !value.branchId && !value.departmentId;
          if (folder.kind === "branch" || folder.kind === "headquarters") return value.branchId === folder.id && !value.departmentId;
          return value.departmentId === folder.id;
        }
        const value = target as HoldingDirectoryNode;
        if (!folder) return !value.branchId && (!value.parentId || value.parentId === companyId);
        if (folder.kind === "branch" || folder.kind === "headquarters") return value.branchId === folder.id &&
          (value.parentId === folder.id || !value.parentId || value.parentId === companyId);
        return value.parentId === folder.id;
      });

  function syncTargetInUrl(target: Target | null) {
    const params = new URLSearchParams({ kind, id: sourceId });
    if (target) { params.set("targetKind", target.kind); params.set("targetId", target.id); }
    // Native history is supported by Next for same-page URL state. The next
    // full-page step uses a document link to avoid a concurrent RSC transition.
    window.history.replaceState(null, "", `${base}/people/move?${params}`);
  }

  function choose(target: Target) {
    const reason = deny(target);
    if (reason) { setNotice(reason); return; }
    setSelected(target);
    syncTargetInUrl(target);
    setNotice("");
  }

  function changeCategory(next: Category) {
    if (next === category) return;
    setCategory(next);
    setFolderId(position?.branchId ?? "root");
    setSearch("");
    setSelected(null);
    syncTargetInUrl(null);
    setNotice("");
  }

  function receiveDrop(event: DragEvent<HTMLElement>, target: Target | null) {
    event.preventDefault();
    const payload = event.dataTransfer.getData(dragMime);
    if (payload !== `${kind}:${sourceId}`) { setNotice("Drag the source card shown on this page."); return; }
    if (target) choose(target);
    else setNotice("Open this folder and choose a destination inside it.");
  }

  function folderRow(item: FolderItem) {
    const target: Target | null = category === "department" && item.kind === "department" ? { kind: "department", id: item.id } : null;
    const reason = target ? deny(target) : null;
    return <div className={styles.folderRow} key={`folder:${item.id}`} data-testid={target ? `people-move-target-${target.kind}-${target.id}` : undefined} onDragOver={event => event.preventDefault()} onDrop={event => receiveDrop(event, target)}>
      <button className={styles.folderOpen} type="button" onClick={() => { setFolderId(item.id); setNotice(""); }} aria-label={`Open ${item.name}`}>
        <Folder aria-hidden="true" size={19} /><span className={styles.rowText}><strong>{item.name}</strong><small>{item.path}</small>{reason && <small>{reason}</small>}</span><ChevronRight aria-hidden="true" size={18} />
      </button>
      {target && <button className={styles.folderSelect} type="button" aria-pressed={selected?.kind === target.kind && selected.id === target.id} aria-disabled={!!reason} onClick={() => choose(target)}>{reason ? "Unavailable" : "Select"}</button>}
    </div>;
  }

  function targetRow(item: TargetItem) {
    const target: Target = { kind: item.kind, id: item.id };
    const reason = deny(target);
    const active = selected?.kind === target.kind && selected.id === target.id;
    const Icon = item.kind === "position" ? BriefcaseBusiness : item.kind === "level" ? Layers3 : item.kind === "branch" ? Building2 : Folder;
    return <button
      className={styles.targetRow}
      key={`${item.kind}:${item.id}`}
      data-testid={`people-move-target-${item.kind}-${item.id}`}
      type="button"
      aria-pressed={active}
      aria-disabled={!!reason}
      onClick={() => choose(target)}
      onDragOver={event => event.preventDefault()}
      onDrop={event => receiveDrop(event, target)}
    >
      <Icon aria-hidden="true" size={19} />
      <span className={styles.rowText}><strong>{item.name}</strong><small>{item.path}</small><small>{reason ?? item.meta}</small></span>
      <span className={styles.radio} aria-hidden="true">{active ? "✓" : ""}</span>
    </button>;
  }

  const categoryLabel = category === "position" ? "Positions" : category === "department" ? "Departments" : category === "level" ? "Company levels" : "Branches";
  const reviewHref = selected ? `${base}/people/move/review?${new URLSearchParams({ kind, id: sourceId, targetKind: selected.kind, targetId: selected.id })}` : "";
  const cancelHref = `${base}/people?${new URLSearchParams({ [kind]: sourceId })}`;

  return <section className={styles.workspace} aria-label="Move people and organization">
    <div className={styles.stepBar}><span className={styles.stepActive}>1 · Choose destination</span><ArrowRight aria-hidden="true" size={16} /><span>2 · Review and confirm</span></div>
    <div className={styles.panes}>
      <section className={styles.sourcePane} aria-labelledby="move-source-title">
        <div className={styles.paneHeader}><span className={styles.eyebrow}>SOURCE</span><h2 id="move-source-title">Current location</h2></div>
        <div className={styles.sourceCard} data-testid="people-move-source" draggable onDragStart={event => {
          event.dataTransfer.setData(dragMime, `${kind}:${sourceId}`);
          event.dataTransfer.setData("text/plain", `${kind}:${sourceId}`);
          event.dataTransfer.effectAllowed = "move";
        }}>
          <span className={styles.sourceIcon}>{kind === "person" ? <UserRound aria-hidden="true" size={21} /> : kind === "position" ? <BriefcaseBusiness aria-hidden="true" size={21} /> : <Folder aria-hidden="true" size={21} />}</span>
          <span className={styles.sourceInfo}><strong>{sourceName}</strong><span>{sourceSubtitle}</span></span>
          <GripVertical className={styles.grip} aria-hidden="true" size={18} />
        </div>
        <p className={styles.sourcePath}>{sourcePath}</p>
        <p className={styles.helper}>Drag this card onto a destination, or select a destination on the right. Search works across every folder.</p>
      </section>
      <section className={styles.destinationPane} aria-labelledby="move-target-title">
        <div className={styles.paneHeader}><span className={styles.eyebrow}>DESTINATION</span><h2 id="move-target-title">Choose a destination</h2><p>Within {company?.name ?? "this company"}. Other companies require a separate employment change.</p></div>
        {kind === "position" && <div className={styles.categories} role="group" aria-label="Destination type">
          <button type="button" aria-pressed={category === "department"} onClick={() => changeCategory("department")}>Departments</button>
          <button type="button" aria-pressed={category === "level"} onClick={() => changeCategory("level")}>Company levels</button>
        </div>}
        <label className={styles.search}><Search aria-hidden="true" size={18} /><span className={styles.visuallyHidden}>Search {categoryLabel.toLowerCase()}</span><input type="search" value={search} onChange={event => { setSearch(event.target.value); setNotice(""); }} placeholder={`Search ${categoryLabel.toLowerCase()} across this company`} /></label>
        {!search.trim() && (category === "position" || category === "department") && <div className={styles.breadcrumbs} aria-label="Folder path">
          <button type="button" onClick={() => setFolderId("root")}>{company?.name ?? "Company"}</button>
          {folderTrail.map((item, index) => <span className={styles.crumb} key={item.id}><ChevronRight size={14} aria-hidden="true" /><button type="button" aria-current={index === folderTrail.length - 1 ? "location" : undefined} onClick={() => setFolderId(item.id)}>{item.name}</button></span>)}
        </div>}
        <div className={styles.results} role="group" aria-label={`${categoryLabel} destinations`}>
          {!search.trim() && folders.map(folderRow)}
          {visibleTargets.map(targetRow)}
          {(search.trim() || !folders.length) && !visibleTargets.length && <p className={styles.empty}>No {categoryLabel.toLowerCase()} here. {search.trim() ? "Try another search." : "Choose a folder above or use search."}</p>}
        </div>
        {notice && <p className={styles.notice} role="status">{notice}</p>}
      </section>
    </div>
    <div className={styles.footer}>
      <a className="button secondary" href={cancelHref}>Cancel</a>
      <span className={styles.selection}>{selectedItem ? `Destination: ${selectedItem.path}` : "Select a destination to continue"}</span>
      {selected ? <a className="button" href={reviewHref}>Continue to review <ArrowRight aria-hidden="true" size={17} /></a> : <button className="button" type="button" disabled>Continue to review <ArrowRight aria-hidden="true" size={17} /></button>}
    </div>
  </section>;
}
