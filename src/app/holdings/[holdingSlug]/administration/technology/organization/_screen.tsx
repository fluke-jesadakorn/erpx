import {technologyContext} from '@/app/holdings/[holdingSlug]/administration/technology/_context';
import {StandardOrganizationAccessWorkspace} from '@/components/identity_access/technology/standard-organization-access-workspace';
import {PageHeader} from '@/components/ui/primitives';
import {hasPermission} from '@/lib/identity_access/permissions';
export default async function Screen({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const {local,actor,queries,base,canReadDirectory}=await technologyContext((await params).holdingSlug,'organization'),query=await searchParams;      const [organization, lifecycle, automation, technology, gpoRules] = await Promise.all([
        queries.organizationAccessData(local), queries.peopleLifecycleData(local),
        canReadDirectory ? queries.directoryAutomationData(actor) : Promise.resolve({ cases: [], connections: [], jobs: [], canApply: false, canManage: false, canReview: false }),
        canReadDirectory ? queries.technologyData(actor) : Promise.resolve({ applications: [], operations: [] }),
        queries.gpoRulesOverview(local),
      ]);
      return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="Technology" title="Organization & Access" description="Choose a unit to review positions, people, policies and delivery."/><StandardOrganizationAccessWorkspace base={base} companyName={organization.nodes.find(node => node.kind === "company")?.display_name ?? local.holdingName} organization={organization} lifecycle={lifecycle} automation={automation} technology={technology} gpoRules={gpoRules} canApply={canReadDirectory && hasPermission(actor,"iam.directory.apply")} initialSection={["person","position","company","branch","department","team","level"].some(key=>query[key])?undefined:typeof query.section === "string" ? query.section : undefined}/></div>;
}
