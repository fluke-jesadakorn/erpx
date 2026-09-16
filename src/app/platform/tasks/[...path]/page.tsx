import { PageHeader, PreviewBanner } from "@/components/ui/primitives";
import { requirePlatformContext } from "@/server/authorization";

export default async function PlatformTaskPage() {
  await requirePlatformContext();
  return (
    <main className="content" data-source="sample">
      <PageHeader
        eyebrow="Platform preview"
        title="Control-plane work item"
        description="Platform tasks remain isolated from every customer Holding."
      />
      <PreviewBanner />
    </main>
  );
}
