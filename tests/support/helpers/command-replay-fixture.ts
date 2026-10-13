/** Model the native receipt JSON boundary; replay tests keep the stored bytes unchanged. */
export function storedCommandReceipt(receipt: Record<string, unknown>) {
  const serialized = JSON.stringify(receipt);
  return async <Result = Record<string, unknown>>(): Promise<Result | null> => JSON.parse(serialized);
}
