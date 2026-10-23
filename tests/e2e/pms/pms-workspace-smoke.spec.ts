import {test} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {verifyFrontWorkspace} from '../../support/helpers/host-workspaces';
test.use({actionTimeout:30_000});
test("Pms workspace serves its owned host and normal Account SSO with a Front drawer without business mutations",browserProjects(['desktop'],'Forward development smoke; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(300_000);await verifyFrontWorkspace(page,context,info,'pms');
});
