import Screen from '@/app/holdings/[holdingSlug]/_presenters/hospitality/setup/standard-screen';
export default function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}){return <Screen params={params} searchParams={searchParams}/>;}
