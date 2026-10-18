import PmsScreen from '@/app/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pms-daily-entry';
import {generateMetadata} from '@/app/front/[holdingSlug]/[companySlug]/[app]/_screens/metadata';
import {notFound} from 'next/navigation';
export {generateMetadata};
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;companySlug:string;app:string;branchSlug?:string;departmentSlug?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;if(!["pms"].includes(p.app))notFound();const Screen=PmsScreen;return <Screen params={Promise.resolve({...p,path:["branches",p.branchSlug,"departments",p.departmentSlug,"cashier"].filter((x):x is string=>!!x)})} searchParams={searchParams}/>;}
