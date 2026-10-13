export * from "@/server/modules/organization/public/rules/organization-domain";
import {reportingIssues as reportingIssuesAt,type OrganizationPerson,type OrganizationPosition} from "@/server/modules/organization/public/rules/organization-domain";
/** UI/native compatibility; core callers supply their clock-derived date. */
export function reportingIssues(person:OrganizationPerson,people:OrganizationPerson[],positions:OrganizationPosition[],today=new Date().toISOString().slice(0,10)){return reportingIssuesAt(person,people,positions,today);}
