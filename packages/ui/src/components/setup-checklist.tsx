"use client";
import NextLink from "next/link";
import type { ComponentProps, ComponentType, ReactNode } from "react";
import { NavigationLink } from "@erpx/ui/components/navigation-context";
export type SetupItem = {
  id: string;
  label: string;
  ready: boolean;
  owner: string;
  href: string;
  detail?: string;
};
export function SetupChecklist({
  title = "Readiness",
  items,
  linkComponent: Link = NavigationLink,
  labels,
}: {
  title?: string;
  items: SetupItem[];
  linkComponent?: ComponentType<ComponentProps<typeof NextLink>>;
  labels?: { ready?: string; review?: string; setup?: string; remaining?: (count: number) => ReactNode };
}) {
  const missing = items.filter((item) => !item.ready).length;
  return (
    <section className="card setup-checklist" aria-label={title}>
      <header className="section-head">
        <h2>{title}</h2>
        <span className={`badge ${missing ? "warning" : "success"}`}>
          {missing
            ? labels?.remaining?.(missing) ?? `${missing} to prepare`
            : labels?.ready ?? "Ready"}
        </span>
      </header>
      <ul>
        {items.map((item) => (
          <li key={item.id}>
            <span aria-hidden>{item.ready ? "✓" : "○"}</span>
            <div>
              <strong>{item.label}</strong>
              <p className="muted">
                {item.owner}
                {item.detail ? ` · ${item.detail}` : ""}
              </p>
            </div>
            <Link href={item.href}>{item.ready ? labels?.review ?? "Review" : labels?.setup ?? "Set up"}</Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
