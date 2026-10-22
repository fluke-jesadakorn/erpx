import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/customers-sales/customers/reviews/identity/_screen";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const query={...await searchParams,...{"view":"reviews","review":"identity"}};
 const props={params:Promise.resolve({...await params,...{}}),searchParams:Promise.resolve(query)} as Parameters<typeof Screen>[0];
 return (await Screen(props))??null;
}
