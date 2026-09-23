import Link from 'next/link';
import type { PageCutEntry } from '@/lib/hospitality-page-cut';

export function ComingSoonPage({entry,base}:{entry:PageCutEntry;base:string}) {
  return <main className="content-stack" style={{maxWidth:720,margin:'0 auto',padding:'32px 20px 80px'}} data-production-feature-cut={entry.id}>
    <p style={{color:'var(--text-muted, #667085)',fontWeight:700}}>{entry.state}</p>
    <h1>{entry.name}</h1>
    <p>This workflow is not active for this company’s hotel and restaurant pilot. It will appear here after its setup, independent acceptance, and company rollout are complete.</p>
    <p><Link href={`${base}/future-features`}>Explore future features and prerequisites</Link></p>
  </main>;
}
