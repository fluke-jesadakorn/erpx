import Screen from "@/app/holdings/[holdingSlug]/_presenters/manage-erpx/executive/screen";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const query={...await searchParams,...{}};
 return <Screen/>;
}
