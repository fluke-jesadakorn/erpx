import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import { salesCustomerAccess } from '@erpx/server-sales/public/rules/navigation';
import type { NavigationContext } from '@erpx/feature-work_projection/lib/workplace-navigation';
import type { Destination } from '@erpx/feature-work_projection/lib/destination-registry';

/** Discovery mirrors owner authorization; readers still authorize every request. */
export function destinationAvailable(context:NavigationContext,item:Destination) {
 const actor={role:context.role??'employee',permissions:context.permissions,assignmentScope:context.scopeKind};
 const can=(permission:string)=>hasPermission(actor,permission),holding=(context.scopeKind??'holding')==='holding';
 const {source,selection}=item;
 // These continuations enter through a fresh named owner proof, not through
 // a generic collection's module/records permission or its feature tabs.
 if(source==='assets'&&selection.view==='accounting'
   ||source==='field-service'&&['parts-setup','initial-stock'].includes(selection.view)
   ||['projects','knowledge'].includes(source)&&selection.view==='worker-preparation')return false;
 if(source==='home'||source==='inbox'||source==='calendar')return true;
 if(source.startsWith('work'))return selection.view==='team'?context.workViewAvailability?.team!==false:source==='work/routing'?context.workViewAvailability?.routing!==false:true;
 if(context.beta)return source==='learn'||source==='settings'&&selection.section==='feedback'||source==='people'&&selection.view==='internal'||source==='knowledge'&&selection.section==='published';
 const rollout:Record<string,string>={people:'people','people/organization':'people','people/roster':'people','people/settings':'people','people/evaluation':'people','people/payroll':'people','people/payslips':'people',technology:'technology',sales:'sales',customers:'crm',assets:'assets',benefits:'benefits',talent:'talent',marketing:'marketing',ecommerce:'ecommerce',legal:'legal',planning:'planning',manufacturing:'manufacturing',quality:'quality',executive:'executive',projects:'projects','staff-tasks':'projects','routine-templates':'projects',operations:'operations'};
 const feature=['sales','customers'].includes(source)?undefined:source==='commerce'?selection.view==='purchase'||selection.view==='parties'?'procurement':['stock','products','warehouses'].includes(selection.view)?'inventory':'sales':rollout[source];
 if(feature&&context.availableAppIds&&!context.availableAppIds.includes(feature)&&!(source==='customers'&&context.availableAppIds.some(id=>['sales','support','customer_authority','customer_360'].includes(id)))&&!(source==='assets'&&context.availableAppIds.includes('maintenance')))return false;
 if(source==='people')return ['external','unclassified'].includes(selection.view)?holding&&can('people.relationships.read'):selection.view==='users'?holding&&can('iam.read'):selection.status==='archived'?can('people.manage'):['people.read','people.manage','iam.read','organization.manage'].some(can);
 if(source==='people/settings')return holding&&(selection.section==='pay-groups'?can('people.payroll.details'):['leave','calendars'].includes(selection.section)?can('leave.policy.manage'):can('people.manage')&&can('organization.manage'));
 if(source==='people/organization')return ['people.read','people.manage','iam.read','organization.manage'].some(can);
 if(source==='people/roster')return can('roster.read');
 if(source==='people/evaluation')return can('people.read');
 if(source.startsWith('people/payroll'))return holding&&(can('payroll.read')||can('payroll.payment.read'));
 if(source==='people/payslips')return can('payroll.read')||can('people.read');
 if(source==='technology')return selection.view==='organization'?['people.read','people.manage','iam.read','organization.manage'].some(can):can('iam.read')&&(selection.view!=='directory'||holding);
 if(source==='sales'||source==='customers') {const access=salesCustomerAccess({...actor,branchId:holding?null:'branch'},context.availableAppIds);return !!access[(source==='sales'?selection.view:selection.view) as keyof typeof access];}
 if(source==='manufacturing'&&selection.setup==='initial-stock')return context.scopeKind!=='department'&&can('inventory.read')&&can('inventory.prepare');
 if(source==='manufacturing'&&selection.setup==='posting')return holding&&can('finance.read')&&['finance.write','finance.approve'].some(can);
 if(source==='commerce')return can('commerce.read')&&(selection.view!=='pricing'||holding&&can('commerce.sales'));
 if(source==='settings')return !!context.adminSections?.includes(selection.section as NonNullable<NavigationContext['adminSections']>[number])&&(selection.section!=='brand'||selection.tab!=='media'||!!context.canManageApprovals);
 if(source.startsWith('settings/')||source==='agents')return source==='settings/notifications'?true:source==='settings/stripe'?holding&&['pos.configure','pos.payments.configure','pms.configure','pms.accounting.prepare','pms.accounting.approve'].some(can):source==='agents'?hasModuleAccess(context.moduleIds??[],'agent')&&can('records.read'):holding&&can('organization.manage');
 if(context.availableAppIds&&['finance/engines','finance/group'].includes(source))return false;
 if(source.startsWith('finance')||source==='expenses')return can('finance.read')&&(source!=='expenses'||can('expense.read')||can('expense.review.read'))&&(!source.includes('hotel')||context.scopeKind!=='department'&&can('pms.reports'));
 if(source==='pos')return selection.tab==='reports'?can('pos.reports'):selection.tab==='settings'?can('pos.configure')||can('pos.payments.configure'):can('pos.approve');
 if(source.startsWith('pms')){const permissions:Record<string,string[]>= {approvals:['pms.approve'],reports:['pms.reports'],settings:['pms.configure','pms.accounting.prepare','pms.accounting.approve'],reservations:['pms.reservations'],'front-desk':['pms.frontdesk'],rooms:['pms.housekeeping','pms.maintenance'],housekeeping:['pms.housekeeping','pms.inspect'],'guest-accounts':['pms.cashier'],cashier:['pms.cashier'],'night-audit':['pms.audit']};return context.scopeKind!=='department'&&(permissions[source.split('/')[1]]??['pms.read']).some(can);}
 if(source.startsWith('manage-erpx'))return !!context.manageErpxVisible;
 if(source==='legal')return can('legal.read');
 if(source==='executive')return context.executiveVisible??can('executive.access');
 if(source==='knowledge')return selection.view==='handbook'?selection.section==='published'||selection.section==='drafts'&&!!context.handbookAccess?.editor||selection.section==='editors'&&!!context.handbookAccess?.owner:hasModuleAccess(context.moduleIds??[],'learning')&&can('records.read');
 if(['learn','future-features'].includes(source))return true;
 if(source.startsWith('support'))return can('support.read');
 if(source.startsWith('stock'))return context.scopeKind!=='department'&&can('inventory.read');
 if(['procurement-rfqs','supplier-bills'].includes(source))return can('commerce.read');
 if(['projects','staff-tasks','routine-templates','requests'].includes(source))return true;
 const modules:Record<string,string>={'field-service':'field_service',assets:selection.view==='register'?'assets':'maintenance',benefits:'benefits',talent:'talent',marketing:'marketing',ecommerce:'ecommerce',planning:'planning',manufacturing:'manufacturing',operations:'operations',quality:'quality'};
 return source in modules&&hasModuleAccess(context.moduleIds??[],modules[source])&&can(source==='ecommerce'?'commerce.read':source==='quality'?'quality.read':'records.read');
}
