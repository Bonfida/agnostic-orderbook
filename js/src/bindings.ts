import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import BN from "bn.js";
import { EventQueueHeader } from "./event_queue";
import { MarketState } from "./market_state";
import { SlabHeader } from "./slab";
import { createMarketInstruction } from "./instructions";
import { PrimedTransaction } from "./types";

const AAOB_ID = PublicKey.default;

export const createMarket = async (
  connection: Connection,
  callerAuthority: PublicKey,
  callBackInfoLen: BN,
  callBackIdLen: BN,
  feePayer: PublicKey
): Promise<PrimedTransaction> => {
  let signers: Keypair[] = [];
  let txInstructions: TransactionInstruction[] = [];

  // Event queue account
  const eventQueue = new Keypair();
  const createEventQueueAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      EventQueueHeader.LEN
    ),
    newAccountPubkey: eventQueue.publicKey,
    programId: AAOB_ID,
    space: EventQueueHeader.LEN,
  });
  signers.push(eventQueue);
  txInstructions.push(createEventQueueAccount);

  // Bids account
  const bids = new Keypair();
  const createBidsAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      SlabHeader.LEN
    ),
    newAccountPubkey: bids.publicKey,
    programId: AAOB_ID,
    space: SlabHeader.LEN,
  });
  signers.push(bids);
  txInstructions.push(createBidsAccount);

  // Asks account
  const asks = new Keypair();
  const createAsksAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      SlabHeader.LEN
    ),
    newAccountPubkey: asks.publicKey,
    programId: AAOB_ID,
    space: SlabHeader.LEN,
  });
  signers.push(asks);
  txInstructions.push(createAsksAccount);

  // Market account
  const market = new Keypair();
  const createMarketAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      MarketState.LEN
    ),
    newAccountPubkey: market.publicKey,
    programId: AAOB_ID,
    space: MarketState.LEN,
  });
  signers.push(market);
  txInstructions.push(createMarketAccount);

  // Create market
  const createMarket = new createMarketInstruction({
    callerAuthority: callerAuthority.toBuffer(),
    callBackInfoLen,
    callBackIdLen,
  }).getInstruction(
    AAOB_ID,
    market.publicKey,
    eventQueue.publicKey,
    bids.publicKey,
    asks.publicKey
  );
  txInstructions.push(createMarket);

  return [signers, txInstructions];
};
