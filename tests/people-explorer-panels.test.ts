import { expect, it } from 'vitest';
import { childSelections, hiddenDraftKeys, peopleExplorerTabs, selectPanel, trayKindsForParents } from '../src/lib/people-explorer';
import { emptyDraft } from '../src/lib/organization-draft';
import type { HoldingPeopleDirectory } from '../src/server/holding-people-directory';
const data:HoldingPeopleDirectory={companies:[{id:'c',name:'Company',slug:'c'}],nodes:[{id:'b',name:'Branch',slug:'b',kind:'branch',companyId:'c',branchId:null,parentId:'c'},{id:'d',name:'Department',slug:'d',kind:'department',companyId:'c',branchId:'b',parentId:'b'},{id:'e',name:'Other department',slug:'e',kind:'department',companyId:'c',branchId:'b',parentId:'b'}],positions:[{id:'p',companyId:'c',branchId:'b',departmentId:'d',title:'Position',levelId:'l',reportsToPositionId:null,isCeo:false}],levels:[{id:'l',companyId:'c',name:'Level',number:2,code:'L2'}],people:[{id:'eng',engagementId:'eng',personId:'person',companyId:'c',branchId:'b',departmentId:'d',positionId:'p',legalName:'Employee',jobTitle:'Position',workEmail:null,workPhone:null}]};
it('opens three rows, replaces only the clicked path and never opens a child for an employee',()=>{
 const initial=[{selection:{kind:'company' as const,id:'c'},tab:''}];
 const source=selectPanel(initial,0,{kind:'department',id:'d'},data);
 const destination=selectPanel(initial,0,{kind:'branch',id:'b'},data);
 expect(source).toHaveLength(2);expect(destination).toHaveLength(2);
 const deeper=selectPanel(source,1,{kind:'position',id:'p'},data);expect(deeper).toHaveLength(3);
 const employee=selectPanel(deeper,2,{kind:'person',id:'eng'},data);expect(employee).toHaveLength(3);
 const changed=selectPanel(employee,0,{kind:'department',id:'e'},data);expect(changed).toHaveLength(1);
 expect(destination[0].selection).toEqual({kind:'branch',id:'b'});
 expect(childSelections(data,{kind:'position',id:'p'})).toEqual([{kind:'person',id:'eng'}]);
});
it('hides children carried by a staged group while keeping the single draft untouched',()=>{
 const draft={...emptyDraft(),items:[{kind:'department' as const,id:'d',parked:true}]};
 expect([...hiddenDraftKeys(data,draft,true)]).toEqual(['department:d','position:p','person:eng']);
 expect([...hiddenDraftKeys(data,draft,false)]).toEqual(['department:d','position:p','person:eng']);
 expect(draft.items).toHaveLength(1);
});
it('limits HR and compensation tabs separately',()=>{
 expect(peopleExplorerTabs('person',false,false)).toEqual(['Profile']);
 expect(peopleExplorerTabs('person',false,true)).toEqual(['Profile','Compensation']);
 expect(peopleExplorerTabs('person',true,false)).toEqual(['Profile','Employment','History']);
});
it('hides carried teams and their descendants even when children precede parents in the directory',()=>{
 const nested={...data,nodes:[{...data.nodes[1],id:'child',parentId:'team'},{...data.nodes[1],id:'team',kind:'team' as const,parentId:'d'},...data.nodes],positions:[{...data.positions[0],departmentId:'child'}],people:[{...data.people[0],departmentId:'child'}]};
 const draft={...emptyDraft(),items:[{kind:'department' as const,id:'d',parked:true}]};
 expect([...hiddenDraftKeys(nested,draft,true)]).toEqual(expect.arrayContaining(['department:d','team:team','department:child','position:p','person:eng']));
});

it('keeps row indices when opening a Tray detail beneath the other path',()=>{
 const path=selectPanel([{selection:null,tab:''}],2,{kind:'person',id:'eng'},data);
 expect(path).toHaveLength(3);expect(path[1]).toEqual({selection:null,tab:''});expect(path[2].selection).toEqual({kind:'person',id:'eng'});
});
it('keeps trays available when the two sides drill through different item types',()=>{
 expect(trayKindsForParents([{kind:'department',id:'d'},{kind:'position',id:'p'}])).toEqual(['position','person']);
 expect(trayKindsForParents([])).toEqual(['department']);
});
