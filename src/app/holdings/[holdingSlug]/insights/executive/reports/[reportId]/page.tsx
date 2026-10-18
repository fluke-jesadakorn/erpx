import Screen from "@/app/holdings/[holdingSlug]/_presenters/executive/reports/[reportId]/screen";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;reportId:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const query={...await searchParams,...{}};
 const props={params:Promise.resolve({...await params,...{}}),searchParams:Promise.resolve(query)} as Parameters<typeof Screen>[0];
 return (await Screen(props))??null;
}
