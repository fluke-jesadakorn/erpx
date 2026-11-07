import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/support/screen";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const props={params:Promise.resolve({...await params,...{}}),searchParams:Promise.resolve({...await searchParams,...{}})} as Parameters<typeof Screen>[0];
 return (await Screen(props))??null;
}
