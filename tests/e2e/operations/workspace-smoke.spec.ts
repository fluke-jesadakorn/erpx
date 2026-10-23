import {test} from '@playwright/test';
import {browserProjects} from '../../../scripts/testing/browser-results';
import {verifyProductWorkspaceSmoke} from '../../support/helpers/product-workspace';

test('Work workspace signs in through Account and opens its isolated drawer without business mutations',browserProjects(['desktop'],'Single forward development smoke; production acceptance matrix remains deferred.'),async({page,context},info)=>{
 test.setTimeout(240_000);
 await verifyProductWorkspaceSmoke(page,context,info,'work');
});
