"use client";

import { useState } from "react";
import { usePathname } from "next/navigation";
import { Factory, GraduationCap, Grid2X2, Handshake, Hotel, Landmark, LifeBuoy, Menu, Network, Package, Route, Scale, Settings, ShoppingCart, UserPlus, UserRound, Users, Wrench, type LucideIcon } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { Dialog } from "@erpx/ui/components/dialog";
import { SidebarItemContent, SidebarSection, sidebarItemClassName, useSidebarAccordion } from "@erpx/ui/components/sidebar-navigation";
import { groupAppsSidebar, type AppsSidebarItem } from "@erpx/feature-work_projection/lib/sidebar-catalog";
import { ProductAccountTrigger } from "@erpx/workplace/components/product-account-provider";
import styles from "@erpx/workplace/components/product-shell.module.css";

const icons: Record<string, LucideIcon> = {
  applications: Grid2X2, apps: Grid2X2, pos: ShoppingCart, pms: Hotel, finance: Landmark,
  people: Users, sales: Handshake, supply: Package, assets: Wrench, projects: Network,
  recruit: UserPlus, learn: GraduationCap, support: LifeBuoy, service: Route, legal: Scale,
  manufacturing: Factory, account: UserRound, administration: Settings,
};

export function AppsSidebar({ items, language, roleLabel, currentId }: {
  items: readonly AppsSidebarItem[]; language: "th" | "en"; roleLabel?: string; currentId?: string;
}) {
  const [open, setOpen] = useState(false), th = language === "th";
  const pathname = usePathname();
  const { primary, groups, account } = groupAppsSidebar(items, language);
  const current = currentId ?? items.filter(item => item.href.startsWith("/") &&
    (item.href === pathname || item.href !== "/" && pathname.startsWith(item.href + "/")))
    .sort((a, b) => b.href.length - a.href.length)[0]?.id;
  const activeGroup = groups.find(group => group.items.some(item => item.id === current))?.id ?? null;
  const { openId, toggle } = useSidebarAccordion({ ids: groups.map(group => group.id), activeId: activeGroup, storageKey: "erpx-apps-sidebar:v1", locationKey: pathname });
  function entry(item: AppsSidebarItem, tone: Parameters<typeof SidebarItemContent>[0]["tone"] = "brand") {
    const Icon = icons[item.productId ?? item.id] ?? Settings;
    return <a className={sidebarItemClassName} key={item.id} data-product-switch href={item.href}
      aria-current={item.id === current ? "page" : undefined} onClick={() => setOpen(false)}>
      <SidebarItemContent icon={<Icon size={18} aria-hidden="true" />} tone={tone}>{item.label}</SidebarItemContent>
    </a>;
  }
  // Each instance gets its own aria-controls IDs, including the mobile panel.
  function contents() {
    return <><nav className={styles.launcherLinks} aria-label={th ? "แอปและพื้นที่ทำงาน" : "Apps and workspaces"}>
      {primary.length > 0 && <div className={styles.launcherPrimary}>{primary.map(item => entry(item))}</div>}
      <div className={styles.launcherGroups}>{groups.map(group => {
        const Icon = icons[group.items[0].productId ?? group.id] ?? Settings;
        return <SidebarSection key={group.id} label={group.label} icon={<Icon size={18} aria-hidden="true" />}
          tone={group.tone} open={openId === group.id} active={activeGroup === group.id} onToggle={() => toggle(group.id)}>
          {group.items.map(item => entry(item, group.tone))}
        </SidebarSection>;
      })}</div>
    </nav><div className={styles.launcherAccount}>{account && entry(account)}<ProductAccountTrigger sidebar roleLabel={roleLabel} language={language} /></div></>;
  }
  return <div className={styles.sidebarContainer}>
    <div className={styles.mobileSidebar}><Button appearance="standard" variant="ghost" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}><Menu size={18} aria-hidden="true" />{th ? "แอปและบัญชี" : "Apps and account"}</Button></div>
    <aside className={styles.launcherSidebar} aria-label={th ? "แถบด้านข้าง" : "Sidebar"}>{contents()}</aside>
    <Dialog appearance="standard" presentation="responsive-panel" title={th ? "แอปและบัญชี" : "Apps and account"} open={open} onClose={() => setOpen(false)}><div className={styles.mobileSidebarContents}>{contents()}</div></Dialog>
  </div>;
}
