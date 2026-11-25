import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercialUsageNode as UsageNode} from "@erpx/server-commercial/public/usage-contract";
import {nodeKindDisplayNameHeadSql} from './organization-read-tail-sources-r62';
export function commercialUsageNodes(db:SqlDatabase,){return db.prepare(nodeKindDisplayNameHeadSql("company_id FROM organization_nodes WHERE kind IN ('company','branch')"))
      .all<UsageNode>();}
