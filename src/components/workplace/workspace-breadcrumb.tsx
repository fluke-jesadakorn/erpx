"use client";

import Link from "next/link";
import { useSelectedLayoutSegments } from "next/navigation";
import { ChevronRight } from "lucide-react";
import {
  findModule,
  moduleHref,
  type ModuleDefinition,
} from "@/data/module-registry";
import styles from "./shell.module.css";

export type WorkspaceCrumb = {
  href?: string;
  label: string;
  fullNavigation?: boolean;
};
type Crumb = WorkspaceCrumb;

const destinationLabels: Record<string, string> = {
  feed: "Updates",
  home: "Home",
  inbox: "Inbox",
  learn: "Learn",
  modules: "Modules",
  work: "Work",
};

function readableSegment(segment: string) {
  try {
    return decodeURIComponent(segment)
      .replaceAll("_", " ")
      .replaceAll("-", " ")
      .replace(/\b\w/g, (letter) => letter.toUpperCase());
  } catch {
    return segment;
  }
}

function moduleCrumbs(
  workspaceBase: string,
  definition: ModuleDefinition | undefined,
  sectionId?: string,
): Crumb[] {
  if (!definition) {
    return [
      { href: `${workspaceBase}/modules`, label: "Modules" },
      { label: "Module" },
    ];
  }

  if (!sectionId) {
    return [
      { href: `${workspaceBase}/modules`, label: "Modules" },
      { label: definition.name },
    ];
  }

  const section = definition.sections.find((item) => item.id === sectionId);
  return [
    { href: `${workspaceBase}/modules`, label: "Modules" },
    {
      href: moduleHref(workspaceBase, definition),
      label: definition.name,
    },
    { label: section?.label ?? readableSegment(sectionId) },
  ];
}

function routeCrumbs(workspaceBase: string, segments: string[]): Crumb[] {
  const [destination, second, third, fourth] = segments;

  if (!destination || destination === "home") return [];

  if (destination === "modules") {
    if (!second) return [{ label: "Modules" }];
    return moduleCrumbs(workspaceBase, findModule(second), third);
  }

  if (destination === "tasks") {
    return [
      { href: `${workspaceBase}/work`, label: "Work" },
      {
        label:
          second === "people_payroll"
            ? "Annual leave request"
            : readableSegment(second ?? "Task"),
      },
    ];
  }

  if (destination === "preview" && second === "records") {
    const definition = third ? findModule(third) : undefined;
    return [
      { href: `${workspaceBase}/modules`, label: "Modules" },
      ...(definition
        ? [
            {
              href: moduleHref(workspaceBase, definition),
              label: definition.name,
            },
          ]
        : []),
      { label: fourth ? readableSegment(fourth) : "Record preview" },
    ];
  }

  const collections: Record<string, [string, string]> = {
    expenses: ["Expenses", "Expense"],
    handbook: ["Handbook", "Article"],
    people: ["People", "Person"],
    projects: ["Projects", "Project"],
    requests: ["Requests", "Request"],
    "routine-templates": ["Routines", "Routine"],
    "staff-tasks": ["Tasks", "Task"],
    "stock-counts": ["Stock counts", "Stock count"],
    "stock-transfers": ["Stock transfers", "Stock transfer"],
  };
  if (collections[destination]) {
    const [label, detail] = collections[destination];
    if (!second) return [{ label }];
    return [
      { label, href: `${workspaceBase}/${destination}` },
      { label: destination === "people" && second === "payroll" ? "Payroll run" : detail },
    ];
  }
  if (destination === "finance" && second) {
    const labels: Record<string, string> = {
      bank: "Bank reconciliation",
      ledger: "General ledger",
      invoices: "Invoices",
      journals: "Journal",
      payments: "Payment",
    };
    const label = labels[second] ?? readableSegment(second);
    return [
      { label: "Finance", href: `${workspaceBase}/finance` },
      {
        label,
        ...(second === "invoices" && third
          ? { href: `${workspaceBase}/finance/invoices` }
          : {}),
      },
      ...(second === "invoices" && third ? [{ label: "Invoice" }] : []),
    ];
  }
  if (destination === "commerce" && second === "orders")
    return [{ label: "Commerce", href: `${workspaceBase}/commerce` }, { label: "Order" }];
  if (destination === "posts")
    return [{ label: "Updates", href: `${workspaceBase}/feed` }, { label: "Post" }];
  if (destination === "records" && second) {
    const definition = findModule(second);
    return [
      { label: "Modules", href: `${workspaceBase}/modules` },
      ...(definition ? [{ label: definition.name, href: moduleHref(workspaceBase, definition) }] : []),
      { label: third === "new" ? "New record" : "Record" },
    ];
  }

  return [
    {
      label: destinationLabels[destination] ?? readableSegment(destination),
    },
  ];
}

export function WorkspaceBreadcrumb({
  workspaceBase,
  ancestors,
}: {
  workspaceBase: string;
  ancestors: WorkspaceCrumb[];
}) {
  const segments = useSelectedLayoutSegments()
    .filter((segment) => !segment.startsWith("("))
    .flatMap((segment) => segment.split("/"));
  const crumbs = [...ancestors, ...routeCrumbs(workspaceBase, segments)];

  return (
    <nav className={styles.breadcrumb} aria-label="Breadcrumb">
      <ol>
        {crumbs.map((crumb, index) => {
          const current = index === crumbs.length - 1;
          return (
            <li key={`${crumb.label}-${index}`} className={styles.breadcrumbItem}>
              {index > 0 && (
                <ChevronRight size={14} aria-hidden="true" className={styles.breadcrumbSeparator} />
              )}
              {crumb.href && !current ? (
                crumb.fullNavigation ? (
                  <a href={crumb.href} title={crumb.label}>{crumb.label}</a>
                ) : (
                  <Link href={crumb.href} title={crumb.label}>{crumb.label}</Link>
                )
              ) : (
                <span aria-current={current ? "page" : undefined} title={crumb.label}>
                  {crumb.label}
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
