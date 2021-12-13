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
import { Slab, SlabHeader } from "./slab";
import { createMarketInstruction } from "./raw_instructions";
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
 * @param callbackInfoLen An example of this would be to store a public key to uniquely identify the owner of a particular order. This example would require a value of 32
 * @param callbackIdLen The prefix length of callback information which is used to identify self-trading in this example
 * @param eventCapacity The capacity of an event
 * @param nodesCapacity The capacity of a node
 * @param feePayer The fee payer of the transaction
 * @param programId The agnostic orderbook program ID, or null to use the deployed program ID
 * @returns
 */
export const createMarket = async (
  connection: Connection,
  callerAuthority: PublicKey,
  callbackInfoLen: BN,
  callbackIdLen: BN,
  eventCapacity: number,
  nodesCapacity: number,
  minBaseOrderSize: BN,
  feePayer: PublicKey,
  tickSize: BN,
  crankerReward: BN,
  programId?: PublicKey
): Promise<PrimedTransaction> => {
  if (programId === undefined) {
    programId = AAOB_ID;
  }
  let signers: Keypair[] = [];
  let txInstructions: TransactionInstruction[] = [];

  // Event queue account
  const eventQueue = new Keypair();
  const eventQueueSize =
    EventQueueHeader.LEN +
    EventQueueHeader.REGISTER_SIZE +
    EventQueueHeader.computeSlotSize(callbackInfoLen)
      .muln(eventCapacity)
      .toNumber();
  const createEventQueueAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(
      eventQueueSize
    ),
    newAccountPubkey: eventQueue.publicKey,
    programId,
    space: eventQueueSize,
  });
  signers.push(eventQueue);
  txInstructions.push(createEventQueueAccount);

  // Bids account
  const bids = new Keypair();
  const slabSize = SlabHeader.PADDED_LEN + Slab.SLOT_SIZE * nodesCapacity;
  const createBidsAccount = SystemProgram.createAccount({
    fromPubkey: feePayer,
    lamports: await connection.getMinimumBalanceForRentExemption(slabSize),
    newAccountPubkey: bids.publicKey,
    programId,
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
    programId,
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
    programId,
    space: MarketState.LEN,
  });
  signers.push(market);
  txInstructions.push(createMarketAccount);

  // Create market
  const createMarket = new createMarketInstruction({
    callerAuthority: callerAuthority.toBuffer(),
    callbackInfoLen,
    callbackIdLen,
    minBaseOrderSize,
    tickSize,
    crankerReward,
  }).getInstruction(
    programId,
    market.publicKey,
    eventQueue.publicKey,
    bids.publicKey,
    asks.publicKey
  );
  txInstructions.push(createMarket);

  return [signers, txInstructions];
};
