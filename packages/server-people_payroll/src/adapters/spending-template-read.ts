import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { spendingTemplateOwner } from '@erpx/server-organization/public/spending-template-owner';
import { createSpendingTemplateRead } from '@erpx/server-people_payroll/application/spending-templates';
export function spendingTemplate(db:SqlDatabase,companyId:string){return createSpendingTemplateRead({read:company=>spendingTemplateOwner(db).read(company)})(companyId);}
