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
export const AAOB_ID = new PublicKey(
  "aaobKniTtDGvCZces7GH5UReLYP671bBkB96ahr9x3e"
);

/**
 *
 * @param connection The solana connection object to the RPC node
 * @param callerAuthority The caller authority will be the required signer for all market instructions.
 * Callback information can be used by the caller to attach specific information to all orders.
 * In practice, it will almost always be a program-derived address.
 * @param callBackInfoLen An example of this would be to store a public key to uniquely identify the owner of a particular order. This example would require a value of 32
 * @param callBackIdLen The prefix length of callback information which is used to identify self-trading in this example
 * @param eventCapacity The capacity of an event
 * @param nodesCapacity The capacity of a node
 * @param feePayer The fee payer of the transaction
 * @returns
 */
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
  const nodeSize = Math.max(32, 25 + callBackInfoLen.toNumber());
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
