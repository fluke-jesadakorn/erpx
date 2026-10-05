import { requireWorkContext,publicScope } from '@/server/authorization';
import { workspaceBase } from '@/lib/workspace';
import { posWorkspace } from '@/server/pos';
import { holdingResources } from '@/server/runtime';
import { one } from '@/server/pos-core';
import { PosConsole } from '@/components/pos/pos-console';
import { frontWorkspaceHref } from '@/server/front-app';
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
 const movement=search.movement?await one((await holdingResources(actor.holdingId)).write,'SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1',search.movement,actor.companyId,actor.branchId,actor.branchId):null;
 const data=await posWorkspace(actor,(search.detailKind === "pos_sale" ? search.detailId : search.sale)??(movement?String(movement.sale_id):undefined),search.q??'',search.tab??'sale',search.cursor);return <PosConsole key={`${search.tab??'sale'}:${search.sale??''}`} initial={data} base={workspaceBase(publicScope(actor))} initialTab={search.tab} frontHref={frontHref??undefined}/>;
}
