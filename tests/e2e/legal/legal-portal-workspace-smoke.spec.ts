import {test} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {verifySharedWorkspace} from '../../support/helpers/host-workspaces';
test.use({actionTimeout:30_000});
test("Legal-Portal workspace serves its owned host and public entry and published visitor surface without business mutations",browserProjects(['desktop'],'Forward development smoke; production acceptance remains deferred.'),async({page},info)=>{
 test.setTimeout(300_000);await verifySharedWorkspace(page,info,'legal-portal');
});
