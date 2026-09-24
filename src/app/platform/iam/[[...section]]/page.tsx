import { redirect } from 'next/navigation';
export default async function LegacyPlatformIam({params}:{params:Promise<{section?:string[]}>}) {
  const {section}=await params;
  redirect('/holdings/erpx/manage-erpx/iam' + (section?.length ? '/' + section.map(encodeURIComponent).join('/') : ''));
}
