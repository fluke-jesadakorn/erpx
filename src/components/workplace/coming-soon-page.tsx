import Link from 'next/link';
import type { PageCutEntry } from "@/lib/shared/hospitality-page-cut";
import { type UiAppearance } from "@/components/ui/foundation";

export function ComingSoonPage({ appearance = "legacy",entry,base}:({entry:PageCutEntry;base:string}) & { appearance?: UiAppearance }) {
  return <div data-ui-appearance={appearance} className={appearance === "standard" ? "ui-stack" : "content-stack"} style={{maxWidth:720,margin:'0 auto',padding:'32px 20px 80px'}} data-production-feature-cut={entry.id}>
    <p data-ui-appearance={appearance} style={{color:'var(--text-muted, #667085)',fontWeight:700}}>{entry.state}</p>
    <h1 data-ui-appearance={appearance}>{entry.name}</h1>
    <p data-ui-appearance={appearance}>This workflow is not active for this company’s hotel and restaurant pilot. It will appear here after its setup, independent acceptance, and company rollout are complete.</p>
    <p data-ui-appearance={appearance}><Link href={`${base}/future-features`}>Explore future features and prerequisites</Link></p>
  </div>;
}
