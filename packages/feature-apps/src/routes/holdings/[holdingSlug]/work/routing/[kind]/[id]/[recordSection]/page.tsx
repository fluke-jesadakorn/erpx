import Screen from '@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/work/routing/screen';
import {notFound} from 'next/navigation';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;kind:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(p.recordSection!=='overview')notFound();return <Screen params={Promise.resolve(p)} searchParams={searchParams}/>;}
