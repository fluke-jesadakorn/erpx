import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { spendingTemplateOwner } from '@erpx/organization-server/public/spending-template-owner';
import { createSpendingTemplateRead } from '@erpx/people-payroll-server/application/spending-templates';
export function spendingTemplate(db:SqlDatabase,companyId:string){return createSpendingTemplateRead({read:company=>spendingTemplateOwner(db).read(company)})(companyId);}
