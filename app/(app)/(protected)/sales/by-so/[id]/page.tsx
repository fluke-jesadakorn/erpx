import { redirect } from 'next/navigation';
import { query } from '@/db';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

export default async function SoRedirect({ params }: PageProps) {
  const { id: ref } = await params;
  const r = await query<{ id: number }>(
    `SELECT id FROM sales_orders WHERE so_number = $1`,
    [ref],
  );
  if (r.rows[0]) {
    const wb = await query<{ id: string }>(
      `SELECT id FROM waybills WHERE origin = 'so' AND origin_id = $1`,
      [r.rows[0].id],
    );
    if (wb.rows[0]) {
      redirect(`/waybill/${wb.rows[0].id}`);
    }
  }
  redirect('/sales');
}
