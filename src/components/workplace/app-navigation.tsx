"use client";

import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { ChevronDown, Grid2X2, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useEffect, useId, useRef, useState, type ComponentType } from "react";
import { Dialog } from "@/components/ui/dialog";
import {
  navigationAccordionStorageKey,
  navigationLauncher,
  navigationRailStorageKey,
} from "@/lib/navigation-catalog";
import {
  navigationCategory,
  resolveNavigation,
  workplaceNavigation,
  type NavigationCategory,
  type NavigationContext,
  type NavigationIconId,
  type NavigationItem,
} from "@/lib/workplace-navigation";
import { useNavigationCounts } from "./navigation-counts";
import styles from "./shell.module.css";
import appStyles from "./app-navigation.module.css";

type IconComponent = ComponentType<{
  id: NavigationIconId;
  size?: number;
  strokeWidth?: number;
}>;

export function AppNavigation({ context, mobile = false, Icon }: {
  context: NavigationContext;
  mobile?: boolean;
  Icon: IconComponent;
}) {
  const base = context.workspaceBase;
  const model = workplaceNavigation(context);
  const pathname = usePathname();
  const query = useSearchParams();
  const active = resolveNavigation(model, base, pathname, query);
  const activeCategoryId = navigationCategory(model, active)?.id ?? null;
  const { counts, readHome } = useNavigationCounts();
  const [launcherOpen, setLauncherOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [rail, setRail] = useState(false);
  const [openCategoryId, setOpenCategoryId] = useState<string | null>(activeCategoryId);
  const navRef = useRef<HTMLElement>(null);
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
          activeCategoryId ??
            (saved && categoryIds.split(":").includes(saved) ? saved : null),
        );
        setRail(window.localStorage.getItem(railKey) === "true");
      } catch {
        setOpenCategoryId(activeCategoryId);
        setRail(false);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [accordionKey, railKey, activeCategoryId, location, categoryIds]);

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
    setOpenCategoryId(activeCategoryId);
    try {
      window.localStorage.setItem(railKey, String(next));
    } catch {
      // Navigation remains usable when storage is unavailable.
    }
  }

  function itemCurrent(item: NavigationItem) {
    return active.groupId === item.id || active.activeId === item.id;
  }

  function primaryLink(item: NavigationItem) {
    const count = counts[item.id as "home" | "work"];
    return (
      <Link
        key={item.id}
        href={item.href}
        aria-current={active.activeId === item.id ? "page" : undefined}
        aria-label={rail && !mobile ? item.label : undefined}
        title={rail && !mobile ? item.label : undefined}
        onClick={() => {
          if (item.id === "home") readHome();
          setOpenCategoryId(null);
        }}
      >
        <Icon id={item.icon} />
        <span>{item.label}</span>
        {["home", "work"].includes(item.id) && (
          <span
            className={styles.navBadge}
            title={item.id === "work" ? "My actions; – means incomplete" : "Unread updates"}
          >
            {count ?? "–"}
          </span>
        )}
      </Link>
    );
  }

  function appLink(item: NavigationItem, inFlyout = false) {
    return (
      <Link
        key={item.id}
        className={inFlyout ? appStyles.flyoutLink : styles.navAppLink}
        href={item.href}
        aria-current={itemCurrent(item) ? "page" : undefined}
        onClick={() => {
          if (rail) setOpenCategoryId(null);
        }}
      >
        <Icon id={item.icon} size={18} strokeWidth={1.8} />
        <span>{item.label}</span>
      </Link>
    );
  }

  function categorySection(category: NavigationCategory) {
    const expanded = openCategoryId === category.id;
    const activeCategory = activeCategoryId === category.id;
    const childrenId = `${treeId}-${category.id}`;
    return (
      <section
        className={rail ? appStyles.railCategory : styles.navCategory}
        data-active={activeCategory || undefined}
        key={category.id}
      >
        <button
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
        </button>
        {rail ? (
          expanded && (
            <div id={childrenId} className={appStyles.railFlyout} role="group" aria-label={category.label}>
              <strong>{category.label}</strong>
              {category.apps.map((item) => appLink(item, true))}
            </div>
          )
        ) : (
          <div id={childrenId} className={styles.navApps} hidden={!expanded}>
            {category.apps.map((item) => appLink(item))}
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
        key={`${item.id}:${item.href}`}
        aria-current={itemCurrent(item) ? "page" : undefined}
        onClick={() => setLauncherOpen(false)}
      >
        <Icon id={item.icon} />
        <span>
          <strong>{item.label}</strong>
          {contextLabel && <small>{contextLabel}</small>}
        </span>
      </Link>
    );
  }

  return (
    <>
      <nav
        ref={navRef}
        className={mobile ? styles.mobileNav : styles.nav}
        aria-label={mobile ? "Mobile navigation" : "Primary"}
        data-rail={!mobile && rail ? "true" : undefined}
      >
        {model.primary.map(primaryLink)}
        {!mobile && (
          <button
            type="button"
            className={appStyles.railToggle}
            onClick={toggleRail}
            aria-label={rail ? "Expand sidebar" : "Collapse sidebar"}
            title={rail ? "Expand sidebar" : "Collapse sidebar"}
          >
            {rail ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
            <span>{rail ? "Expand sidebar" : "Collapse sidebar"}</span>
          </button>
        )}
        {!mobile && <span className={styles.navLabel}>Apps by work</span>}
        {!mobile && model.categories.map(categorySection)}
        <button
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
        </button>
      </nav>

      <Dialog
        title="App launcher"
        open={launcherOpen}
        onClose={() => setLauncherOpen(false)}
        presentation="responsive-panel"
      >
        <div className={appStyles.content}>
          <input
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
                  {category.apps.map((item) => launcherRow(item))}
                </section>
              ))}
            </div>
          )}
        </div>
      </Dialog>
    </>
  );
}
