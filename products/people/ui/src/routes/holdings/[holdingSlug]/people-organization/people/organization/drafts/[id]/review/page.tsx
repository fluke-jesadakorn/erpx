import {DraftPage} from '@erpx/platform-workplace/routes/holdings/[holdingSlug]/_presenters/people/organization/drafts/draft-page';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id?:string}>}){return <DraftPage {...await params} review={true}/>;}
