export {assetsFinancialWorkspace} from '@erpx/finance-server/adapters/assets-financial-workflow';
export {createAssetsFinancialAsset,editAssetsFinancialAsset,cancelAssetsFinancialAsset,recognizeAssetsFinancialAsset,prepareAssetsFinancialDisposal,decideAssetsFinancialDisposal} from '@erpx/finance-server/adapters/finance-assets';
export {assetsFinancialProductEntry} from '@erpx/finance-server/adapters/assets-financial-entry';
export {assetsFinancialCreateSchema,assetsFinancialEditSchema,assetsFinancialRecognitionSchema,assetsFinancialDecisionSchema,assetsFinancialDisposalSchema} from '@erpx/finance-server/domain/assets-financial-workflow';
export type {AssetsFinancialWorkspace,AssetsFinancialCreateInput,AssetsFinancialEditInput,AssetsFinancialRecognitionInput,AssetsFinancialDecisionInput,AssetsFinancialDisposalInput,AssetsFinancialResult} from '@erpx/finance-server/domain/assets-financial-workflow';
export type {AssetsFinancialProductScope} from '@erpx/finance-server/adapters/assets-financial-entry';
