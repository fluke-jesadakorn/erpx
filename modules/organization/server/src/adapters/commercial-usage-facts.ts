import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {CommercialUsageNode as UsageNode} from "@erpx/commercial-server/public/usage-contract";
import {nodeKindDisplayNameHeadSql} from './organization-read-tail-sources-r62';
export function commercialUsageNodes(db:SqlDatabase,){return db.prepare(nodeKindDisplayNameHeadSql("company_id FROM organization_nodes WHERE kind IN ('company','branch')"))
      .all<UsageNode>();}
