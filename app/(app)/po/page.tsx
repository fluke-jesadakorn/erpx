import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';

export const dynamic = 'force-dynamic';

export default async function PoLanding() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  redirect('/my-waybills?scope=all');
}
