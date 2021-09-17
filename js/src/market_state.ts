import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize } from "borsh";
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
  tag: AccountTag;
  callerAuthority: PublicKey;
  eventQueue: PublicKey;
  bids: PublicKey;
  asks: PublicKey;
  callBackIdLen: BN;
  callBackInfoLen: BN;
  feeBudget: BN;
  initialLamports: BN;

  static LEN: number = 161;

  static schema: Schema = new Map([
    [
      MarketState,
      {
        kind: "struct",
        fields: [
          ["accountFlags", "u8"],
          ["callerAuthority", [32]],
          ["eventQueue", [32]],
          ["bids", [32]],
          ["asks", [32]],
          ["callBackIdLen", "u64"],
          ["callBackInfoLen", "u64"],
          ["feeBudget", "u64"],
          ["initialLamports", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: number;
    callerAuthority: Uint8Array;
    eventQueue: Uint8Array;
    bids: Uint8Array;
    asks: Uint8Array;
    callBackInfoLen: BN;
    callBackIdLen: BN;
    feeBudget: BN;
    initialLamports: BN;
  }) {
    this.tag = arg.tag as AccountTag;
    this.callerAuthority = new PublicKey(arg.callerAuthority);
    this.eventQueue = new PublicKey(arg.eventQueue);
    this.bids = new PublicKey(arg.bids);
    this.asks = new PublicKey(arg.asks);
    this.callBackInfoLen = arg.callBackInfoLen;
    this.callBackIdLen = arg.callBackIdLen;
    this.feeBudget = arg.feeBudget;
    this.initialLamports = arg.initialLamports;
  }

  /**
   * Loads a market from its address
   * @param connection The solana connection object to the RPC node
   * @param market The address of the market to load
   * @returns Returns a market state object
   */
  static async retrieve(connection: Connection, market: PublicKey) {
    const accountInfo = await connection.getAccountInfo(market);
    if (!accountInfo?.data) {
      throw new Error("Invalid account provided");
    }
    return deserialize(
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
  async loadBidsSlab(connection: Connection) {
    const bidsInfo = await connection.getAccountInfo(this.bids);
    if (!bidsInfo?.data) {
      throw new Error("Invalid bids account");
    }
    return deserialize(Slab.schema, Slab, bidsInfo.data) as Slab;
  }

  /**
   * Loads the asks Slab associated to the market
   * @param connection The solana connection object to the RPC node
   * @returns Returns a Slab object
   */
  async loadAsksSlab(connection: Connection) {
    const asksInfo = await connection.getAccountInfo(this.asks);
    if (!asksInfo?.data) {
      throw new Error("Invalid asks account");
    }
    return deserialize(Slab.schema, Slab, asksInfo.data) as Slab;
  }
}
