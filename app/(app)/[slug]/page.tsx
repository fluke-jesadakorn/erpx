import { notFound, redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { SlugTile } from '../(protected)/_components/SlugTile';

const DEPRECATED_SLUGS = new Set(['my-waybills', 'my_waybills']);

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ slug: string }>;
}

export default async function TilePageRoute({ params }: PageProps) {
  const { slug } = await params;
  if (DEPRECATED_SLUGS.has(slug)) notFound();
  const actor = await loadActor();
  if (!actor) redirect('/?login=1');

  return <SlugTile slug={slug} actor={actor as any} />;
}