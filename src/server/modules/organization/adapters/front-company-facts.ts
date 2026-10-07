import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {storefrontBrandColumns,storefrontBrandJoin} from "../../brand/public/front-brand-projection";
export function frontCompanyFacts(db:SqlDatabase){return {
async companyBrand(companySlug:string){return db.prepare(
    "SELECT c.id,c.slug,c.display_name," + storefrontBrandColumns + " " +
    "FROM organization_nodes c " + storefrontBrandJoin + " " +
    "WHERE c.kind='company' AND c.slug=? AND c.status='active'",
  ).bind(companySlug).first<{
    id: string;
    slug: string;
    display_name: string;
    short_name: string | null;
    accent_color: string | null;
    logo_attachment_id: string | null;
    logo_publication_id:string|null;
  }>();},
async branch(companyId:string,value:string){return db.prepare(
      "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='branch' AND slug=? AND status='active'",
    ).bind(companyId, value).first<{ slug: string }>();},
async department(companyId:string,value:string){return db.prepare(
      "SELECT slug FROM organization_nodes WHERE company_id=? AND kind='department' AND slug=? AND branch_id IS NULL AND status='active'",
    ).bind(companyId, value).first<{ slug: string }>();},
async branchDepartment(companyId:string,value:string,value2:string){return db.prepare(
      "SELECT d.slug,b.slug AS branch_slug FROM organization_nodes d " +
      "JOIN organization_nodes b ON b.id=d.branch_id AND b.company_id=d.company_id AND b.kind='branch' AND b.status='active' " +
      "WHERE d.company_id=? AND d.kind='department' AND d.slug=? AND b.slug=? AND d.status='active'",
    ).bind(companyId, value, value2).first<{ slug: string; branch_slug: string }>();},
async actorCompany(companyId:string,companyId2:string){return db.prepare(
    "SELECT slug FROM organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'",
  ).bind(companyId, companyId2).first<{ slug: string }>();}
};}
