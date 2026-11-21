"use client";

import { useId, useState } from "react";
import { ChevronDown, type LucideIcon } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { SidebarItemContent, SidebarSection, sidebarItemClassName, useSidebarAccordion } from "@erpx/ui/components/sidebar-navigation";
import { groupHospitalitySidebar } from "@erpx/feature-work_projection/lib/sidebar-catalog";
import Link from "@erpx/workplace/components/workspace-link";
import styles from "./grouped-navigation.module.css";

type Item = { id: string; label: string; description: string; icon: LucideIcon; href?: string };

/** Native owners supply the eligible entries and the original navigation action. */
export function HospitalityGroupedNavigation({ app, items, current, label, language, onSelect, className }: {
  app: "pos" | "pms"; items: Item[]; current: string; label: string; language: "th" | "en";
  onSelect?: (id: string) => void; className?: string;
}) {
  const [expanded, setExpanded] = useState(false), id = useId();
  const groups = groupHospitalitySidebar(app, items, language);
  const active = groups.find(group => group.items.some(item => item.id === current))?.id ?? null;
  const { openId, toggle } = useSidebarAccordion({ ids: groups.map(group => group.id), activeId: active, storageKey: `erpx-${app}-sidebar:v1`, locationKey: current });
  const selected = items.find(item => item.id === current);
  return <div className={`${styles.navigation} ${className ?? ""}`} data-expanded={expanded}>
    <Button appearance="standard" variant="secondary" type="button" className={styles.picker}
      aria-expanded={expanded} aria-controls={id} onClick={() => setExpanded(value => !value)}>
      <span><small>{label}</small><strong>{selected?.label ?? label}</strong></span><ChevronDown size={18} aria-hidden="true" />
    </Button>
    <nav id={id} className={styles.groups} aria-label={label}>
      {groups.map(group => {
        const GroupIcon = group.items[0].icon;
        return <SidebarSection key={group.id} label={group.label} layout="tiles" icon={<GroupIcon size={18} aria-hidden="true" />}
          tone={group.tone} open={openId === group.id} active={active === group.id} onToggle={() => toggle(group.id)}>
          {group.items.map(item => {
            const Icon = item.icon;
            const content = <SidebarItemContent icon={<Icon size={18} aria-hidden="true" />} tone={group.tone} description={item.description}>{item.label}</SidebarItemContent>;
            return item.href ? <Link key={item.id} className={sidebarItemClassName} href={item.href}
              aria-label={item.label} aria-current={current === item.id ? "page" : undefined} onClick={() => setExpanded(false)}>{content}</Link>
              : <Button key={item.id} appearance="standard" variant="plain" type="button" className={sidebarItemClassName}
                aria-label={item.label} aria-current={current === item.id ? "page" : undefined} onClick={() => { setExpanded(false); onSelect?.(item.id); }}>{content}</Button>;
          })}
        </SidebarSection>;
      })}
    </nav>
  </div>;
}
