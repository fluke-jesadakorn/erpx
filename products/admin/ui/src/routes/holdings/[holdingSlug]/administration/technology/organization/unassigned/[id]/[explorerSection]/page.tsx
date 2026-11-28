import Screen from '@erpx/platform-workplace/routes/holdings/[holdingSlug]/administration/technology/organization/_screen';
import {notFound,forbidden} from 'next/navigation';
import {requireWorkContext} from '@erpx/identity-access-server/public/authorization';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;explorerSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(!['policies','directory','delivery'].includes(p.explorerSection))notFound();const actor=await requireWorkContext(p.holdingSlug);if(p.explorerSection==='directory'&&!hasPermission(actor,'iam.read'))forbidden();return <Screen params={Promise.resolve({holdingSlug:p.holdingSlug})} searchParams={Promise.resolve({...await searchParams,view:'organization',unassigned:p.id,section:p.explorerSection==='delivery'?'operations':p.explorerSection})}/>;}
