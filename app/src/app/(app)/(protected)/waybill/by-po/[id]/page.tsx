import { redirect } from 'next/navigation';
import { loadWaybillByOrigin } from '@folio-lib/waybill/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export default async function LookupByExpense({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const wb = await loadWaybillByOrigin('po', Number(id));
  if (!wb) redirect('/expense');
  redirect(`/waybill/${wb.id}`);
}
