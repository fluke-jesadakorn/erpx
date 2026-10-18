import { PageHeader } from "@/components/ui/primitives";
import type { UiAppearance } from "@/components/ui/appearance";
import Link from "./workspace-link";

/** Presentation is owned explicitly by each route, including legacy backgrounds
 * underneath record drawers. This view does not receive an authority snapshot.
 */
export function DestinationIndexView({ title, links, appearance }: {
  title: string;
  links: readonly { href: string; label: string }[];
  appearance: UiAppearance;
}) {
  const standard = appearance === "standard";
  return <div className="content-stack" data-source="live" data-appearance={appearance}>
    <PageHeader appearance={appearance} title={title} description="Choose a page in your current work scope." />
    <nav aria-label="Available pages" className={standard ? "ui-stack" : "flex flex-col gap-4"}>
      {links.map(({ href, label }) => <Link key={href} href={href}
        className={standard ? "ui-record-item" : "block rounded-lg border border-[var(--ui-line)] bg-[var(--ui-card)] px-4 py-3"}>{label}</Link>)}
    </nav>
    {!links.length && <p>No pages are available for your selected work role.</p>}
  </div>;
}
