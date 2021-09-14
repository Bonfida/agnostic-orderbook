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

// Devnet
const AAOB_ID = new PublicKey("2sgmVooraACQbzTABEzAr4k33FhUxBi8mkgfbFRKMWSX");

export const createMarket = async (
  connection: Connection,
  callerAuthority: PublicKey,
  callBackInfoLen: BN,
  callBackIdLen: BN,
  eventCapacity: number,
  nodesCapacity: number,
  feePayer: PublicKey
): Promise<PrimedTransaction> => {
  let signers: Keypair[] = [];
  let txInstructions: TransactionInstruction[] = [];

  // Event queue account
  const eventQueue = new Keypair();
  const eventQueueSize =
    EventQueueHeader.LEN +
    42 +
    (1 + 33 + 2 * callBackInfoLen.toNumber()) * eventCapacity;
  const createEventQueueAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      eventQueueSize
    ),
    newAccountPubkey: eventQueue.publicKey,
    programId: AAOB_ID,
    space: eventQueueSize,
  });
  signers.push(eventQueue);
  txInstructions.push(createEventQueueAccount);

  // Bids account
  const bids = new Keypair();
  const nodeSize = Math.max(28, 24 + callBackInfoLen.toNumber());
  const slabSize = SlabHeader.LEN + nodeSize * nodesCapacity;
  const createBidsAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(slabSize),
    newAccountPubkey: bids.publicKey,
    programId: AAOB_ID,
    space: slabSize,
  });
  signers.push(bids);
  txInstructions.push(createBidsAccount);

  // Asks account
  const asks = new Keypair();
  const createAsksAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(slabSize),
    newAccountPubkey: asks.publicKey,
    programId: AAOB_ID,
    space: slabSize,
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
