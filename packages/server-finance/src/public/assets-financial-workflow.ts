export {assetsFinancialWorkspace} from '@erpx/server-finance/adapters/assets-financial-workflow';
export {createAssetsFinancialAsset,editAssetsFinancialAsset,cancelAssetsFinancialAsset,recognizeAssetsFinancialAsset,prepareAssetsFinancialDisposal,decideAssetsFinancialDisposal} from '@erpx/server-finance/adapters/finance-assets';
export {assetsFinancialProductEntry} from '@erpx/server-finance/adapters/assets-financial-entry';
export {assetsFinancialCreateSchema,assetsFinancialEditSchema,assetsFinancialRecognitionSchema,assetsFinancialDecisionSchema,assetsFinancialDisposalSchema} from '@erpx/server-finance/domain/assets-financial-workflow';
export type {AssetsFinancialWorkspace,AssetsFinancialCreateInput,AssetsFinancialEditInput,AssetsFinancialRecognitionInput,AssetsFinancialDecisionInput,AssetsFinancialDisposalInput,AssetsFinancialResult} from '@erpx/server-finance/domain/assets-financial-workflow';
export type {AssetsFinancialProductScope} from '@erpx/server-finance/adapters/assets-financial-entry';
