"use client";
import type { CSSProperties, HTMLAttributes, ReactNode } from "react";
import { Building2, ChevronDown, ChevronRight, Folder } from "lucide-react";
import { Button } from "@/components/ui/foundation";
import type { HoldingPeopleDirectory } from "@/server/modules/organization/public/holding-people-directory";
import type { OrganizationSelection } from "@/components/people_payroll/organization-explorer";
import { organizationContextFor as contextFor } from "@/lib/organization/organization-presentation";
import styles from "./organization-structure.module.css";

type Source = { kind: "person" | "position" | "department"; id: string };
type Target = { kind: "branch" | "department" | "level" | "position"; id: string };
export function OrganizationUnitsTree({ data, unit, selection = null, expanded, term = "", activePath = [], detailOpen = false, toggle, selectUnit, actions = () => null, targetProps = () => ({}), change = () => false, testIdPrefix = "people" }: {
  testIdPrefix?: string;
  data: HoldingPeopleDirectory; unit: OrganizationSelection; selection?: OrganizationSelection | null;
  expanded: Set<string>; term?: string; activePath?: OrganizationSelection[]; detailOpen?: boolean;
  toggle: (id: string) => void; selectUnit: (selection: OrganizationSelection) => void;
  actions?: (source: Source) => ReactNode; targetProps?: (target: Target) => HTMLAttributes<HTMLElement>;
  change?: (source: Source) => unknown;
}) {
  const visibleCompanies = data.companies.filter(company => !term || contextFor(data, {kind:"company",id:company.id}).people.some(person => `${person.legalName} ${person.jobTitle} ${person.workEmail ?? ""}`.toLocaleLowerCase().includes(term)) || company.name.toLocaleLowerCase().includes(term) || data.nodes.some(node => node.companyId === company.id && node.name.toLocaleLowerCase().includes(term)) || data.positions.some(position => position.companyId === company.id && position.title.toLocaleLowerCase().includes(term)));
  function unitNode(
    node: HoldingPeopleDirectory["nodes"][number],
    depth: number,
    ancestors = new Set<string>(),
    inheritedMatch = false,
  ): ReactNode {
    if (ancestors.has(node.id)) return null;
    const nextAncestors = new Set([...ancestors, node.id]);
    const kind = node.kind === "headquarters" ? "branch" : node.kind;
    const item: OrganizationSelection = { kind, id: node.id };
    const children = data.nodes.filter(
      (n) =>
        (n.id !== node.id && n.parentId === node.id) ||
        (kind === "branch" &&
          n.branchId === node.id &&
          ["department", "team"].includes(n.kind) &&
          !data.nodes.some(
            (parent) => parent.id === n.parentId && ["department", "team"].includes(parent.kind),
          )),
    );
    const ownMatch =
      inheritedMatch ||
      node.name.toLocaleLowerCase().includes(term) ||
      data.companies
        .find((c) => c.id === node.companyId)
        ?.name.toLocaleLowerCase()
        .includes(term);
    const nodeContext = contextFor(data, item);
    const matches =
      !term ||
      ownMatch ||
      nodeContext.positions.some((p) => p.title.toLocaleLowerCase().includes(term)) ||
      nodeContext.people.some((p) =>
        `${p.legalName} ${p.jobTitle} ${p.workEmail ?? ""}`.toLocaleLowerCase().includes(term),
      );
    if (!matches) return null;
    return (
      <div key={node.id} className={styles.unitGroup} style={{
        "--unit-indent": `${Math.min(depth, 4) * 16}px`,
      } as CSSProperties}>
        <div
          className={styles.unitRow}
          style={{ marginLeft: Math.min(depth, 4) * 16 }}
          data-unit-kind={kind}
          data-detail-selected={detailOpen && selection?.id === node.id && unit.id !== node.id || undefined}
          data-ancestor={activePath.some((item) => item.id === node.id) && unit.id !== node.id && selection?.id !== node.id || undefined}
          data-people-drag-card={kind}
          data-selected={unit.id === node.id || undefined}
          data-testid={`${testIdPrefix}-${kind}-${node.id}`}
          {...(["branch", "department"].includes(kind) ? targetProps(item as Target) : {})}
        >
          {children.length ? (
            <Button appearance="standard"
              variant="plain"
              type="button"
              className={styles.expand}
              aria-label={`${expanded.has(node.id) ? "Collapse" : "Expand"} ${node.name}`}
              aria-expanded={expanded.has(node.id)}
              onClick={() => toggle(node.id)}
            >
              {expanded.has(node.id) ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            </Button>
          ) : (
            <span className={styles.expand}><Folder size={14} aria-hidden="true" /></span>
          )}
          <Button appearance="standard"
            variant="plain"
            type="button"
            className={styles.unitName}
            aria-pressed={unit.id === node.id}
            onClick={() => selectUnit(item)}
          >
            {kind === "branch" && <small className={styles.unitKind}>Branch{unit.id === node.id ? " · Viewing" : ""}</small>}
            {kind !== "branch" && unit.id === node.id && <small className={styles.unitKind}>Viewing {kind}</small>}
            {detailOpen && selection?.id === node.id && unit.id !== node.id && <small className={styles.unitKind}>Details open</small>}
            {node.name}
            {kind === "department" && !!change({ kind, id: node.id }) && (
              <span className={styles.dot} title="Pending change" />
            )}
          </Button>
          {kind === "department" && actions({ kind, id: node.id })}
        </div>
        {(expanded.has(node.id) || !!term) &&
          children.map((child) => unitNode(child, depth + 1, nextAncestors, !!ownMatch))}
      </div>
    );
  }
  return <>          {visibleCompanies.map((company) => (
            <div key={company.id} className={styles.company}>
              <div
                className={styles.unitRow}
                data-unit-kind="company"
                data-ancestor={activePath.some((item) => item.kind === "company" && item.id === company.id) && unit.id !== company.id && selection?.id !== company.id || undefined}
                data-detail-selected={detailOpen && selection?.kind === "company" && selection.id === company.id && unit.id !== company.id || undefined}
                data-selected={(unit.kind === "company" && unit.id === company.id) || undefined}
              >
                <Button appearance="standard"
                  variant="plain"
                  type="button"
                  className={styles.expand}
                  aria-expanded={expanded.has(company.id)}
                  aria-label={`${expanded.has(company.id) ? "Collapse" : "Expand"} ${company.name}`}
                  onClick={() => toggle(company.id)}
                >
                  {expanded.has(company.id) ? (
                    <ChevronDown size={16} />
                  ) : (
                    <ChevronRight size={16} />
                  )}
                </Button>
                <Button appearance="standard"
                  variant="plain"
                  type="button"
                  className={styles.unitName}
                  aria-pressed={unit.kind === "company" && unit.id === company.id}
                  onClick={() => selectUnit({ kind: "company", id: company.id })}
                >
                  <small className={styles.unitKind}><Building2 size={12} aria-hidden="true" /> Company{unit.kind === "company" && unit.id === company.id ? " · Viewing" : detailOpen && selection?.kind === "company" && selection.id === company.id ? " · Details open" : ""}</small>
                  {company.name}
                </Button>
              </div>
              {(expanded.has(company.id) || !!term) && (
                <>
                  {data.nodes
                    .filter(
                      (n) =>
                        n.companyId === company.id &&
                        n.kind !== "company" &&
                        (!n.parentId ||
                          !data.nodes.some((p) => p.id === n.parentId && p.kind !== "company")) &&
                        (!n.branchId || n.kind === "branch" || n.kind === "headquarters"),
                    )
                    .map((node) => unitNode(node, 1))}
                  <Button appearance="standard"
                    variant="plain"
                    type="button"
                    className={styles.unassigned}
                    aria-pressed={unit.kind === "unassigned" && unit.id === company.id}
                    onClick={() => selectUnit({ kind: "unassigned", id: company.id })}
                  >
                    Without position ·{" "}
                    {data.people.filter((p) => p.companyId === company.id && !p.positionId).length}
                  </Button>
                </>
              )}
            </div>
          ))}</>;
}

export function PositionPeopleGroup({ title, subtitle, count, expanded, onToggle, onDetails, children, ...props }: Omit<HTMLAttributes<HTMLElement>, "onToggle"> & {
  title: string; subtitle?: string; count: ReactNode; expanded: boolean; onToggle: () => void; onDetails: () => void;
}) {
  return <section {...props} className={styles.group}>
    <div className={styles.groupHeading}><h3 className={styles.groupTitle}><Button appearance="standard" variant="plain" type="button" className={styles.groupName} aria-label={title} aria-expanded={expanded} onClick={onToggle}>{expanded ? <ChevronDown size={16}/> : <ChevronRight size={16}/>}<span>{title}{subtitle && <small>{subtitle}</small>}</span></Button></h3>
    <span className={styles.groupCounts} data-testid="people-group-counts">{count}</span><Button appearance="standard" variant="plain" type="button" onClick={onDetails}>Details</Button></div>
    {expanded && <><div className={styles.columnLabels} aria-hidden="true"><span>Position</span><span>People</span></div>{children}</>}
  </section>;
}
export function PositionPeopleRow({ position, people, positionSelected, ...props }: HTMLAttributes<HTMLElement> & { position: ReactNode; people: ReactNode; positionSelected?: boolean }) {
  return <article {...props} className={styles.position}><div className={styles.positionHeading} data-detail-selected={positionSelected || undefined}><span className={styles.cellLabel} aria-hidden="true">Position</span>{position}</div><div className={styles.holders}><span className={styles.cellLabel} aria-hidden="true">People</span>{people}</div></article>;
}
