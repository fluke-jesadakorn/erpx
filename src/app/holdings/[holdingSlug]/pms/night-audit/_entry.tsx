import Screen from "@/app/holdings/[holdingSlug]/_presenters/pms/standard-entry";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const query={...await searchParams,...{}};
 const props={params:Promise.resolve({...await params,...{"section":["night-audit"]}}),searchParams:Promise.resolve(query)} as Parameters<typeof Screen>[0];
 return (await Screen(props))??null;
}
