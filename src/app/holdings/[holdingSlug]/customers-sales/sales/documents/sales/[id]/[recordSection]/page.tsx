import Screen from '@/app/holdings/[holdingSlug]/_presenters/sales/screen';
import {notFound} from 'next/navigation';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(p.recordSection&&p.recordSection!=='overview')notFound();return <Screen params={Promise.resolve(p)} searchParams={Promise.resolve({...await searchParams,view:"documents",type:"sales"})}/>;}
