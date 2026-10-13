import { posMovementSaleForPresentation } from "@/server/modules/pos/public/movement-presentation-query";
import { requireWorkContext,publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { posWorkspace } from "@/server/modules/pos/public/pos";


import { PosConsole } from '@/components/pos/pos-console';
import { frontWorkspaceHref } from "@/server/modules/work_projection/public/front-app";
import { redirect } from 'next/navigation';
export const dynamic='force-dynamic';
export default async function PosPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{sale?:string;detailKind?:string;detailId?:string;tab?:string;movement?:string;q?:string;cursor?:string;queue?:string}>}){
 const {holdingSlug}=await params,search=await searchParams,actor=await requireWorkContext(holdingSlug);
 const frontHref=await frontWorkspaceHref(actor,'pos');
 const dailyRoute=!search.tab||['sale','kitchen'].includes(search.tab)||(['history','shifts'].includes(search.tab)&&search.queue!=='approval');
 if(frontHref&&dailyRoute){
  const query=new URLSearchParams({tab:search.tab==='history'?'history':search.tab??'sale'});
  for(const [key,value] of Object.entries({sale:search.sale,detailKind:search.detailKind,detailId:search.detailId,movement:search.movement,q:search.q,cursor:search.cursor}))if(value)query.set(key,value);
  redirect(`${frontHref}?${query}`);
 }
 const movement=search.movement?await posMovementSaleForPresentation(actor, search.movement):null;
 const data=await posWorkspace(actor,(search.detailKind === "pos_sale" ? search.detailId : search.sale)??(movement?String(movement.sale_id):undefined),search.q??'',search.tab??'sale',search.cursor);return <PosConsole key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}:${search.tab??'sale'}:${search.sale??''}`} initial={data} base={workspaceBase(publicScope(actor))} initialTab={search.tab} initialRead={{sale:(search.detailKind === "pos_sale" ? search.detailId : search.sale)??(movement?String(movement.sale_id):undefined),q:search.q,tab:search.tab,cursor:search.cursor}} frontHref={frontHref??undefined}/>;
}
