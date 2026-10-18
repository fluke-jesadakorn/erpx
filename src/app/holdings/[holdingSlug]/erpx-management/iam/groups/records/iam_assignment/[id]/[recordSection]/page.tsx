import Screen from '@/app/holdings/[holdingSlug]/erpx-management/iam/groups/_entry';
import {notFound} from 'next/navigation';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>}){const p=await params;if(p.recordSection!=='overview')notFound();return <Screen/>;}
