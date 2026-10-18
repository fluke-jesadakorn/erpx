import Screen from '@/app/holdings/[holdingSlug]/administration/technology/organization/_screen';
export default function Page({params}:{params:Promise<{holdingSlug:string}>}){return <Screen params={params} searchParams={Promise.resolve({view:'organization'})}/>;}
