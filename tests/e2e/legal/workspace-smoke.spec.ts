import {test} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {verifyApplicationWorkspace} from '../../support/helpers/application-workspace';
test.use({actionTimeout:30_000});
test("Legal workspace preserves existing module authority through Account SSO and its owner drawer without Commercial grants",browserProjects(['desktop'],'Forward development smoke; production acceptance remains deferred.'),async({page,context},info)=>{
 test.setTimeout(300_000);await verifyApplicationWorkspace(page,context,info,'legal');
});
