import {technologyContext} from '@erpx/workplace/routes/holdings/[holdingSlug]/administration/technology/_context';
import {TechnologySectionWorkspace} from '@erpx/feature-identity_access/components/technology/section-workspace';
import {PeopleAccessWorkspace} from '@erpx/feature-identity_access/components/people-access-workspace';
import {AccountInvitations} from '@erpx/feature-identity_access/components/account-invitations';
import {PageHeader} from '@erpx/ui/components/primitives';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}) {
 const {local,actor,queries,base}=await technologyContext((await params).holdingSlug,'resources'),data=await queries.peopleAccessSnapshot(actor);
 return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="Administration" title="Accounts" description="Prepare accounts, invite users and assign access independently of employee records."/><TechnologySectionWorkspace base={base} view="resources" section="accounts" title="Accounts"><PeopleAccessWorkspace initial={data} base={base} api={`/api/v1${base}/iam/people-access`}/>{hasPermission(local,'iam.roles.manage')&&<AccountInvitations actor={local} base={base}/>}</TechnologySectionWorkspace></div>;
}
