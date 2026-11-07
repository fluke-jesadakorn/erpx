"use client";
import { canonicalUiHref,workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";
import type { UiAppearance } from "@erpx/ui/components/appearance";

import { SidebarIcon, SidebarSection, sidebarItemClassName } from "@erpx/ui/components/sidebar-navigation";
import { sidebarCategoryTone, sidebarCategoryItems, sidebarSelection } from "@erpx/feature-work_projection/lib/sidebar-catalog";
import { Button, TextInput } from "@erpx/ui/components/foundation";

import Link from "@erpx/workplace/components/workspace-link";
import {useProductNavigationScope,useProductPathname} from './product-link-context';
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import {
  ArrowUpRight,
  ChevronDown,
  CirclePlus,
  Grid2X2,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { Suspense, useEffect, useId, useRef, useState, type ComponentType } from "react";
import { Dialog } from "@erpx/ui/components/dialog";
import {
  navigationAccordionStorageKey,
  navigationLauncher,
  navigationRailStorageKey,
} from "@erpx/feature-work_projection/lib/navigation-catalog";
import {
  navigationCategory,
  pmsSidebarHref,
  resolveNavigation,
  workplaceNavigation,
  type NavigationCategory,
  type NavigationContext,
  type NavigationIconId,
  type NavigationItem,
} from "@erpx/feature-work_projection/lib/workplace-navigation";
import { NavigationBadge } from "@erpx/workplace/components/navigation-badge";
import { useNavigationState } from "@erpx/workplace/components/navigation-counts";
import { NewWork } from "@erpx/workplace/components/new-work";
import { StartActions } from "@erpx/feature-work_projection/components/today/start-actions";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import styles from "@erpx/workplace/components/shell.module.css";
import appStyles from "@erpx/workplace/components/app-navigation.module.css";

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
  const { readHome, workViewAvailability } = useNavigationState();
  const model = workplaceNavigation({ ...context, workViewAvailability });
  const visiblePath=useProductPathname(),productScope=useProductNavigationScope(),parsedPath=workspacePathParts(visiblePath,productScope.holdingSlug);
  const pathname = parsedPath?`${parsedPath.base}/${parsedPath.path}`:visiblePath;
  const query = useSearchParams();
  const active = resolveNavigation(model, base, pathname, query);
  const activePrimary = model.primary.some((entry) => entry.id === active.activeId);
  const sidebar = context.productId ? sidebarSelection(model, active, pathname) : null;
  const activeCategoryId = sidebar?.categoryId ?? navigationCategory(model, active)?.id ?? null;
  const [launcherOpen, setLauncherOpen] = useState(false);
  const [startOpen, setStartOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [rail, setRail] = useState(false);
  const [openCategoryId, setOpenCategoryId] = useState<string | null>(activeCategoryId);
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

  function primaryLink(item: NavigationItem) {
    const link = (
      <Link
        key={item.id}
        className={sidebarItemClassName}
        href={pmsSidebarHref(item, pathname, query)}
        aria-current={
          active.activeId === item.id
            ? "page"
            : undefined
        }
        aria-label={rail && !mobile ? item.label : undefined}
        title={rail && !mobile ? item.label : undefined}
        onClick={() => {
          if (item.id === "home") readHome();
          setOpenCategoryId(null);
        }}
      >
        <SidebarIcon tone={sidebarCategoryTone(item.categoryId)}><Icon id={item.icon} /></SidebarIcon>
        <span data-nav-label>{item.label}</span>
        {(item.id === "work" || item.id === "home") && (
          <Suspense fallback={<span className={styles.navBadge}>–</span>}>
            <NavigationBadge kind={item.id} />
          </Suspense>
        )}
      </Link>
    );
    return link;
  }

  function appLink(item: NavigationItem, inFlyout = false) {
    const current = (sidebar?.itemId ?? active.sidebarId ?? active.activeId) === item.id;
    return (
      <Link
        key={item.id}
        ref={current ? activeLinkRef : undefined}
        className={`${sidebarItemClassName} ${inFlyout ? appStyles.flyoutLink : styles.navAppLink}`}
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
        <SidebarIcon tone={sidebarCategoryTone(item.categoryId)}><Icon id={item.icon} size={18} strokeWidth={1.8} /></SidebarIcon>
        <span data-nav-label>{item.label}</span>
        {item.openInNewTab && <ArrowUpRight size={14} aria-hidden="true" />}
      </Link>
    );
  }

  function categoryItems(category: NavigationCategory, inFlyout = false) {
    return sidebarCategoryItems(model, category).map(item => appLink(item, inFlyout));
  }

  function categorySection(category: NavigationCategory) {
    const expanded = openCategoryId === category.id;
    const childrenId = `${treeId}-${category.id}`;
    return (
      <section
        className={rail ? appStyles.railCategory : styles.navCategory}
        data-active={activeCategoryId === category.id || undefined}
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
          <SidebarIcon tone={sidebarCategoryTone(category.id)}><Icon id={category.icon} /></SidebarIcon>
          <span data-nav-label>{category.label}</span>
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
        className={`${sidebarItemClassName} ${appStyles.launcherRow}`}
        href={item.href}
        target={item.openInNewTab ? "_blank" : undefined}
        rel={item.openInNewTab ? "noopener noreferrer" : undefined}
        key={`${item.id}:${item.href}`}
        aria-current={itemCurrent(item) ? "page" : undefined}
        aria-label={item.openInNewTab ? `${item.label}, opens in a new tab` : undefined}
        onClick={() => setLauncherOpen(false)}
      >
        <SidebarIcon tone={sidebarCategoryTone(item.categoryId)}><Icon id={item.icon} /></SidebarIcon>
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
      {!mobile && base !== "/platform" && (!context.productId || context.productId === "apps") && (
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
          onClick={() => { if (!openCategoryId) setOpenCategoryId(activeCategoryId ?? model.categories[0]?.id ?? null); setLauncherOpen(true); }}
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
              href={canonicalUiHref(`${base}/calendar`)}
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
                <SidebarSection key={category.id} label={category.label} icon={<Icon id={category.icon}/>}
                  tone={sidebarCategoryTone(category.id)} open={openCategoryId === category.id}
                  active={activeCategoryId === category.id} onToggle={() => toggleCategory(category.id)}>
                  {category.apps.map((item) => launcherRow(item))}
                  {(category.secondaryItems ?? []).map((item) => launcherRow(item))}
                </SidebarSection>
              ))}
            </div>
          )}
        </div>
      </Dialog>
      <NewWork key={pathname} base={base} creation={[]} current={pathname===canonicalUiHref(`${base}/work`)?query.toString():""} launcherOnly/>
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
