import {assistantQuotaSql} from './assistant-quota-sql';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function GuidedIntakeCollaborationFacts(database: SqlDatabase) { return { guard4(id: string, value: "assistant-quota", companyId: string, value2: number, companyId2: string, month: string, companyId3: string, month2: string, companyId4: string, month3: string, companyId5: string) { return assertion(database, id, value, `${assistantQuotaSql()}`, [companyId, value2, companyId2, month, companyId3, month2, companyId4, month3, companyId5]); } }; }
