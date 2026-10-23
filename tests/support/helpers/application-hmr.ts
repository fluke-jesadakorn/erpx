export const applicationHmrEntries={
 finance:{source:'packages/feature-finance/src/components/standard-finance-reporting-console.tsx',needle:"?'Prepare statement edition':",label:'Prepare statement edition',replace:(label:string)=>'?'+JSON.stringify(label)+':'},
 sales:{source:'packages/feature-sales/src/components/customers/sales-views.tsx',needle:'label="New lead"',label:'New lead',replace:(label:string)=>'label='+JSON.stringify(label)},
 supply:{source:'packages/feature-commerce/src/components/commerce-controls.tsx',needle:'title="Order details"',label:'Order details',replace:(label:string)=>'title='+JSON.stringify(label)},
 assets:{source:'packages/feature-assets/src/components/asset-maintenance-console.tsx',needle:'title="Register asset"',label:'Register asset',replace:(label:string)=>'title='+JSON.stringify(label)},
 projects:{source:'packages/feature-work_projection/src/components/staff-work-controls.tsx',needle:'"Manage project and members" : "New project"',label:'New project',replace:(label:string)=>'"Manage project and members" : '+JSON.stringify(label)},
 support:{source:'packages/feature-sales/src/components/customers/customer-views.tsx',needle:'label="New service case"',label:'New service case',replace:(label:string)=>'label='+JSON.stringify(label)},
 service:{source:'packages/feature-field_service/src/components/field-service-console.tsx',needle:'title="Create service order"',label:'Create service order',replace:(label:string)=>'title='+JSON.stringify(label)},
 legal:{source:'packages/feature-legal/src/components/legal-workspace.tsx',needle:'title={`${item ? "Edit" : "New"} ${kind}`}',label:'New request',replace:(label:string)=>'title={item ? `Edit ${kind}` : '+JSON.stringify(label)+'}'},
 manufacturing:{source:'packages/feature-manufacturing/src/components/production-console.tsx',needle:'<summary>Draft BOM or formula</summary>',label:'Draft BOM or formula',replace:(label:string)=>'<summary>{'+JSON.stringify(label)+'}</summary>'},
} as const;
