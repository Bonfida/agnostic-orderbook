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
  eventQueue: PublicKey;
  bids: PublicKey;
  asks: PublicKey;
  minBaseOrderSize: BN;
  tickSize: BN;
  callbackInfoLen!: number;

  static LEN: number = 120;

  static schema: Schema = new Map([
    [
      MarketState,
      {
        kind: "struct",
        fields: [
          ["tag", "u64"],
          ["eventQueue", [32]],
          ["bids", [32]],
          ["asks", [32]],
          ["minBaseOrderSize", "u64"],
          ["tickSize", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    tag: BN;
    eventQueue: Uint8Array;
    bids: Uint8Array;
    asks: Uint8Array;
    minBaseOrderSize: BN;
    tickSize: BN;
  }) {
    this.tag = new BN(arg.tag);
    this.eventQueue = new PublicKey(arg.eventQueue);
    this.bids = new PublicKey(arg.bids);
    this.asks = new PublicKey(arg.asks);
    this.minBaseOrderSize = arg.minBaseOrderSize;
    this.tickSize = arg.tickSize;
  }

  /**
   * Deserialize a market account data into `MarketState`
   * @param data Account data to deserialize
   * @returns
   */
  static deserialize(data: Buffer, callbackInfoLen: number): MarketState {
    const res = deserializeUnchecked(
      this.schema,
      MarketState,
      data
    ) as MarketState;
    res.callbackInfoLen = callbackInfoLen;
    return res;
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
    callbackInfoLen: number,
    commitment?: Commitment
  ) {
    const accountInfo = await connection.getAccountInfo(market, commitment);
    if (!accountInfo?.data) {
      throw new Error("Invalid account provided");
    }
    const res = this.deserialize(accountInfo.data, callbackInfoLen);
    return res;
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
    return Slab.deserialize(bidsInfo.data, this.callbackInfoLen);
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
    return Slab.deserialize(asksInfo.data, this.callbackInfoLen);
  }
}
