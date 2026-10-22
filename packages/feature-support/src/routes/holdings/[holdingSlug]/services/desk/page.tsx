import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/work/screen";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {return <Screen params={params} searchParams={Promise.resolve({...await searchParams,view:"all",type:"service_request"})}/>;}
