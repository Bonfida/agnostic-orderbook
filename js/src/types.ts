import { Keypair, TransactionInstruction } from "@solana/web3.js";
import BN from "bn.js";

export type PrimedTransaction = [Keypair[], TransactionInstruction[]];

export interface Price {
  size: BN;
  price: BN;
}
