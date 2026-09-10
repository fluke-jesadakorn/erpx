"use client";
import Link from "next/link";
import { useState } from "react";
import { ChevronDown, ArrowUpRight } from "lucide-react";
import {
  type ModuleDefinition,
  moduleHref,
  capabilitySupport,
} from "@/data/module-registry";
import { Dialog } from "@/components/ui/dialog";
import styles from "./modules.module.css";
export function ModuleTabs({
  workspaceBase,
  module,
  active,
}: {
  workspaceBase: string;
  module: ModuleDefinition;
  active: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  return (
    <nav className={styles.tabs} aria-label={`${module.name} sections`}>
      {module.primary.map((id) => {
        const section = module.sections.find((item) => item.id === id)!;
        return (
          <Link
            key={id}
            href={moduleHref(workspaceBase, module, id)}
            aria-current={active === id ? "page" : undefined}
          >
            {section.label}
          </Link>
        );
      })}
      <div className={styles.more}>
        <button
          type="button"
          onClick={() => setOpen(true)}
          data-active={!module.primary.includes(active)}
          aria-expanded={open}
        >
          More <ChevronDown size={13} />
        </button>
      </div>
      <Dialog
        open={open}
        title={`${module.name} · All sections`}
        onClose={() => setOpen(false)}
      >
        <input
          className="input"
          type="search"
          placeholder="Find a section…"
          aria-label="Search module sections"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className={styles.moreList}>
          {module.sections
            .filter((section) =>
              section.label.toLowerCase().includes(query.toLowerCase()),
            )
            .map((section) => (
              <Link
                key={section.id}
                href={moduleHref(workspaceBase, module, section.id)}
                aria-current={section.id === active ? "page" : undefined}
                onClick={() => setOpen(false)}
              >
                <span>
                  {section.label}
                  <small
                    style={{
                      display: "block",
                      color: "var(--muted)",
                      marginTop: 3,
                    }}
                  >
                    {capabilitySupport(module.id, section.id).availability ===
                    "preview"
                      ? "Preview"
                      : "Includes live actions"}
                  </small>
                </span>
                <ArrowUpRight size={13} />
              </Link>
            ))}
          {!module.sections.some((section) =>
            section.label.toLowerCase().includes(query.toLowerCase()),
          ) && (
            <p className="small">No sections match. Try a different name.</p>
          )}
        </div>
      </Dialog>
    </nav>
  );
}
