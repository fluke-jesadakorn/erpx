import { PageHeader, PreviewBanner } from "@/components/ui/primitives";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformPreviewPage() {
  await requirePlatformContext();
  return <main className="content" data-source="sample">
    <PageHeader eyebrow="Platform preview" title="Sample record" description="This preview contains no customer Holding data." />
    <PreviewBanner />
  </main>;
}
