import Screen,{generateMetadata} from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-front-base-entry';
export {generateMetadata};
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;companySlug:string;app:string;branchSlug?:string;departmentSlug?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const p=await params;return <Screen params={Promise.resolve({...p,path:[]})} searchParams={searchParams}/>;}
