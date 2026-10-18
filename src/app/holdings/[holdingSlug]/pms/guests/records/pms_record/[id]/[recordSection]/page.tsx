import Screen from '@/app/holdings/[holdingSlug]/pms/guests/_entry';
import {notFound} from 'next/navigation';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(p.recordSection!=='overview')notFound();const props={params:Promise.resolve(p),searchParams:Promise.resolve({...await searchParams,detailKind:'pms_record',detailId:p.id})} as Parameters<typeof Screen>[0];return (await Screen(props))??null;}
