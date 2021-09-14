import { Keypair, TransactionInstruction } from "@solana/web3.js";

export type PrimedTransaction = [Keypair[], TransactionInstruction[]];
