"use client";
import type { UiAppearance } from "@/components/ui/appearance";

import { Button, TextInput } from "@/components/ui/foundation";

import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import {
  ArrowUpRight,
  ChevronDown,
  CirclePlus,
  Grid2X2,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { useEffect, useId, useRef, useState, type ComponentType } from "react";
import { Dialog } from "@/components/ui/dialog";
import {
  navigationAccordionStorageKey,
  navigationLauncher,
  navigationRailStorageKey,
} from "@/lib/navigation-catalog";
import {
  navigationCategory,
  pmsSidebarHref,
  resolveNavigation,
  workplaceNavigation,
  type NavigationCategory,
  type NavigationContext,
  type NavigationIconId,
  type NavigationItem,
} from "@/lib/workplace-navigation";
import { useNavigationCounts } from "./navigation-counts";
import { NewWork } from "./new-work";
import { WorkShortcuts } from "./work-shortcuts";
import { StartActions } from "@/components/today/start-actions";
import { hasPermission } from "@/lib/permissions";
import styles from "./shell.module.css";
import appStyles from "./app-navigation.module.css";

type IconComponent = ComponentType<{
  id: NavigationIconId;
  size?: number;
  strokeWidth?: number;
}>;

export function AppNavigation({
  appearance = "legacy",
  context,
  mobile = false,
  Icon,
}: {
  appearance?: UiAppearance;
  context: NavigationContext;
  mobile?: boolean;
  Icon: IconComponent;
}) {
  const base = context.workspaceBase;
  const { counts, readHome, workViewAvailability } = useNavigationCounts();
  const model = workplaceNavigation({ ...context, workViewAvailability });
  const pathname = usePathname();
  const query = useSearchParams();
  const active = resolveNavigation(model, base, pathname, query);
  const activePrimary = model.primary.some((entry) => entry.id === active.activeId);
  const activeCategoryId = navigationCategory(model, active)?.id ?? null;
  const [launcherOpen, setLauncherOpen] = useState(false);
  const [workViewsOpen, setWorkViewsOpen] = useState(false);
  const [startOpen, setStartOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [rail, setRail] = useState(false);
  const [openCategoryId, setOpenCategoryId] = useState<string | null>(activeCategoryId);
  const [openPrimaryId, setOpenPrimaryId] = useState<string | null>(
    ["work", "inbox", "calendar"].includes(active.activeId) ? active.activeId : null,
  );
  const navRef = useRef<HTMLElement>(null);
  const railScrollRef = useRef<HTMLDivElement>(null);
  const flyoutRef = useRef<HTMLDivElement>(null);
  const activeLinkRef = useRef<HTMLAnchorElement>(null);
  const triggerRefs = useRef(new Map<string, HTMLButtonElement>());
  const treeId = useId().replaceAll(":", "");
  const location = `${pathname}?${query}`;
  const accordionKey = navigationAccordionStorageKey(base);
  const railKey = navigationRailStorageKey(base);
  const categoryIds = model.categories.map((entry) => entry.id).join(":");
  const launcher = navigationLauncher(model, search);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      try {
        const saved = window.localStorage.getItem(accordionKey);
        setOpenCategoryId(
          rail
            ? null
            : (activeCategoryId ??
                (activePrimary
                  ? null
                  : saved && categoryIds.split(":").includes(saved)
                    ? saved
                    : null)),
        );
        if (["work", "inbox", "calendar"].includes(active.activeId))
          setOpenPrimaryId(active.activeId);
      } catch {
        setOpenCategoryId(rail ? null : activeCategoryId);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [accordionKey, activeCategoryId, active.activeId, activePrimary, location, categoryIds, rail]);

  useEffect(() => {
    const wideScreen = window.matchMedia("(min-width: 1440px)");
    const syncRail = () => {
      try {
        setRail(!wideScreen.matches && window.localStorage.getItem(railKey) === "true");
      } catch {
        setRail(false);
      }
    };
    syncRail();
    wideScreen.addEventListener("change", syncRail);
    return () => wideScreen.removeEventListener("change", syncRail);
  }, [railKey]);

  useEffect(() => {
    if (!rail || !openCategoryId) return;
    const trigger = triggerRefs.current.get(openCategoryId);
    const scroller = railScrollRef.current;
    const flyout = flyoutRef.current;
    if (!trigger || !scroller || !flyout) return;
    const position = () => {
      const anchor = trigger.getBoundingClientRect();
      const bounds = scroller.getBoundingClientRect();
      if (anchor.bottom <= bounds.top || anchor.top >= bounds.bottom) {
        setOpenCategoryId(null);
        return;
      }
      flyout.style.top = `${Math.max(8, Math.min(anchor.top, window.innerHeight - flyout.offsetHeight - 8))}px`;
      flyout.style.left = `${Math.max(8, Math.min(anchor.right + 12, window.innerWidth - flyout.offsetWidth - 8))}px`;
      flyout.style.visibility = "visible";
    };
    const frame = window.requestAnimationFrame(() => {
      trigger.scrollIntoView({ block: "nearest" });
      position();
    });
    scroller.addEventListener("scroll", position, { passive: true });
    window.addEventListener("resize", position);
    return () => {
      window.cancelAnimationFrame(frame);
      scroller.removeEventListener("scroll", position);
      window.removeEventListener("resize", position);
      flyout.style.visibility = "hidden";
    };
  }, [rail, openCategoryId]);

  useEffect(() => {
    if (!rail || !openCategoryId) return;
    const closeOutside = (event: MouseEvent) => {
      if (!navRef.current?.contains(event.target as Node)) setOpenCategoryId(null);
    };
    const closeEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      const id = openCategoryId;
      setOpenCategoryId(null);
      triggerRefs.current.get(id)?.focus();
    };
    document.addEventListener("mousedown", closeOutside);
    document.addEventListener("keydown", closeEscape);
    return () => {
      document.removeEventListener("mousedown", closeOutside);
      document.removeEventListener("keydown", closeEscape);
    };
  }, [rail, openCategoryId]);

  useEffect(() => {
    if (rail || !openCategoryId) return;
    const frame = window.requestAnimationFrame(() =>
      activeLinkRef.current?.scrollIntoView({ block: "nearest" }),
    );
    return () => window.cancelAnimationFrame(frame);
  }, [rail, openCategoryId, location]);

  function toggleCategory(id: NavigationCategory["id"]) {
    const next = openCategoryId === id ? null : id;
    setOpenCategoryId(next);
    try {
      if (next) window.localStorage.setItem(accordionKey, next);
      else window.localStorage.removeItem(accordionKey);
    } catch {
      // Navigation remains usable when storage is unavailable.
    }
  }

  function toggleRail() {
    const next = !rail;
    setRail(next);
    setOpenCategoryId(next ? null : activeCategoryId);
    try {
      window.localStorage.setItem(railKey, String(next));
    } catch {
      // Navigation remains usable when storage is unavailable.
    }
  }

  function itemCurrent(item: NavigationItem) {
    if (item.id === "erpx-management") return active.activeId === item.id;
    return active.groupId === item.id || (active.sidebarId ?? active.activeId) === item.id;
  }

  function primaryViewHref(item: NavigationItem) {
    if (!item.id.startsWith("work-") || item.id === "work-reports") return item.href;
    const params = new URLSearchParams({ view: item.id.slice(5) });
    const type = query.get("type"), q = query.get("q");
    if (type && type !== "all") params.set("type", type);
    if (q) params.set("q", q);
    return `${base}/work?${params}`;
  }

  function primaryLink(item: NavigationItem) {
    const count = counts[item.id as "home" | "work"];
    const submodules = !mobile && !rail ? item.submodules : undefined;
    const link = (
      <Link
        key={item.id}
        href={pmsSidebarHref(item, pathname, query)}
        aria-current={
          active.activeId === item.id && (!item.submodules?.length || mobile || rail)
            ? "page"
            : undefined
        }
        aria-label={rail && !mobile ? item.label : undefined}
        title={rail && !mobile ? item.label : undefined}
        onClick={(event) => {
          if (item.id === "work" && (mobile || rail) && item.submodules?.length) {
            event.preventDefault();
            setWorkViewsOpen(true);
          }
          if (item.id === "home") readHome();
          setOpenCategoryId(null);
        }}
      >
        <Icon id={item.icon} />
        <span>{item.label}</span>
        {(item.id === "work" || (item.id === "home" && count !== 0)) && (
          <span
            className={styles.navBadge}
            title={item.id === "work" ? "My actions; – means incomplete" : "New team updates"}
          >
            {count ?? "–"}
          </span>
        )}
      </Link>
    );
    if (!submodules?.length) return link;
    const expanded = openPrimaryId === item.id;
    return (
      <div
        key={item.id}
        className={appStyles.primaryGroup}
        data-active={active.activeId === item.id || undefined}
      >
        <div className={appStyles.primaryHead}>
          {link}
          <Button appearance={appearance}
            variant={appearance === "standard" ? "ghost" : "plain"}
            type="button"
            aria-label={`${expanded ? "Collapse" : "Expand"} ${item.label} views`}
            aria-expanded={expanded}
            onClick={() => setOpenPrimaryId(expanded ? null : item.id)}
          >
            <ChevronDown size={16} aria-hidden="true" />
          </Button>
        </div>
        {expanded && (
          <div className={appStyles.primarySubmenu} aria-label={`${item.label} views`}>
            {submodules.map((submodule) => {
              const tab = query.get("tab");
              const current =
                item.id === "work"
                  ? (query.get("view") ?? "actions") === submodule.id.slice(5)
                  : item.id === "inbox"
                    ? (tab ?? "chats") === submodule.id.slice(6)
                    : query.get("preset") === "coverage"
                      ? submodule.id === "calendar-coverage"
                      : (query.get("mode") ?? "team") === "mine"
                        ? submodule.id === "calendar-mine"
                        : submodule.id === "calendar-team";
              return (
                <Link
                  key={submodule.id}
                  href={primaryViewHref(submodule)}
                  aria-current={pathname === item.href && current ? "page" : undefined}
                >
                  {submodule.label}
                </Link>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  function appLink(item: NavigationItem, inFlyout = false) {
    const current =
      item.id === "people-current"
        ? pathname === item.href &&
          query.get("status") !== "archived" &&
          query.get("view") !== "payroll"
        : item.id === "people-archived"
          ? pathname === item.href.split("?")[0] && query.get("status") === "archived"
          : item.id === "people-payroll"
            ? active.activeId === "people-payroll"
            : [
                  "people-payroll-history",
                  "people-payroll-advances",
                  "people-payroll-settings",
                ].includes(item.id)
              ? pathname === item.href
              : item.submodules?.length
                ? item.id !== "people" &&
                  active.groupId === item.id &&
                  active.activeId === model.apps.find((app) => app.id === item.id)?.items[0]?.id
                : itemCurrent(item);
    return (
      <Link
        key={item.id}
        ref={current ? activeLinkRef : undefined}
        className={inFlyout ? appStyles.flyoutLink : styles.navAppLink}
        href={pmsSidebarHref(item, pathname, query)}
        target={item.openInNewTab ? "_blank" : undefined}
        rel={item.openInNewTab ? "noopener noreferrer" : undefined}
        aria-current={current ? "page" : undefined}
        data-active={current || undefined}
        aria-label={item.openInNewTab ? `${item.label}, opens in a new tab` : undefined}
        onClick={() => {
          if (rail) setOpenCategoryId(null);
        }}
      >
        <Icon id={item.icon} size={18} strokeWidth={1.8} />
        <span>{item.label}</span>
        {item.openInNewTab && <ArrowUpRight size={14} aria-hidden="true" />}
      </Link>
    );
  }

  function categoryItems(category: NavigationCategory, inFlyout = false) {
    const links = category.apps.flatMap((item) => {
      if (!item.submodules?.length) return [appLink(item, inFlyout)];
      const app = model.apps.find((entry) => entry.id === item.id);
      const root = app?.items[0];
      if (!root) return [appLink(item, inFlyout)];

      const destinations = [
        { ...root, ...(item.id === "people" ? { label: "People" } : {}) },
        ...item.submodules.filter((entry) => entry.id !== "people-current" && entry.id !== root.id),
      ];
      return destinations.map((entry) => appLink(entry, inFlyout));
    });
    return [...links, ...(category.secondaryItems?.length ? [<details key={`${category.id}-more`} className={appStyles.moreNav} open={category.secondaryItems.some(item => item.id === active.activeId)}><summary>More</summary>{category.secondaryItems.map(item => appLink(item, inFlyout))}</details>] : [])];
  }

  function categorySection(category: NavigationCategory) {
    const expanded = openCategoryId === category.id;
    const childrenId = `${treeId}-${category.id}`;
    return (
      <section
        className={rail ? appStyles.railCategory : styles.navCategory}
        data-active={rail && !expanded && activeCategoryId === category.id ? true : undefined}
        key={category.id}
      >
        <Button appearance={appearance}
          variant={appearance === "standard" ? "ghost" : "plain"}
          ref={(node) => {
            if (node) triggerRefs.current.set(category.id, node);
            else triggerRefs.current.delete(category.id);
          }}
          type="button"
          className={rail ? appStyles.railCategoryButton : styles.navCategoryButton}
          aria-label={`${expanded ? "Collapse" : "Expand"} ${category.label}`}
          aria-expanded={expanded}
          aria-controls={childrenId}
          title={rail ? category.label : undefined}
          onClick={() => toggleCategory(category.id)}
        >
          <Icon id={category.icon} />
          <span>{category.label}</span>
          {!rail && <ChevronDown className={styles.navGroupChevron} size={16} aria-hidden="true" />}
        </Button>
        {rail ? (
          expanded && (
            <div
              ref={flyoutRef}
              id={childrenId}
              className={appStyles.railFlyout}
              role="group"
              aria-label={category.label}
            >
              <strong>{category.label}</strong>
              {categoryItems(category, true)}
            </div>
          )
        ) : (
          <div id={childrenId} className={styles.navApps} hidden={!expanded}>
            {categoryItems(category)}
          </div>
        )}
      </section>
    );
  }

  function launcherRow(item: NavigationItem, contextLabel?: string) {
    return (
      <Link
        className={appStyles.launcherRow}
        href={item.href}
        target={item.openInNewTab ? "_blank" : undefined}
        rel={item.openInNewTab ? "noopener noreferrer" : undefined}
        key={`${item.id}:${item.href}`}
        aria-current={itemCurrent(item) ? "page" : undefined}
        aria-label={item.openInNewTab ? `${item.label}, opens in a new tab` : undefined}
        onClick={() => setLauncherOpen(false)}
      >
        <Icon id={item.icon} />
        <span>
          <strong>{item.label}</strong>
          {contextLabel && <small>{contextLabel}</small>}
        </span>
        {item.openInNewTab && <ArrowUpRight size={14} aria-hidden="true" />}
      </Link>
    );
  }

  const entries = (
    <>
      {model.primary
        .filter((entry) => !mobile || entry.id !== "calendar")
        .flatMap((entry, index) =>
          mobile && base !== "/platform" && index === 2
            ? [
                <Button appearance={appearance}
                  variant={appearance === "standard" ? "ghost" : "plain"}
                  key="start"
                  type="button"
                  className={appStyles.mobileStart}
                  onClick={() => setStartOpen(true)}
                  aria-label="Start work"
                >
                  <CirclePlus size={24} aria-hidden="true" />
                  <span>Start</span>
                </Button>,
                primaryLink(entry),
              ]
            : [primaryLink(entry)],
        )}
      {!mobile && base !== "/platform" && (
        <Button appearance={appearance}
          variant={appearance === "standard" ? "ghost" : "plain"}
          type="button"
          className={appStyles.launcherButton}
          aria-label="Start"
          title={rail ? "Start" : undefined}
          onClick={() => setStartOpen(true)}
        >
          <CirclePlus size={18} aria-hidden="true" />
          <span>Start</span>
        </Button>
      )}
      {!mobile && !rail && base !== "/platform" && (
        <WorkShortcuts base={base} role={context.role} />
      )}
      {!mobile &&
        model.categories
          .filter((category) => category.id === "erpx-management")
          .map(categorySection)}
      {!mobile && <span className={styles.navLabel}>Apps by work</span>}
      {!mobile &&
        model.categories
          .filter((category) => category.id !== "erpx-management")
          .map(categorySection)}
    </>
  );

  return (
    <>
      <nav
        ref={navRef}
        className={mobile ? styles.mobileNav : `${styles.nav} ${appStyles.appNav}`}
        aria-label={mobile ? "Mobile navigation" : "Primary"}
        data-rail={!mobile && rail ? "true" : undefined}
      >
        {!mobile && rail ? (
          <div ref={railScrollRef} className={appStyles.railScroll}>
            {entries}
          </div>
        ) : (
          entries
        )}
        <Button appearance={appearance}
          variant={appearance === "standard" ? "ghost" : "plain"}
          type="button"
          className={mobile ? styles.moreButton : appStyles.launcherButton}
          onClick={() => setLauncherOpen(true)}
          aria-expanded={launcherOpen}
          aria-current={mobile && activeCategoryId ? "page" : undefined}
          aria-label="App launcher"
          title={rail && !mobile ? "App launcher" : undefined}
        >
          <Grid2X2 size={18} />
          <span>Apps</span>
        </Button>
        {!mobile && (
          <Button appearance={appearance}
            variant={appearance === "standard" ? "ghost" : "plain"}
            type="button"
            className={appStyles.railToggle}
            onClick={toggleRail}
            aria-label={rail ? "Expand sidebar" : "Collapse sidebar"}
            title={rail ? "Expand sidebar" : "Collapse sidebar"}
          >
            {rail ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
            <span>{rail ? "Expand sidebar" : "Collapse sidebar"}</span>
          </Button>
        )}
      </nav>

      <Dialog appearance={appearance} title="My work views" open={workViewsOpen} onClose={() => setWorkViewsOpen(false)} presentation="responsive-panel">
        <nav className={appStyles.content} aria-label="My work views">
          {model.primary.find(entry => entry.id === "work")?.submodules?.map(entry => (
            <Link key={entry.id} className={appStyles.launcherRow} href={primaryViewHref(entry)}
              aria-current={pathname === `${base}/work` && (query.get("view") ?? "actions") === entry.id.slice(5) ? "page" : undefined}
              onClick={() => setWorkViewsOpen(false)}><Icon id={entry.icon} /><span>{entry.label}</span></Link>
          ))}
        </nav>
      </Dialog>

      <Dialog appearance={appearance}
        title="App launcher"
        open={launcherOpen}
        onClose={() => setLauncherOpen(false)}
        presentation="responsive-panel"
      >
        <div className={appStyles.content}>
          {model.primary.some((entry) => entry.id === "calendar") && (
            <Link
              className={appStyles.launcherRow}
              href={`${base}/calendar`}
              onClick={() => setLauncherOpen(false)}
            >
              <Icon id="calendar" />
              <span>
                <strong>Calendar</strong>
                <small>Your time and team availability</small>
              </span>
            </Link>
          )}
          <TextInput appearance={appearance}
            className="input"
            type="search"
            data-autofocus
            aria-label="Search apps and pages"
            placeholder="Search apps and pages…"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          {search.trim() ? (
            <div className={appStyles.results} aria-live="polite">
              {launcher.results.map((result) => launcherRow(result.item, result.context))}
              {!launcher.results.length && <p>No matching apps or pages. Try another search.</p>}
            </div>
          ) : (
            <div className={appStyles.launcherCategories}>
              {launcher.categories.map((category) => (
                <section className={appStyles.launcherCategory} key={category.id}>
                  <h3>{category.label}</h3>
                  {category.apps.flatMap((item) => [
                    launcherRow(item),
                    ...(item.submodules ?? []).flatMap((submodule) => [
                      launcherRow(submodule, item.label),
                    ]),
                  ])}
                  {!!category.secondaryItems?.length && <details className={appStyles.moreNav}><summary>More</summary>{category.secondaryItems.map(item => launcherRow(item))}</details>}
                </section>
              ))}
            </div>
          )}
        </div>
      </Dialog>
      <NewWork key={pathname} base={base} creation={[]} current={pathname===`${base}/work`?query.toString():""} launcherOnly/>
      <Dialog appearance={appearance}
        title="Start work"
        open={startOpen}
        onClose={() => setStartOpen(false)}
        presentation="responsive-panel"
      >
        <StartActions
          base={base}
          compact
          canRequestLegal={
            hasPermission(
              { role: context.role ?? "", permissions: context.permissions },
              "legal.create",
            ) &&
            (!context.availableAppIds || context.availableAppIds.includes("legal"))
          }
          onNavigate={() => setStartOpen(false)}
        />
      </Dialog>
    </>
  );
}
