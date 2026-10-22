import PosScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pos-entry';
import {generateMetadata} from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/metadata';
import {notFound} from 'next/navigation';
export {generateMetadata};
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;companySlug:string;app:string;branchSlug?:string;departmentSlug?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(!["pos"].includes(p.app))notFound();const Screen=PosScreen;return <Screen params={Promise.resolve({...p,path:["branches",p.branchSlug,"departments",p.departmentSlug,"shifts"].filter((x):x is string=>!!x)})} searchParams={searchParams}/>;}
