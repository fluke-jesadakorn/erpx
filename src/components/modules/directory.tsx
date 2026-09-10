"use client";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { Search, ArrowUpRight } from "lucide-react";
import {
  moduleRegistry,
  businessGroups,
  moduleAvailability,
  moduleHref,
} from "@/data/module-registry";
import { ModuleIcon } from "@/components/workplace/module-icon";
import { PageHeader, EmptyState } from "@/components/ui/primitives";
import styles from "./modules.module.css";
export function ModuleDirectory({
  workspaceBase,
  allowedModules,
}: {
  workspaceBase: string;
  allowedModules: string[];
}) {
  const search = useSearchParams(),
    pathname = usePathname(),
    q = search.get("q") ?? "",
    group = search.get("group") ?? "all",
    availability = search.get("availability") ?? "all";
  const accessible = moduleRegistry.filter((m) =>
    allowedModules.includes(m.id),
  );
  const groups = businessGroups.filter((group) =>
    accessible.some((m) => m.group === group),
  );
  const filtered = accessible.filter(
    (m) =>
      (group === "all" || m.group === group) &&
      (availability === "all" || moduleAvailability(m) === availability) &&
      `${m.name} ${m.description} ${m.sections.map((s) => s.label).join(" ")}`
        .toLowerCase()
        .includes(q.toLowerCase()),
  );
  return (
    <main className="content">
      <PageHeader
        title="ERP tools"
        description="Tools available in your current workspace."
      />
      <form className={styles.filters} action={pathname}>
        <div className={styles.search}>
          <Search size={15} />
          <input
            className="input"
            type="search"
            name="q"
            aria-label="Search modules"
            placeholder="Find a tool…"
            defaultValue={q}
          />
        </div>
        <select
          className="input"
          name="group"
          aria-label="Business group"
          defaultValue={group}
        >
          <option value="all">All business groups</option>
          {groups.map((label) => (
            <option key={label}>{label}</option>
          ))}
        </select>
        <button className="button secondary">Search</button>
        {(q || group !== "all" || availability !== "all") && (
          <Link className="button ghost" href={pathname}>
            Reset
          </Link>
        )}
      </form>
      {!filtered.length ? (
        <EmptyState
          title="No matching tools"
          description="Try another name or change your workspace."
        />
      ) : (
        groups
          .filter((group) => filtered.some((m) => m.group === group))
          .map((group) => (
            <section key={group}>
              <header className={styles.directoryHeader}>
                <h2>{group}</h2>
              </header>
              <div className={styles.moduleGrid}>
                {filtered
                  .filter((m) => m.group === group)
                  .map((m) => (
                    <Link
                      key={m.id}
                      href={moduleHref(workspaceBase, m)}
                      className={styles.moduleCard}
                    >
                      <div className={styles.moduleTop}>
                        <span className={styles.moduleIcon}>
                          <ModuleIcon id={m.id} />
                        </span>
                        <ArrowUpRight size={15} />
                      </div>
                      <h2>{m.name}</h2>
                      <p>{m.description}</p>
                    </Link>
                  ))}
              </div>
            </section>
          ))
      )}
    </main>
  );
}
