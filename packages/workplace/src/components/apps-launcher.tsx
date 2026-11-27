"use client";

import { useEffect, useRef, useState, type MouseEvent } from "react";
import { ArrowUpRight, Building2, ChevronDown, Grid2X2, MapPin, MessageCircle, Settings, ShoppingBag } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { Dialog } from "@erpx/ui/components/dialog";
import { TextInput } from "@erpx/ui/components/input";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { emptyAppsMemory, readAppsMemory, writeAppsMemory, restoreAppsSelection, rememberAppsLaunch, visibleApps, type AppsCompany, type AppsWorkspace, type AppsMemory } from "@erpx/feature-work_projection/lib/apps-launcher";
import styles from "./apps-launcher.module.css";

const launcherHash = "#apps";

/** The tree previews choices. Only an authorized owner launch link changes context. */
export function AppsLauncher({ companies, accountId, language, provider, existingWorkspaces = [], currentCompanyId, currentWorkspaceId, productLabel, workspaceLabel, entry = false, requireSelection=false }: {
  companies: AppsCompany[]; accountId: string; language: "th" | "en";
  provider?: { href: string; role: string } | null;
  existingWorkspaces?: { holding: string; href: string }[];
  currentCompanyId?: string; currentWorkspaceId?: string; productLabel?: string; workspaceLabel?: string; entry?: boolean;requireSelection?:boolean;
}) {
  const th = language === "th", t = (thai: string, english: string) => th ? thai : english;
  const [memory, setMemory] = useState<AppsMemory>(emptyAppsMemory);
  const [open, setOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [areaQuery, setAreaQuery] = useState("");
  const [appQuery, setAppQuery] = useState("");
  const [showAreas, setShowAreas] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const initialized = useRef<string | null>(null);
  const launched = useRef(false);
  const current = restoreAppsSelection(companies, memory);
  const company = companies.find(item => item.id === current.memory.companyId);
  const workspace = company?.workspaces.find(item => item.id === current.memory.workspaceId);

  useEffect(() => {
    if (initialized.current === accountId) return;
    initialized.current = accountId;
    let saved = emptyAppsMemory();
    try { if(!requireSelection)saved = readAppsMemory(window.localStorage, accountId); } catch { /* Optional storage. */ }
    if (currentCompanyId && currentWorkspaceId) saved = { ...saved, companyId: currentCompanyId, workspaceId: currentWorkspaceId };
    const restored = restoreAppsSelection(companies, saved);
    setMemory(restored.memory);
    setNotice(restored.invalid);
    const requestedByQuery = new URLSearchParams(window.location.search).get("launcher") === "open";
    const requested = requestedByQuery || window.location.hash === launcherHash;
    if (requestedByQuery) {
      const url = new URL(window.location.href); url.searchParams.delete("launcher");
      window.history.replaceState(window.history.state, "", url.pathname + url.search + url.hash);
    }
    const area = companies.find(item => item.id === restored.memory.companyId)?.workspaces.find(item => item.id === restored.memory.workspaceId);
    const feed = area?.apps.find(app => app.productId === "apps");
    if (entry && !requireSelection && !requested && !restored.invalid && feed) {
      // This href comes from the fresh server-authorized snapshot, never storage.
      window.location.assign(feed.href);
    } else if (entry || requested) setOpen(true);
  }, [accountId, companies, currentCompanyId, currentWorkspaceId, entry,requireSelection]);

  useEffect(() => {
    const sync = () => {
      launched.current = false;
      setOpen(window.location.hash === launcherHash);
    };
    const show = () => {
      if (window.location.hash !== launcherHash) window.history.pushState(null, "", launcherHash);
      setOpen(true);
    };
    const restore = (event: PageTransitionEvent) => { if (event.persisted) sync(); };
    window.addEventListener("erpx:open-apps", show);
    window.addEventListener("popstate", sync);
    window.addEventListener("pageshow", restore);
    return () => {
      window.removeEventListener("erpx:open-apps", show);
      window.removeEventListener("popstate", sync);
      window.removeEventListener("pageshow", restore);
    };
  }, []);

  function close() {
    if (window.location.hash === launcherHash) window.history.back();
    else setOpen(false);
  }

  function select(item: AppsCompany, area?: AppsWorkspace) {
    setMemory({ ...current.memory, companyId: item.id, workspaceId: area?.id ?? null });
    setNotice(null); setAppQuery(""); setShowAreas(false);
  }
  function remember(productId?: string) {
    if (!company || !workspace) return;
    const next = productId ? rememberAppsLaunch(current.memory, company.id, productId) : current.memory;
    setMemory(next);
    try { writeAppsMemory(window.localStorage, accountId, next); } catch { /* Optional storage. */ }
  }

  function launch(event: MouseEvent<HTMLAnchorElement>, productId: string) {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    remember(productId);
    if (window.location.hash !== launcherHash) return;
    event.preventDefault();
    if (launched.current) return;
    launched.current = true;
    const tile = event.currentTarget;
    const href = tile.href;
    const navigate = () => window.location.replace(href);
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches || !tile.animate) {
      navigate();
      return;
    }
    const animation = tile.animate(
      [{ transform: "scale(1)" }, { transform: "scale(1.085)", filter: "brightness(1.12)" }],
      { duration: 155, easing: "cubic-bezier(.2,.75,.25,1)" },
    );
    animation.finished.then(navigate, navigate);
  }
  const matches = (text: string) => text.toLocaleLowerCase().includes(areaQuery.trim().toLocaleLowerCase());
  function areaButton(item: AppsCompany, area: AppsWorkspace) {
    const label = area.kind === "holding" ? item.label : area.branchLabel && area.label.startsWith(area.branchLabel + " / ") ? area.label.slice(area.branchLabel.length + 3) : area.label;
    return <button type="button" key={area.id} data-workspace-id={area.id} className={styles.area} aria-pressed={company?.id === item.id && workspace?.id === area.id} onClick={() => select(item, area)}><MapPin size={16} aria-hidden="true" /><span>{label}</span></button>;
  }
  const holdings = [...new Set(companies.map(item => item.holdingSlug ?? item.holding))].map(id => ({ id, companies: companies.filter(item => (item.holdingSlug ?? item.holding) === id) }));
  const apps = workspace ? visibleApps(workspace.apps, current.memory.recent[company!.id] ?? [], appQuery) : [];
  return <>
    <Button ref={trigger} appearance="standard" variant="ghost" className={styles.trigger} data-apps-launcher-trigger aria-label={[productLabel ?? t("แอป", "Apps"),workspaceLabel].filter(Boolean).join(" · ")} aria-haspopup="dialog" aria-expanded={open} onClick={() => window.dispatchEvent(new Event("erpx:open-apps"))}>
      <Grid2X2 size={18} aria-hidden="true" /><span>{productLabel ?? t("แอปและพื้นที่ทำงาน", "Apps and workspaces")}{workspaceLabel && <small>{workspaceLabel}</small>}</span><ChevronDown size={16} aria-hidden="true" />
    </Button>
    <Dialog appearance="standard" presentation="fullscreen" open={open} onClose={close} title={t("แอป", "Apps")} className={styles.modal} bodyClassName={styles.modalBody} finalFocus={() => trigger.current} closeLabel={t("ปิดแอป", "Close Apps")}>
      {(notice || current.invalid) && <InlineNotice tone="info">{t("พื้นที่ที่จำไว้ไม่อยู่ในสิทธิ์ปัจจุบันแล้ว กรุณาเลือกใหม่", "Your remembered workspace is no longer available. Choose again.")}</InlineNotice>}
      {workspace && <Button className={styles.areaToggle} appearance="standard" variant="ghost" aria-expanded={showAreas} onClick={() => setShowAreas(!showAreas)}><MapPin size={17} aria-hidden="true" />{t("เปลี่ยนพื้นที่ทำงาน", "Change workspace")}</Button>}
      <div className={styles.panes}>
        <nav className={styles.tree} data-mobile-hidden={!!workspace && !showAreas} aria-label={t("พื้นที่ทำงาน", "Workspaces")}>
          <label className={styles.search}><span>{t("ค้นหาพื้นที่ทำงาน", "Search workspaces")}</span><TextInput data-autofocus={!workspace || undefined} appearance="standard" type="search" value={areaQuery} onChange={event => setAreaQuery(event.target.value)} /></label>
          <ul>{holdings.map(holding => {
            const items = holding.companies.filter(item => matches(`${item.holding} ${item.label}`) || item.workspaces.some(area => matches(area.label)));
            if (!items.length) return null;
            return <li key={holding.id}><details open={!!areaQuery.trim() || items.some(item => item.id === company?.id)}><summary><Building2 size={17} aria-hidden="true" />{items[0].holding}</summary><ul>{items.map(item => {
              const allMatch = matches(`${item.holding} ${item.label}`);
              const areas = item.workspaces.filter(area => allMatch || matches(area.label));
              const root = areas.find(area => area.kind === "holding");
              const branches = [...new Set(areas.flatMap(area => area.branchId ? [area.branchId] : []))];
              return <li key={item.id}><details data-company-id={item.id} open={!!areaQuery.trim() || company?.id === item.id}><summary>{item.label}</summary><ul>
                {root && <li>{areaButton(item, root)}</li>}
                {areas.filter(area => area.kind === "department" && !area.branchId).map(area => <li key={area.id}>{areaButton(item, area)}</li>)}
                {branches.map(id => {
                  const children = areas.filter(area => area.branchId === id), branch = children.find(area => area.kind === "branch");
                  return <li key={id}><details open={!!areaQuery.trim() || company?.id === item.id && workspace?.branchId === id}><summary>{branch?.label ?? children[0].branchLabel ?? id.split("/").at(-1)}</summary><ul>
                    {branch && <li>{areaButton(item, branch)}</li>}
                    {children.filter(area => area.kind === "department").map(area => <li key={area.id}>{areaButton(item, area)}</li>)}
                  </ul></details></li>;
                })}
              </ul></details></li>;
            })}</ul></details></li>;
          })}</ul>
          {!companies.length && <p role="status">{t("ยังไม่มีพื้นที่ทำงานที่มีสิทธิ์เข้าใช้", "No workspaces are currently available.")}</p>}
          {companies.length > 0 && !holdings.some(holding => holding.companies.some(item => matches(`${item.holding} ${item.label}`) || item.workspaces.some(area => matches(area.label)))) && <p role="status">{t("ไม่พบพื้นที่ทำงาน", "No matching workspaces.")}</p>}
        </nav>
        <section className={styles.results} aria-label={t("แอปที่เข้าใช้ได้", "Available apps")}>
          <div aria-live="polite"><h2>{workspace?.label && workspace.kind !== "holding" ? workspace.label : company?.label ?? t("เลือกพื้นที่ทำงาน", "Choose a workspace")}</h2>{workspace?.kind !== "holding" && company && <p className={styles.hint}>{company.label}</p>}</div>
          {workspace && <>
            <label className={styles.search}><span>{t("ค้นหาแอปทั้งหมด", "Search all apps")}</span><TextInput appearance="standard" type="search" value={appQuery} onChange={event => setAppQuery(event.target.value)} /></label>
            <div className={styles.grid}>{apps.map(app => {
              const Icon = app.productId === "sales" ? ShoppingBag : app.productId === "apps" ? MessageCircle : Grid2X2;
              return <a data-product-switch data-app-id={app.productId} className={styles.app} href={app.href} key={app.productId} onClick={event => launch(event, app.productId)}>
                <Icon size={23} aria-hidden="true" /><ArrowUpRight className={styles.openIcon} size={17} aria-hidden="true" />
                <strong>{app.label}</strong>{app.accessLabel && <small>{app.accessLabel}</small>}
              </a>;
            })}</div>
            {!apps.length && <p role="status">{workspace.apps.length ? t("ไม่พบแอปที่ค้นหา", "No matching apps.") : t("ยังไม่มีแอปที่เข้าใช้ได้ในพื้นที่นี้", "No apps are available in this workspace.")}</p>}
            {(workspace.adminHref || workspace.managementHref) && <div className={styles.administration}>{[[workspace.adminHref, t("ดูแลบริษัท", "Company administration")], [workspace.managementHref, "ERPX Management"]].map(([href, label]) => href && <a key={href} data-product-switch href={href} onClick={() => remember()}><Settings size={18} aria-hidden="true" />{label}<ArrowUpRight size={16} aria-hidden="true" /></a>)}</div>}
          </>}
          {provider && <a data-product-switch href={provider.href}>{t("พื้นที่ผู้ให้บริการ", "Provider administration")} · {provider.role}</a>}
          {existingWorkspaces.map(item => <a key={item.href} data-product-switch href={item.href}>{item.holding}</a>)}
        </section>
      </div>
    </Dialog>
  </>;
}

/** Internal triggers open the picker without leaving the current document. */
export function OpenAppsButton({ language = "en" }: { language?: "en" | "th" }) {
  return <Button appearance="standard" variant="ghost" onClick={() => window.dispatchEvent(new Event("erpx:open-apps"))}>{language === "th" ? "แอปทั้งหมด" : "All apps"}</Button>;
}
