import {test} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {verifyApplicationWorkspace} from '../../support/helpers/application-workspace';
test.use({actionTimeout:30_000});
test("Finance owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'finance',true);
});
test("Sales owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'sales',true);
});
test("Supply owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'supply',true);
});
test("Assets owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'assets',true);
});
test("Projects owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'projects',true);
});
test("Support owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'support',true);
});
test("Service owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'service',true);
});
test("Legal owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'legal',true);
});
test("Manufacturing owner drawer preserves its draft through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(360_000);await verifyApplicationWorkspace(page,context,info,'manufacturing',true);
});
