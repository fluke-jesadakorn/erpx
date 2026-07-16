// Root template: forces subtree re-mount on every navigation.
// Per Next.js docs (template.js), templates receive a unique key per segment
// change → DOM reset + state reset + effect re-sync. This fixes stale
// event handlers on the Navbar (which contains Server-Component data +
// Client-Component children) when navigating between routes without a
// full page refresh.

export const dynamic = 'force-dynamic';

export default function Template({ children }: { children: React.ReactNode }) {
  return <>{children}</>;
}