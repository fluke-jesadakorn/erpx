import Screen from '@erpx/workplace/routes/holdings/[holdingSlug]/erpx-management/iam/roles/_entry';
import {notFound} from 'next/navigation';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>}){const p=await params;if(p.recordSection!=='overview')notFound();return <Screen/>;}
