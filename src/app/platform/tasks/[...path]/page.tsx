import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader,PreviewBanner } from "@/components/ui/primitives";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformTaskPage() {
  await requirePlatformContext();
  return (
    <FeaturePage className="content" data-source="sample">
      <PageHeader
        eyebrow="Platform preview"
        title="Control-plane work item"
        description="Platform tasks remain isolated from every customer Holding."
      /><FeatureTabs/><WidgetGrid pageKey="platform-tasks-overview">
      <FeatureWidget id="platform-tasks-overview-preview" title="Preview"><PreviewBanner /></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
