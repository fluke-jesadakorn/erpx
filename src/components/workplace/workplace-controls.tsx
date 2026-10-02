"use client";

import { AppNavigation } from "./app-navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import Link from "next/link";
import {
  useCallback,
  useEffect,
  useId,
  useState,
  useSyncExternalStore,
} from "react";
import { usePathname, useSearchParams } from "next/navigation";
import {
  ArrowLeftRight,
  ArrowUpRight,
  Banknote,
  CalendarDays,
  BookOpenText,
  BookOpen,
  Bot,
  BriefcaseBusiness,
  Building2,
  Boxes,
  ChevronDown,
  ClipboardCheck,
  CreditCard,
  FileText,
  Folder,
  GraduationCap,
  Handshake,
  HelpCircle,
  Home,
  KeyRound,
  Landmark,
  ListTodo,
  Menu,
  MessagesSquare,
  Monitor,
  Moon,
  Package,
  PackageSearch,
  ReceiptText,
  RefreshCw,
  Repeat2,
  Search,
  Scale,
  Settings,
  ShieldCheck,
  ShoppingCart,
  Sun,
  type LucideIcon,
  Users,
  UsersRound,
  WalletCards,
  Warehouse,
  Workflow,
} from "lucide-react";
import {
  resolveNavigation,
  navigationCollapsedValue,
  workplaceNavigation,
  type NavigationContext,
  type NavigationApp,
  type NavigationIconId,
  type NavigationItem,
} from "@/lib/workplace-navigation";
import { Dialog } from "@/components/ui/dialog";
import { useNavigationCounts } from "./navigation-counts";
import styles from "./shell.module.css";

const icons: Record<NavigationIconId, LucideIcon> = {
  home: Home,
  work: BriefcaseBusiness,
  inbox: MessagesSquare,
  calendar: CalendarDays,
  people: UsersRound,
  commerce: Package,
  finance: Landmark,
  pms: Building2,
  learn: BookOpen,
  settings: Settings,
  feedback: HelpCircle,
  projects: Folder,
  technology: Monitor,
  feed: MessagesSquare,
  organization: Building2,
  payroll: Banknote,
  access: ShieldCheck,
  quotation: FileText,
  purchasing: ShoppingCart,
  stock: Boxes,
  "stock-count": ClipboardCheck,
  "stock-transfer": ArrowLeftRight,
  product: PackageSearch,
  warehouse: Warehouse,
  parties: Handshake,
  ledger: Scale,
  invoice: ReceiptText,
  bank: RefreshCw,
  expense: WalletCards,
  "staff-task": ListTodo,
  routine: Repeat2,
  handbook: BookOpenText,
  education: GraduationCap,
  "admin-people": Users,
  "admin-organization": Building2,
  "admin-approvals": Workflow,
  "admin-usage": CreditCard,
  "admin-ai": Bot,
  "admin-iam": KeyRound,
};

function NavigationIcon({
  id,
  size = 18,
  strokeWidth = 2,
}: {
  id: NavigationIconId;
  size?: number;
  strokeWidth?: number;
}) {
  const Icon = icons[id];
  return <Icon size={size} strokeWidth={strokeWidth} aria-hidden="true" />;
}

function writeAppearance(value: string) {
  document.documentElement.setAttribute("data-theme", value);
  document.cookie = `erpx_appearance=${value}; Path=/; Max-Age=31536000; SameSite=Lax`;
  window.dispatchEvent(new Event("erpx-appearance"));
}

function subscribeAppearance(callback: () => void) {
  window.addEventListener("erpx-appearance", callback);
  return () => window.removeEventListener("erpx-appearance", callback);
}

const subscribeHydration = () => () => {};

export function Appearance({
  initialTheme,
  compact = false,
}: {
  initialTheme: string;
  compact?: boolean;
}) {
  const theme = useSyncExternalStore(
    subscribeAppearance,
    () => document.documentElement.getAttribute("data-theme") ?? "system",
    () => initialTheme,
  );
  return (
    <div
      className={`${styles.appearance} ${compact ? styles.appearanceCompact : ""}`}
      role="group"
      aria-label="Appearance"
    >
      {(
        [
          ["light", "Light", Sun],
          ["dark", "Dark", Moon],
          ["system", "System", Monitor],
        ] as const
      ).map(([value, label, Icon]) => (
        <button
          type="button"
          key={value}
          aria-label={label}
          title={label}
          aria-pressed={theme === value}
          onClick={() => writeAppearance(value)}
        >
          <Icon size={13} />
          {!compact && label}
        </button>
      ))}
    </div>
  );
}

function readCollapsed(value: string) {
  try {
    const parsed = JSON.parse(value);
    return new Set<string>(
      Array.isArray(parsed)
        ? parsed.filter((entry): entry is string => typeof entry === "string")
        : [],
    );
  } catch {
    return new Set<string>();
  }
}

export function WorkplaceNavigation(props: NavigationContext & { mobile?: boolean; theme?: string }) {
  return props.scopeKind === "platform" || props.workspaceBase === "/platform"
    ? <LegacyWorkplaceNavigation {...props}/>
    : <AppNavigation context={props} mobile={props.mobile} Icon={NavigationIcon}/>;
}

function LegacyWorkplaceNavigation(
  props: NavigationContext & { mobile?: boolean; theme?: string },
) {
  const { workspaceBase, mobile = false } = props;
  const { counts, readHome } = useNavigationCounts();
  const ready = useHydrated();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const model = workplaceNavigation(props);
  const resolved = resolveNavigation(
    model,
    workspaceBase,
    pathname,
    searchParams,
  );
  const business = model.apps.filter((group) => group.placement === "business");
  const secondary = model.apps.filter(
    (group) => group.placement === "secondary",
  );
  const standard = !props.beta && props.scopeKind !== "platform" && workspaceBase !== "/platform";
  const legacyStorageKey = `erpx-navigation-collapsed:v2:${workspaceBase}`;
  const storageKey = standard ? `erpx-navigation-collapsed:v3:${workspaceBase}` : legacyStorageKey;
  const defaultCollapsed = JSON.stringify(model.apps.map((group) => group.id));
  const location = `${pathname}?${searchParams}`;
  const [activeCollapse, setActiveCollapse] = useState<string | null>(null);
  const [mobileOpen, setMobileOpen] = useState(false);
  const treeId = useId().replaceAll(":", "");
  const subscribeCollapsed = useCallback(
    (callback: () => void) => {
      const eventName = `erpx-navigation:${storageKey}`;
      const storage = (event: StorageEvent) => {
        if (event.key === storageKey) callback();
      };
      window.addEventListener(eventName, callback);
      window.addEventListener("storage", storage);
      return () => {
        window.removeEventListener(eventName, callback);
        window.removeEventListener("storage", storage);
      };
    },
    [storageKey],
  );
  const collapsedValue = useSyncExternalStore(
    subscribeCollapsed,
    () => {
      try {
        const saved = window.localStorage.getItem(storageKey);
        return standard
          ? navigationCollapsedValue(saved, window.localStorage.getItem(legacyStorageKey), model.apps.map((group) => group.id))
          : saved ?? defaultCollapsed;
      } catch { return defaultCollapsed; }
    },
    () => defaultCollapsed,
  );
  const collapsed = readCollapsed(collapsedValue);

  const persistCollapsed = (next: Set<string>) => {
    try {
      window.localStorage.setItem(storageKey, JSON.stringify([...next]));
      window.dispatchEvent(new Event(`erpx-navigation:${storageKey}`));
    } catch {
      // Browser privacy settings can disable local storage; navigation still works.
    }
  };
  const expanded = (id: string) =>
    (resolved.groupId === id && activeCollapse !== `${location}:${id}`) ||
    !collapsed.has(id);
  const toggleExpanded = (id: string) => {
    const next = new Set(collapsed);
    const open = expanded(id);
    if (open) next.add(id);
    else next.delete(id);
    if (resolved.groupId === id) setActiveCollapse(open ? `${location}:${id}` : null);
    persistCollapsed(next);
  };

  const closeMobile = () => {
    if (mobile) setMobileOpen(false);
  };
  const link = (item: NavigationItem, child = false) => {
    const count = ready ? counts[item.id as "home" | "work"] : null;
    return (
      <Link
        key={item.id}
        className={child ? styles.navChildLink : undefined}
        href={item.href}
        target={item.openInNewTab ? "_blank" : undefined}
        rel={item.openInNewTab ? "noopener noreferrer" : undefined}
        aria-current={item.id === resolved.activeId ? "page" : undefined}
        title={item.label}
        aria-label={item.openInNewTab ? `${item.label}, opens in a new tab` : undefined}
        onClick={() => {
          closeMobile();
          if (item.id === "home") readHome();
        }}
      >
        {child ? (
          <span className={styles.navChildIcon}>
            <NavigationIcon id={item.icon} size={15} strokeWidth={1.7} />
          </span>
        ) : (
          <NavigationIcon id={item.icon} />
        )}
        <span>{item.label}</span>
        {item.openInNewTab && <ArrowUpRight size={14} aria-hidden="true" />}
        {(item.id === "home" || item.id === "work") &&
          workspaceBase !== "/platform" && (
            <span
              className={styles.navBadge}
              title={
                item.id === "home"
                  ? "Unread team updates"
                  : "Unfinished work, including waiting"
              }
              aria-label={
                count === null
                  ? "Count unavailable"
                  : `${count} ${item.id === "home" ? "unread updates" : "unfinished work items"}`
              }
            >
              {count ?? "–"}
            </span>
          )}
      </Link>
    );
  };
  const renderGroup = (group: NavigationApp) => {
    const open = expanded(group.id);
    const childrenId = `${treeId}-${group.id}`;
    const groupActive = resolved.groupId === group.id;
    return (
      <section
        className={styles.navGroup}
        data-active={groupActive || undefined}
        key={group.id}
      >
        <div className={styles.navGroupHead}>
          <button
            type="button"
            className={styles.navGroupLabel}
            onClick={() => toggleExpanded(group.id)}
            aria-label={`${open ? "Collapse" : "Expand"} ${group.label}`}
            aria-expanded={open}
            aria-controls={childrenId}
          >
            <NavigationIcon id={group.icon} />
            <span>{group.label}</span>
            <ChevronDown className={styles.navGroupChevron} size={15} aria-hidden="true" />
          </button>
        </div>
        <div
          id={childrenId}
          className={styles.navChildren}
          data-expanded={open}
          hidden={!open}
        >
          {group.items.map((item) => link(item, true))}
        </div>
      </section>
    );
  };

  const secondaryContent = (
    <>
      {secondary.map(renderGroup)}
      {model.standalone.map((item) => link(item))}
    </>
  );

  return (
    <>
      <nav
        className={mobile ? styles.mobileNav : styles.nav}
        aria-label={mobile ? "Mobile navigation" : "Primary"}
      >
        {model.primary.map((item) => link(item))}
        {!mobile && !!business.length && (
          <span className={styles.navLabel}>Business</span>
        )}
        {!mobile && business.map(renderGroup)}
        {!mobile && (secondary.length > 0 || model.standalone.length > 0) && (
          <div className={styles.secondaryNav} data-expanded="true">
            {!standard && <span className={styles.navLabel}>Workspace</span>}
            {secondaryContent}
          </div>
        )}
        {mobile && (
          <button
            type="button"
            className={styles.moreButton}
            disabled={!ready}
            onClick={() => setMobileOpen(true)}
            aria-expanded={mobileOpen}
            aria-current={
              resolved.groupId ||
              model.standalone.some((item) => item.id === resolved.activeId)
                ? "page"
                : undefined
            }
            aria-label="More navigation"
          >
            <Menu size={18} aria-hidden="true" />
            <span>More</span>
          </button>
        )}
      </nav>
      {mobile && (
        <Dialog
          title="More"
          open={mobileOpen}
          onClose={() => setMobileOpen(false)}
          presentation="responsive-panel"
        >
          <nav className={styles.mobileTree} aria-label="More destinations">
            {business.map(renderGroup)}
            {secondaryContent}
          </nav>
        </Dialog>
      )}
    </>
  );
}

export function CommandSearch(props: NavigationContext) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const ready = useSyncExternalStore(
    subscribeHydration,
    () => true,
    () => false,
  );
  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setQuery("");
        setOpen((value) => !value);
      }
    };
    document.addEventListener("keydown", handle);
    return () => document.removeEventListener("keydown", handle);
  }, []);
  const normalized = query.trim().toLowerCase();
  const results = workplaceNavigation(props)
    .search.filter((item) =>
      `${item.label} ${item.group} ${(item.keywords ?? []).join(" ")}`
        .toLowerCase()
        .includes(normalized),
    )
    .slice(0, 40);
  return (
    <>
      <button
        type="button"
        className={styles.searchButton}
        onClick={() => {
          setQuery("");
          setOpen(true);
        }}
        aria-label="Search workspace"
        disabled={!ready}
      >
        <Search size={16} />
        <span>Go to</span>
        <kbd>⌘ K</kbd>
      </button>
      <Dialog title="Go to…" open={open} onClose={() => setOpen(false)}>
        <div className={styles.commandSearch}>
          <Search size={17} />
          <input
            data-autofocus
            className="input"
            aria-label="Search workspace destinations"
            placeholder="Find a page…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <div className={styles.commandResults}>
          {results.map((item) => (
            <Link key={item.id} href={item.href} target={item.openInNewTab ? "_blank" : undefined} rel={item.openInNewTab ? "noopener noreferrer" : undefined} aria-label={item.openInNewTab ? `${item.label}, opens in a new tab` : undefined} onClick={() => setOpen(false)}>
              <NavigationIcon id={item.icon} />
              <span>
                <strong>{item.label}</strong>
                <small>{item.group}</small>
              </span>
              <ArrowUpRight size={14} aria-hidden="true" />
            </Link>
          ))}
          {!results.length && <p>No matching pages in this workspace.</p>}
        </div>
      </Dialog>
    </>
  );
}
