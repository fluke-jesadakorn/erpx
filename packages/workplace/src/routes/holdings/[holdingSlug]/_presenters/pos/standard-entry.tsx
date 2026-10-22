import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import { posMovementSaleForPresentation } from "@erpx/server-pos/public/movement-presentation-query";
import { requireWorkContext,publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { posWorkspace } from "@erpx/server-pos/public/pos";


import { StandardBackofficePosConsole as PosConsole } from "@erpx/feature-pos/components/standard-pos-backoffice-console";
import { frontWorkspaceHref } from "@erpx/server-work_projection/public/front-app";
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
 const financial=!hasPermission(actor,"pos.read"),section=financial?(search.tab==='settings'?'settings':'reports'):search.tab??'sale';
 const movement=search.movement?await posMovementSaleForPresentation(actor, search.movement):null;
 const data=await posWorkspace(actor,(search.detailKind === "pos_sale" ? search.detailId : search.sale)??(movement?String(movement.sale_id):undefined),search.q??'',section,search.cursor,false,financial);return <PosConsole key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}:${search.tab??'sale'}:${search.sale??''}`} initial={data} base={workspaceBase(publicScope(actor))} initialTab={section} financeMode={financial} initialRead={{sale:(search.detailKind === "pos_sale" ? search.detailId : search.sale)??(movement?String(movement.sale_id):undefined),q:search.q,tab:search.tab,cursor:search.cursor}} frontHref={frontHref??undefined}/>;
}
