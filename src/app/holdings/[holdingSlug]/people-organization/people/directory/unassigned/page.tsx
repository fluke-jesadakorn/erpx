import Screen from '@/app/holdings/[holdingSlug]/people-organization/people/directory/_collection';
export default function Page({params}:{params:Promise<{holdingSlug:string}>}){return <Screen params={params} searchParams={Promise.resolve({view:'internal'})}/>;}
