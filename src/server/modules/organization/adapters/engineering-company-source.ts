import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export async function engineeringCompanySource(db:SqlDatabase,companyId:string):Promise<boolean>{return !!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND kind='company'").bind(companyId).first();}
