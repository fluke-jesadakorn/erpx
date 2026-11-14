import { PageHeader } from "@erpx/ui/components/primitives";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import { SearchableTopicDirectory } from "@erpx/workplace/components/topic-navigation";

/** Presentation is owned explicitly by each route, including legacy backgrounds
 * underneath record drawers. This view does not receive an authority snapshot.
 */
export function DestinationIndexView({ title, links, appearance }: {
  title: string;
  links: readonly { href: string; label: string; description?: string }[];
  appearance: UiAppearance;
}) {
  return <div className="content-stack" data-source="live" data-appearance={appearance}>
    <PageHeader appearance={appearance} title={title} description="Choose a page in your current work scope." />
    <SearchableTopicDirectory label="Available pages" items={links.map(item=>({id:item.href,title:item.label,href:item.href,description:item.description}))}/>
    {!links.length && <p>No pages are available for your selected work role.</p>}
  </div>;
}
