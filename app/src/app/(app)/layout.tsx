import { LayOut } from '@/components/LayOut';

export const dynamic = 'force-dynamic';

export default function AppLayout({ children }: { children: React.ReactNode }) {
  return <LayOut>{children}</LayOut>;
}