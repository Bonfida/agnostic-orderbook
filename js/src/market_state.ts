import { Connection, PublicKey, Commitment } from "@solana/web3.js";
import { Schema, deserializeUnchecked } from "borsh";
import { Slab } from "./slab";
import BN from "bn.js";

///////////////////////////////////////////////
////// Market State
///////////////////////////////////////////////

/** @enum {number} */
export enum AccountTag {
  Initialized = 0,
  Market = 1,
  EventQueue = 2,
  Bids = 3,
  Asks = 4,
}

/** @enum {number} */
export enum SelfTradeBehavior {
  DecrementTake = 0,
  CancelProvide = 1,
  AbortTransaction = 2,
}

/**
 * MarketState object
 */
export class MarketState {
  tag: BN;
  callerAuthority: PublicKey;
  eventQueue: PublicKey;
  bids: PublicKey;
  asks: PublicKey;
  callBackIdLen: BN;
  callBackInfoLen: BN;
  feeBudget: BN;
  initialLamports: BN;
  minOrderSize: BN;
  tickSize: BN;
  crankerReward: BN;

  static LEN: number = 192;

  static schema: Schema = new Map([
    [
      MarketState,
      {
        kind: "struct",
        fields: [
          ["tag", "u64"],
          ["callerAuthority", [32]],
          ["eventQueue", [32]],
          ["bids", [32]],
          ["asks", [32]],
          ["callBackIdLen", "u64"],
          ["callBackInfoLen", "u64"],
          ["feeBudget", "u64"],
          ["initialLamports", "u64"],
          ["minOrderSize", "u64"],
          ["tickSize", "u64"],
          ["crankerReward", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: AccountTag;
    callerAuthority: Uint8Array;
    eventQueue: Uint8Array;
    bids: Uint8Array;
    asks: Uint8Array;
    callBackInfoLen: BN;
    callBackIdLen: BN;
    feeBudget: BN;
    initialLamports: BN;
    minOrderSize: BN;
    tickSize: BN;
    crankerReward: BN;
  }) {
    this.tag = new BN(arg.tag);
    this.callerAuthority = new PublicKey(arg.callerAuthority);
    this.eventQueue = new PublicKey(arg.eventQueue);
    this.bids = new PublicKey(arg.bids);
    this.asks = new PublicKey(arg.asks);
    this.callBackInfoLen = arg.callBackInfoLen;
    this.callBackIdLen = arg.callBackIdLen;
    this.feeBudget = arg.feeBudget;
    this.initialLamports = arg.initialLamports;
    this.minOrderSize = arg.minOrderSize;
    this.tickSize = arg.tickSize;
    this.crankerReward = arg.crankerReward;
  }

  /**
   * Loads a market from its address
   * @param connection The solana connection object to the RPC node
   * @param market The address of the market to load
   * @returns Returns a market state object
   */
  static async retrieve(
    connection: Connection,
    market: PublicKey,
    commitment?: Commitment
  ) {
    const accountInfo = await connection.getAccountInfo(market, commitment);
    if (!accountInfo?.data) {
      throw new Error("Invalid account provided");
    }
    return deserializeUnchecked(
      this.schema,
      MarketState,
      accountInfo.data
    ) as MarketState;
  }

  /**
   * Loads the bids Slab associated to the market
   * @param connection The solana connection object to the RPC node
   * @returns Returns a Slab object
   */
  async loadBidsSlab(connection: Connection, commitment?: Commitment) {
    const bidsInfo = await connection.getAccountInfo(this.bids, commitment);
    if (!bidsInfo?.data) {
      throw new Error("Invalid bids account");
    }
    return Slab.deserialize(bidsInfo.data, this.callBackInfoLen);
  }

  /**
   * Loads the asks Slab associated to the market
   * @param connection The solana connection object to the RPC node
   * @returns Returns a Slab object
   */
  async loadAsksSlab(connection: Connection, commitment?: Commitment) {
    const asksInfo = await connection.getAccountInfo(this.asks, commitment);
    if (!asksInfo?.data) {
      throw new Error("Invalid asks account");
    }
    return Slab.deserialize(asksInfo.data, this.callBackInfoLen);
  }
}
