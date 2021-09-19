import { Keypair, TransactionInstruction } from "@solana/web3.js";

export type PrimedTransaction = [Keypair[], TransactionInstruction[]];

export interface Price {
  size: number;
  price: number;
}
