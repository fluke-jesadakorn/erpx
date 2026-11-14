import { RelationshipsPage } from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/relationships-page";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}) { return <RelationshipsPage params={params} classification="external" section="organizations"/>; }
