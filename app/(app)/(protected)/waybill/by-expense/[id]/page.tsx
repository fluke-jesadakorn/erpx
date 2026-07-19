import { redirect } from 'next/navigation';
import { loadWaybillByOrigin } from '@/waybill/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export default async function LookupByExpense({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ submitted?: string }>;
}) {
  const { id } = await params;
  const sp = await searchParams;
  const wb = await loadWaybillByOrigin('expense', Number(id));
  if (!wb) redirect('/expense');
  redirect(`/waybill/${wb.id}${sp.submitted === '1' ? '?submitted=1' : ''}`);
}
