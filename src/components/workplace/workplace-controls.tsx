"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import Link from "next/link";
import { useEffect, useState, useSyncExternalStore } from "react";
import { usePathname } from "next/navigation";
import {
  Search,
  Sun,
  Moon,
  Monitor,
  ArrowUpRight,
  Home,
  BriefcaseBusiness,
  Grid2X2,
  BookOpen,
  Bell,
  Menu,
  UsersRound,
  Package,
  Landmark,
  Settings,
  HelpCircle,
  Folder,
  MessagesSquare,
} from "lucide-react";
import {
  workplaceNavigation,
  activeDestination,
  type NavigationContext,
  type NavigationItem,
} from "@/lib/workplace-navigation";
import { Dialog } from "@/components/ui/dialog";
import { ModuleIcon } from "./module-icon";
import styles from "./shell.module.css";
const icons = {
  home: Home,
  work: BriefcaseBusiness,
  inbox: Bell,
  people: UsersRound,
  commerce: Package,
  finance: Landmark,
  modules: Grid2X2,
  learn: BookOpen,
  settings: Settings,
  feedback: HelpCircle,
  projects: Folder,
  feed: MessagesSquare,
};
function NavigationIcon({ id }: { id: string }) {
  const Icon = icons[id as keyof typeof icons];
  return Icon ? <Icon size={18} /> : <ModuleIcon id={id} size={18} />;
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
export function Appearance({ initialTheme }: { initialTheme: string }) {
  const theme = useSyncExternalStore(
    subscribeAppearance,
    () => document.documentElement.getAttribute("data-theme") ?? "light",
    () => initialTheme,
  );
  return (
    <div className={styles.appearance} role="group" aria-label="Appearance">
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
          aria-pressed={theme === value}
          onClick={() => writeAppearance(value)}
        >
          <Icon size={13} />
          {label}
        </button>
      ))}
    </div>
  );
}

export function WorkplaceNavigation(
  props: NavigationContext & { mobile?: boolean; theme?: string },
) {
  const { workspaceBase, mobile = false } = props;
  const ready = useHydrated();
  const pathname = usePathname(),
    [open, setOpen] = useState(false);
  const { primary, more } = workplaceNavigation(props),
    active = activeDestination(workspaceBase, pathname);
  const link = (item: NavigationItem) => (
    <Link
      key={item.id}
      href={item.href}
      aria-current={item.id === active ? "page" : undefined}
      title={item.label}
      onClick={() => setOpen(false)}
    >
      <NavigationIcon id={item.icon} />
      <span>{item.label}</span>
    </Link>
  );
  return (
    <>
      <nav
        className={mobile ? styles.mobileNav : styles.nav}
        aria-label={mobile ? "Mobile navigation" : "Primary"}
      >
        {primary.map(link)}
        <button
          type="button"
          className={styles.moreButton}
          disabled={!ready}
          onClick={() => setOpen(true)}
          aria-expanded={open}
          aria-current={active === "more" ? "page" : undefined}
          aria-label="More navigation"
        >
          <Menu size={18} />
          <span>More</span>
        </button>
      </nav>
      <Dialog title="More" open={open} onClose={() => setOpen(false)}>
        <div className={styles.menuLinks}>{more.map(link)}</div>
      </Dialog>
    </>
  );
}
export function CommandSearch(props: NavigationContext) {
  const [open, setOpen] = useState(false),
    [query, setQuery] = useState("");
  const ready = useSyncExternalStore(
    subscribeHydration,
    () => true,
    () => false,
  );
  useEffect(() => {
    const handle = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setQuery("");
        setOpen((value) => !value);
      }
    };
    document.addEventListener("keydown", handle);
    return () => document.removeEventListener("keydown", handle);
  }, []);
  const results = workplaceNavigation(props)
    .search.filter((item) =>
      `${item.label} ${item.group}`.toLowerCase().includes(query.toLowerCase()),
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
        <span>Find a tool</span>
        <kbd>⌘ K</kbd>
      </button>
      <Dialog title="Go to…" open={open} onClose={() => setOpen(false)}>
        <div className={styles.commandSearch}>
          <Search size={17} />
          <input
            data-autofocus
            className="input"
            aria-label="Search destinations and capabilities"
            placeholder="Find a page or tool…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className={styles.commandResults}>
          {results.map((item) => (
            <Link key={item.id} href={item.href} onClick={() => setOpen(false)}>
              <NavigationIcon id={item.icon} />
              <span>
                <strong>{item.label}</strong>
                <small>{item.group}</small>
              </span>
              <ArrowUpRight size={14} />
            </Link>
          ))}
          {!results.length && <p>No matching tools in this workspace.</p>}
        </div>
      </Dialog>
    </>
  );
}
