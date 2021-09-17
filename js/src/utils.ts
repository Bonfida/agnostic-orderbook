import { PublicKey } from "@solana/web3.js";
import {
  Schema,
  deserialize,
  BinaryReader,
  deserializeUnchecked,
  serialize,
} from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";

// Extract the order price from its order ID
export function getPriceFromKey(key: BN) {
  return key.div(new BN(2).pow(new BN(64)));
}
