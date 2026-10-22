import Screen from '@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/payroll/settings/posting/screen';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){return (await Screen({params}))??null;}
