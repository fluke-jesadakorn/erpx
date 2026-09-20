import { FeaturePage, FeatureTabs,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader,PreviewBanner } from "@/components/ui/primitives";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformPreviewPage() {
  await requirePlatformContext();
  return <FeaturePage className="content" data-source="sample">
    <PageHeader eyebrow="Platform preview" title="Sample record" description="This preview contains no customer Holding data." /><FeatureTabs/><WidgetGrid>
    <PreviewBanner />
  </WidgetGrid></FeaturePage>;
}
