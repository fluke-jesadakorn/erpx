import {technologyContext} from '@/app/holdings/[holdingSlug]/administration/technology/_context';
import {TechnologySectionWorkspace} from '@/components/identity_access/technology/section-workspace';
import {PageHeader} from '@/components/ui/primitives';
import {IamConsole} from '@/components/iam/sections/reviews';
import {AccountInvitations} from '@/components/identity_access/account-invitations';
import {hasPermission} from '@/lib/identity_access/permissions';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const {local,actor,queries,base}=await technologyContext((await params).holdingSlug,'settings'),query=await searchParams;const data=await queries.iamOverview(actor,false);return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="Technology" title="Access settings" description="Choose a section, inspect records and review changes."/><TechnologySectionWorkspace base={base} view="settings" section="reviews" title="Access settings"><><IamConsole embedded presentation="workspace" data={data} base={base} api={`/api/v1${base}/iam`} section="reviews"/>{hasPermission(local,'iam.roles.manage')&&<AccountInvitations actor={local} base={base}/>}</></TechnologySectionWorkspace></div>;}
