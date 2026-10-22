import type {Clock} from "@erpx/server-shared/application/ports";
export type LegalReportRows={results:Record<string,unknown>[]};
export function createLegalReportsQuery(ports:{require():void;clock:Clock;featureCategories:readonly string[];repository:{registers(now:number):Promise<LegalReportRows>;upcoming():Promise<LegalReportRows>;workflow(category:string):Promise<LegalReportRows>;costs():Promise<LegalReportRows>;access(now:number):Promise<LegalReportRows>;jobs():Promise<LegalReportRows>;invitations():Promise<LegalReportRows>}}){
async function legalReports() {
  ports.require();
  const registers = (
    await ports.repository.registers(ports.clock.now())
  ).results;
  const upcoming = (
    await ports.repository.upcoming()
  ).results;
  const workflows = [];
  for (const category of ports.featureCategories)
    workflows.push({
      category,
      rows: (
        await ports.repository.workflow(category)
      ).results,
    });
  const costs = (
    await ports.repository.costs()
  ).results;
  const access = (
    await ports.repository.access(ports.clock.now())
  ).results;
  const jobs = (
    await ports.repository.jobs()
  ).results;
  const invitations = (
    await ports.repository.invitations()
  ).results;
  return {
    asOf: ports.clock.now(),
    registers,
    upcoming,
    workflows,
    costs,
    access,
    jobs,
    invitations,
  };
}

return legalReports;}
