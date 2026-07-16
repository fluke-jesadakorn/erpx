import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { SignInPanel } from '@/components/SignInPanel';

export const dynamic = 'force-dynamic';

interface LoginPageProps {
  searchParams: Promise<{ next?: string | string[] }>;
}

function safeNext(raw: string | string[] | undefined): string {
  const v = Array.isArray(raw) ? raw[0] : raw;
  if (!v || typeof v !== 'string') return '/';
  if (!v.startsWith('/') || v.startsWith('//')) return '/';
  return v;
}

export default async function LoginPage({ searchParams }: LoginPageProps) {
  const actor = await loadActor();
  const params = await searchParams;
  const next = safeNext(params.next);

  if (actor) {
    redirect(next);
  }

  return <SignInPanel next={next} />;
}
