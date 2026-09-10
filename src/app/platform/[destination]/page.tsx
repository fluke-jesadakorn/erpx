import { notFound } from "next/navigation";
import { PageHeader, PreviewBanner } from "@/components/ui/primitives";

const destinations: Record<string, [string, string]> = {
  home: ["Platform overview", "Control-plane operations are separated from every customer Holding."],
  work: ["Platform work", "Control-plane tasks appear here without granting access to customer data."],
  feed: ["Platform updates", "Operational announcements for Platform administrators."],
  inbox: ["Platform inbox", "Control-plane notifications remain isolated from Holding activity."],
  learn: ["Platform learning", "Guidance for safe control-plane administration."],
};

export default async function PlatformDestinationPage({ params }: PageProps<"/platform/[destination]">) {
  const { destination } = await params;
  const content = destinations[destination];
  if (!content) notFound();
  return <main className="content" data-source="sample">
    <PageHeader eyebrow="Platform" title={content[0]} description={content[1]} />
    <PreviewBanner />
  </main>;
}
