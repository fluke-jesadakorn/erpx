import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {CommercialUsageNode as UsageNode} from "../../commercial/public/usage-contract";
export function commercialUsageNodes(db:SqlDatabase,){return db.prepare("SELECT id,kind,display_name,company_id FROM organization_nodes WHERE kind IN ('company','branch')")
      .all<UsageNode>();}
