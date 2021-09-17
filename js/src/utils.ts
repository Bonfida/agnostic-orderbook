import BN from "bn.js";

/**
 * Extract the order price from its order ID
 * @param key Order key
 * @returns The price of the order
 */
export function getPriceFromKey(key: BN) {
  return key.div(new BN(2).pow(new BN(64)));
}
