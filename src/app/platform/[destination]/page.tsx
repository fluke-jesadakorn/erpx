import { notFound, redirect } from 'next/navigation';
export default async function LegacyPlatformDestination({params}:{params:Promise<{destination:string}>}) {
  const {destination}=await params;
  if (!['home','work','feed','inbox','learn'].includes(destination)) notFound();
  redirect('/holdings/erpx/manage-erpx');
}
