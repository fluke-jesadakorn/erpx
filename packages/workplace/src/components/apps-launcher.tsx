"use client";

import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import { ArrowUpRight, Building2, CalendarDays, ChevronDown, CircleHelp, ClipboardList, Factory, FileText, FolderKanban, GraduationCap, Grid2X2, Hotel, MapPin, MessageCircle, Package, Search, Settings, ShoppingBag, UsersRound, Wallet, Wrench } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { Dialog } from "@erpx/ui/components/dialog";
import { TextInput } from "@erpx/ui/components/input";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { scheduleWorkRefresh } from "@erpx/feature-work_projection/lib/work-refresh";
import { CompanyPluginCatalogue } from "./company-plugin-catalogue";
import { emptyAppsMemory, readAppsMemory, writeAppsMemory, restoreAppsSelection, rememberAppsLaunch, visibleApps, recentApps, saveFeedScroll, takeFeedScroll, type FeedScrollPosition, type AppsCompany, type AppsWorkspace, type AppsMemory } from "@erpx/feature-work_projection/lib/apps-launcher";
import styles from "./apps-launcher.module.css";

const launcherHash = "#apps";

function primaryLauncherTrigger() {
  return Array.from(document.querySelectorAll<HTMLButtonElement>("[data-apps-launcher-trigger]"))
    .find(button => button.getClientRects().length && !button.closest("[hidden]"));
}
const appIcons = {
  apps: MessageCircle,
  assets: Building2,
  booking: CalendarDays,
  sales: ShoppingBag,
  finance: Wallet,
  forms: ClipboardList,
  people: UsersRound,
  pms: Hotel,
  pos: ShoppingBag,
  projects: FolderKanban,
  manufacturing: Factory,
  service: Wrench,
  supply: Package,
  legal: FileText,
  learn: GraduationCap,
  support: CircleHelp,
};

function QuotationApprovalCount({ base, language }: { base: string; language: "en" | "th" }) {
  const [count, setCount] = useState<number | null>(null);
  useEffect(() => {
    let request: AbortController | undefined;
    let active = true;
    const refresh = async () => {
      if (!active || document.visibilityState !== "visible") return;
      request?.abort();
      const current = new AbortController();
      request = current;
      try {
        const response = await workFetch(`/api/v1${base}/commerce/pending-quotations`, {
          cache: "no-store",
          signal: current.signal,
        });
        const pending: unknown = response.ok ? (await response.json()).data?.pending : null;
        if (current.signal.aborted) return;
        if ([401, 403, 409].includes(response.status)) active = false;
        setCount(typeof pending === "number" && Number.isSafeInteger(pending) && pending >= 0 ? pending : null);
      } catch {
        if (!current.signal.aborted) setCount(null);
      }
    };
    const clearContext = () => {
      active = false;
      request?.abort();
      setCount(null);
    };
    void refresh();
    const stopRefresh = scheduleWorkRefresh(() => void refresh());
    const timer = setInterval(() => void refresh(), 15_000);
    window.addEventListener("erpx:context-switch-started", clearContext);
    window.addEventListener("erpx:work-context-changed", clearContext);
    return () => {
      request?.abort();
      clearInterval(timer);
      stopRefresh();
      window.removeEventListener("erpx:context-switch-started", clearContext);
      window.removeEventListener("erpx:work-context-changed", clearContext);
    };
  }, [base]);

  if (count === null) return null;
  return <span className={styles.quotationMetric} data-pending-quotations={count}>
    <span>{new Intl.NumberFormat(language).format(count)}</span>
    {language === "th" ? "ใบเสนอราคารออนุมัติ" : "Quotations awaiting approval"}
  </span>;
}

/** The tree previews choices. Only an authorized owner launch link changes context. */
export function AppsLauncher({ companies, accountId, language, provider, existingWorkspaces = [], currentCompanyId, currentWorkspaceId, currentProductId, productLabel, workspaceLabel, entry = false }: {
  companies: AppsCompany[]; accountId: string; language: "th" | "en";
  provider?: { href: string; role: string } | null;
  existingWorkspaces?: { holding: string; href: string }[];
  currentCompanyId?: string; currentWorkspaceId?: string; currentProductId?: string; productLabel?: string; workspaceLabel?: string; entry?: boolean;
}) {
  const th = language === "th", t = (thai: string, english: string) => th ? thai : english;
  const [memory, setMemory] = useState<AppsMemory>(emptyAppsMemory);
  const [open, setOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [areaQuery, setAreaQuery] = useState("");
  const [appQuery, setAppQuery] = useState("");
  const [showAreas, setShowAreas] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const grid = useRef<HTMLDivElement>(null);
  const initialized = useRef<string | null>(null);
  const launched = useRef(false);
  const feedScroll = useRef<FeedScrollPosition | null>(null);
  const flight = useRef<{ layer: HTMLElement; animation: Animation } | null>(null);
  const current = restoreAppsSelection(companies, memory);
  const company = companies.find(item => item.id === current.memory.companyId);
  const workspace = company?.workspaces.find(item => item.id === current.memory.workspaceId);

  const showHere = useCallback(() => {
    if (window.location.hash !== launcherHash) {
      feedScroll.current = { x: window.scrollX, y: window.scrollY };
      window.history.pushState({ erpxAppsLauncher: true }, "", launcherHash);
    }
    setOpen(true);
  }, []);

  useEffect(() => {
    if (initialized.current === accountId) return;
    initialized.current = accountId;
    let saved = emptyAppsMemory();
    try { saved = readAppsMemory(window.localStorage, accountId); } catch { /* Optional storage. */ }
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
    if (entry && !requested && !restored.invalid && feed) {
      // This href comes from the fresh server-authorized snapshot, never storage.
      window.location.assign(feed.href);
    } else if (entry || requested) setOpen(true);
  }, [accountId, companies, currentCompanyId, currentWorkspaceId, entry]);

  useEffect(() => {
    const sync = () => {
      flight.current?.animation.cancel();
      flight.current?.layer.remove();
      flight.current = null;
      launched.current = false;
      setOpen(window.location.hash === launcherHash && primaryLauncherTrigger() === trigger.current);
    };
    const show = () => { if (primaryLauncherTrigger() === trigger.current) showHere(); };
    const restore = (event: PageTransitionEvent) => { if (event.persisted) sync(); };
    window.addEventListener("erpx:open-apps", show);
    window.addEventListener("popstate", sync);
    window.addEventListener("pageshow", restore);
    return () => {
      window.removeEventListener("erpx:open-apps", show);
      window.removeEventListener("popstate", sync);
      window.removeEventListener("pageshow", restore);
      flight.current?.animation.cancel();
      flight.current?.layer.remove();
    };
  }, [showHere]);

  useEffect(() => {
    if (entry || currentProductId !== "apps" || !currentCompanyId || !currentWorkspaceId) return;
    const companyId = currentCompanyId, workspaceId = currentWorkspaceId;
    let stop: (() => void) | undefined;
    function restore() {
      if (window.location.hash || primaryLauncherTrigger() !== trigger.current) return;
      const route = window.location.pathname + window.location.search;
      let position: FeedScrollPosition | null;
      try { position = takeFeedScroll(window.sessionStorage, { accountId, companyId, workspaceId, route }); }
      catch { return; }
      if (!position) return;
      const { x, y } = position;
      const input = ["wheel", "touchstart", "pointerdown", "keydown"];
      const observer = new ResizeObserver(apply);
      const timeout = window.setTimeout(clear, 2000);
      function apply() {
        if (window.location.pathname + window.location.search !== route) return clear();
        if (Math.abs(window.scrollX - x) > 1 || Math.abs(window.scrollY - y) > 1)
          window.scrollTo({ left: x, top: y, behavior: "instant" });
      }
      function clear() {
        window.clearTimeout(timeout);
        observer.disconnect();
        window.removeEventListener("scroll", apply);
        for (const event of input) window.removeEventListener(event, clear);
      }
      stop?.();
      stop = clear;
      observer.observe(document.body);
      window.addEventListener("scroll", apply, { passive: true });
      for (const event of input) window.addEventListener(event, clear, { passive: true });
      apply();
    }
    restore();
    window.addEventListener("pageshow", restore);
    return () => {
      stop?.();
      window.removeEventListener("pageshow", restore);
    };
  }, [accountId, entry, currentCompanyId, currentWorkspaceId, currentProductId]);

  function close() {
    if (window.location.hash === launcherHash) {
      if (window.history.state?.erpxAppsLauncher) {
        window.history.back();
        return;
      }
      window.history.replaceState(null, "", window.location.pathname + window.location.search);
    }
    flight.current?.animation.cancel();
    flight.current?.layer.remove();
    flight.current = null;
    launched.current = false;
    setOpen(false);
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

  function launch(event: MouseEvent<HTMLAnchorElement>, productId?: string) {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    if (productId && productId === activeProduct) {
      close();
      return;
    }
    if (launched.current) return;
    if (productId) remember(productId);
    launched.current = true;
    const tile = event.currentTarget;
    const href = tile.href;
    const returningToPage = window.location.hash === launcherHash;
    const navigate = () => {
      if (currentProductId === "apps" && currentCompanyId && currentWorkspaceId && feedScroll.current) {
        try {
          saveFeedScroll(window.sessionStorage, {
            accountId, companyId: currentCompanyId, workspaceId: currentWorkspaceId,
            route: window.location.pathname + window.location.search,
          }, feedScroll.current);
        } catch { /* Optional storage. */ }
      }
      if (returningToPage) window.location.replace(href);
      else window.location.assign(href);
    };
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches || !tile.animate) {
      navigate();
      return;
    }
    const rect = tile.getBoundingClientRect();
    const layer = document.createElement("div");
    layer.className = styles.flight;
    layer.ariaHidden = "true";
    layer.dataset.appsLaunchTransition = "";
    const source = Array.from(grid.current?.children ?? []).find(child => child.getAttribute("data-app-id") === productId) ?? tile;
    const appearance = window.getComputedStyle(source);
    const sourceIsTile = source.hasAttribute("data-app-id") || source.hasAttribute("data-plugin-id");
    layer.style.backgroundColor = sourceIsTile ? appearance.backgroundColor : appearance.getPropertyValue("--launcher-base");
    layer.style.backgroundImage = sourceIsTile ? appearance.backgroundImage : "";
    layer.style.color = sourceIsTile ? appearance.color : "#fff";
    const artwork = document.createElement("div");
    artwork.className = styles.flightArtwork;
    const icon = source.querySelector("svg")?.cloneNode(true);
    if (icon) {
      const badge = document.createElement("span");
      badge.className = styles.appIcon;
      badge.append(icon);
      artwork.append(badge);
    }
    const label = document.createElement("strong");
    label.textContent = source.querySelector("strong")?.textContent ?? tile.textContent;
    artwork.append(label);
    layer.append(artwork);
    document.body.append(layer);
    const x = window.innerWidth, y = window.innerHeight;
    const tileShape = `inset(${rect.top}px ${x - rect.right}px ${y - rect.bottom}px ${rect.left}px round ${window.getComputedStyle(tile).borderRadius})`;
    const timing = { duration: 260, easing: "cubic-bezier(.22,.76,.2,1)", fill: "forwards" } as const;
    const animation = layer.animate([
      { clipPath: tileShape },
      { clipPath: "inset(0px round 0px)" },
    ], timing);
    artwork.animate([
      { transform: `translate(${rect.left + (rect.width - x) / 2}px, ${rect.top + (rect.height - y) / 2}px) scale(.75)`, opacity: 0 },
      { transform: "translate(0, 0) scale(1)", opacity: 1 },
    ], timing);
    flight.current = { layer, animation };
    animation.finished.then(() => { if (flight.current?.animation === animation) navigate(); }, () => {});
  }
  const matches = (text: string) => text.toLocaleLowerCase().includes(areaQuery.trim().toLocaleLowerCase());
  function areaButton(item: AppsCompany, area: AppsWorkspace) {
    const label = area.kind === "holding" ? item.label : area.branchLabel && area.label.startsWith(area.branchLabel + " / ") ? area.label.slice(area.branchLabel.length + 3) : area.label;
    return <button type="button" key={area.id} data-workspace-id={area.id} className={styles.area} aria-pressed={company?.id === item.id && workspace?.id === area.id} onClick={() => select(item, area)}><MapPin size={16} aria-hidden="true" /><span>{label}</span></button>;
  }
  const holdings = [...new Set(companies.map(item => item.holdingSlug ?? item.holding))].map(id => ({ id, companies: companies.filter(item => (item.holdingSlug ?? item.holding) === id) }));
  const activeProduct = workspace?.id === currentWorkspaceId && company?.id === currentCompanyId ? currentProductId : undefined;
  const history = company ? current.memory.recent[company.id] ?? [] : [];
  const apps = workspace ? visibleApps(workspace.apps, appQuery) : [];
  const featuredPair = apps.length === 2 && apps[0].productId === "apps" && apps[1].productId === "sales";
  const recent = workspace ? recentApps(workspace.apps, history, activeProduct) : [];
  return <>
    <Button ref={trigger} appearance="standard" variant="ghost" className={styles.trigger} data-apps-launcher-trigger aria-label={[t("แอป", "Apps"),workspaceLabel].filter(Boolean).join(" · ")} aria-haspopup="dialog" aria-expanded={open} onClick={showHere}>
      <Grid2X2 size={18} aria-hidden="true" /><span>{t("แอป", "Apps")}{workspaceLabel && <small>{productLabel && productLabel !== t("แอป", "Apps") ? `${productLabel} · ${workspaceLabel}` : workspaceLabel}</small>}</span><ChevronDown size={16} aria-hidden="true" />
    </Button>
    <Dialog appearance="standard" presentation="fullscreen" open={open} onClose={close} title={t("แอป", "Apps")} className={styles.modal} bodyClassName={styles.modalBody} finalFocus={() => trigger.current} closeLabel={t("ปิดแอป", "Close Apps")}>
      {(notice || current.invalid) && <InlineNotice tone="info">{t("พื้นที่ที่จำไว้ไม่อยู่ในสิทธิ์ปัจจุบันแล้ว กรุณาเลือกใหม่", "Your remembered workspace is no longer available. Choose again.")}</InlineNotice>}
      <div className={styles.panes} data-holding-slug={company?.holdingSlug}>
        {workspace && <Button className={styles.areaToggle} appearance="standard" variant="ghost" aria-expanded={showAreas} onClick={() => setShowAreas(!showAreas)}><MapPin size={16} aria-hidden="true" />{t("เปลี่ยนพื้นที่ทำงาน", "Change workspace")}<ChevronDown size={15} aria-hidden="true" /></Button>}
        <nav className={styles.tree} data-mobile-hidden={!!workspace && !showAreas} aria-label={t("พื้นที่ทำงาน", "Workspaces")}>
          <h2>{t("พื้นที่ทำงาน", "Workspaces")}</h2>
          <label className={styles.search}>
            <Search size={16} aria-hidden="true" />
            <TextInput aria-label={t("ค้นหาพื้นที่ทำงาน", "Search workspaces")} placeholder={t("ค้นหาพื้นที่ทำงาน", "Search workspaces")} data-autofocus={!workspace || undefined} appearance="standard" type="search" value={areaQuery} onChange={event => setAreaQuery(event.target.value)} />
          </label>
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
          <div className={styles.resultsScroll}>
          <div className={styles.resultsHeader}>
            <div aria-live="polite">
              {company && company.holding !== (workspace?.label ?? company.label) && <p className={styles.hint}><Building2 size={15} aria-hidden="true" />{company.holding}</p>}
              <h2>{workspace?.label && workspace.kind !== "holding" ? workspace.label : company?.label ?? t("เลือกพื้นที่ทำงาน", "Choose a workspace")}</h2>
            </div>
            {workspace && <label className={styles.search}>
              <Search size={17} aria-hidden="true" />
              <TextInput aria-label={t("ค้นหาแอปทั้งหมด", "Search all apps")} placeholder={t("ค้นหาแอปทั้งหมด", "Search all apps")} appearance="standard" type="search" value={appQuery} onChange={event => setAppQuery(event.target.value)} />
            </label>}
          </div>
          {workspace && <>
            <div className={styles.grid} data-featured-pair={featuredPair || undefined} ref={grid}>{apps.map(app => {
              const Icon = appIcons[app.productId as keyof typeof appIcons] ?? Grid2X2;
              const active = app.productId === activeProduct;
              const liveSales = open && !entry && app.productId === "sales" &&
                workspace.id === currentWorkspaceId && company?.id === currentCompanyId;
              return <a data-product-switch={active ? undefined : ""} aria-current={active ? "location" : undefined} data-app-id={app.productId} className={styles.app} href={app.href} key={app.productId} onClick={event => launch(event, app.productId)}>
                <span className={styles.appIcon}><Icon size={27} strokeWidth={1.65} aria-hidden="true" /></span><ArrowUpRight className={styles.openIcon} size={18} aria-hidden="true" />
                {liveSales && <QuotationApprovalCount base={workspace.id} language={language} />}
                <strong>{app.label}</strong>{app.accessLabel && <small>{app.accessLabel}</small>}
              </a>;
            })}</div>
            {!apps.length && <p className={styles.emptyApps} role="status">{workspace.apps.length ? t("ไม่พบแอปที่ค้นหา", "No matching apps.") : t("ยังไม่มีแอปที่เข้าใช้ได้ในพื้นที่นี้", "No apps are available in this workspace.")}</p>}
            {(workspace.adminHref || workspace.managementHref) && <div className={styles.administration}>{[[workspace.adminHref, t("ดูแลบริษัท", "Company administration")], [workspace.managementHref, "ERPX Management"]].map(([href, label]) => href && <a key={href} data-product-switch href={href} onClick={() => remember()}><Settings size={18} aria-hidden="true" />{label}<ArrowUpRight size={16} aria-hidden="true" /></a>)}</div>}
            {workspace.id === currentWorkspaceId && company && company.id === currentCompanyId && !entry && <CompanyPluginCatalogue
              key={`${company.id}:${workspace.id}`} api={`/api/v1${workspace.id}/plugins`} language={language} appQuery={appQuery} onLaunch={launch} />}
          </>}
          {provider && <a data-product-switch href={provider.href}>{t("พื้นที่ผู้ให้บริการ", "Provider administration")} · {provider.role}</a>}
          {existingWorkspaces.map(item => <a key={item.href} data-product-switch href={item.href}>{item.holding}</a>)}
          </div>
          {recent.length > 0 && <nav className={styles.recent} aria-label={t("สลับแอป", "Switch apps")}>
            {recent.map(app => {
              const Icon = appIcons[app.productId as keyof typeof appIcons] ?? Grid2X2;
              const active = app.productId === activeProduct;
              return <a data-product-switch={active ? undefined : ""} aria-current={active ? "location" : undefined} data-recent-app={app.productId} href={app.href} key={app.productId} onClick={event => launch(event, app.productId)}>
                <Icon size={19} strokeWidth={1.65} aria-hidden="true" /><span>{app.label}</span>
              </a>;
            })}
          </nav>}
        </section>
      </div>
    </Dialog>
  </>;
}

/** Internal triggers open the picker without leaving the current document. */
export function OpenAppsButton({ language = "en" }: { language?: "en" | "th" }) {
  return <Button appearance="standard" variant="ghost" onClick={() => window.dispatchEvent(new Event("erpx:open-apps"))}>{language === "th" ? "แอปทั้งหมด" : "All apps"}</Button>;
}
