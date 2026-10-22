import type {workspaceChoices} from "@erpx/server-organization/public/workspace-choices";
import type {FrontAppKind,FrontCompanyApp} from "@erpx/server-work_projection/domain/front-app";
import type {FrontCompanyChoice} from "@erpx/server-work_projection/domain/front-quick-access";
export function createFrontQuickAccessService(ports:{workspaces():ReturnType<typeof workspaceChoices>;activeCompanies(holding:string,ids:string[]):Promise<{id:string;slug:string}[]>;frontCompanyApp(holding:string,slug:string,app:FrontAppKind):Promise<FrontCompanyApp|null>}) {
async function frontCompanyChoices(app: FrontAppKind): Promise<FrontCompanyChoice[]> {
  const choices = (await ports.workspaces()).filter((choice) => choice.kind === "holding");
  const candidates = (await Promise.all(choices.map(async (choice) => {
    
    const ids = choice.companies.map((company) => company.id);
    const rows = await ports.activeCompanies(choice.holdingSlug, ids);
    return choice.companies.flatMap((company) => {
      const row = rows.find((candidate) => candidate.id === company.id);
      return row ? [{
        id: "company:" + choice.holdingSlug + ":" + row.slug,
        label: company.label,
        holding: choice.holdingSlug,
        slug: row.slug,
      }] : [];
    });
  }))).flat();
  const companies = await Promise.all(candidates.map(async (company) => {
    const entry = await ports.frontCompanyApp(company.holding, company.slug, app).catch(() => null);
    return entry?.locations.length ? company : null;
  }));
  return companies.filter((company): company is FrontCompanyChoice => !!company);
}


return {frontCompanyChoices};}
