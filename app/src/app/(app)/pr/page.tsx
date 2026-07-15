import { redirect } from 'next/navigation';
import { loadActor } from '@folio-lib/server/guard';

export const dynamic = 'force-dynamic';

export default async function PrLanding() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  redirect('/my-waybills?scope=mine');
}
