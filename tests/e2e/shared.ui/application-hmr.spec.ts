import{representativeRoute}from '../../support/helpers/representative-route';
import{verifyFrontWorkspace,verifySharedWorkspace}from '../../support/helpers/host-workspaces';
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

test("pos owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{test.setTimeout(420_000);await verifyFrontWorkspace(page,context,info,'pos',true);});

test("pms owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page,context},info)=>{test.setTimeout(420_000);await verifyFrontWorkspace(page,context,info,'pms',true);});

test("booking owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(420_000);await verifySharedWorkspace(page,info,'booking',true);});

test("portal owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(420_000);await verifySharedWorkspace(page,info,'portal',true);});

test("legal-portal owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(420_000);await verifySharedWorkspace(page,info,'legal-portal',true);});

test("forms owned host preserves its functional state through twenty measured HMR updates",browserProjects(['desktop'],'Explicit development performance benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(420_000);await verifySharedWorkspace(page,info,'forms',true);});

test("finance representative cold route uses normal fictional sign-in on its declared runtime",browserProjects(['desktop'],'Explicit isolated development route benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(360_000);await representativeRoute(page,info,'finance');});

test("people representative cold route uses normal fictional sign-in on its declared runtime",browserProjects(['desktop'],'Explicit isolated development route benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(360_000);await representativeRoute(page,info,'people');});

test("work representative cold route uses normal fictional sign-in on its declared runtime",browserProjects(['desktop'],'Explicit isolated development route benchmark; production acceptance remains deferred.'),async({page},info)=>{test.setTimeout(360_000);await representativeRoute(page,info,'work');});
