import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import BN from "bn.js";
import { Schema, serialize } from "borsh";
import { Side } from "./event_queue";
import { SelfTradeBehavior } from "./market_state";

export class createMarketInstruction {
  tag: number;
  callerAuthority: Uint8Array;
  callBackInfoLen: BN;
  callBackIdLen: BN;
  minOrderSize: BN;
  priceBitMask: BN;

  static schema: Schema = new Map([
    [
      createMarketInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["callerAuthority", [32]],
          ["callBackInfoLen", "u64"],
          ["callBackIdLen", "u64"],
          ["minOrderSize", "u64"],
          ["priceBitMask", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    callerAuthority: Uint8Array;
    callBackInfoLen: BN;
    callBackIdLen: BN;
    minOrderSize: BN;
    priceBitMask: BN;
  }) {
    this.tag = 0;
    this.callerAuthority = obj.callerAuthority;
    this.callBackInfoLen = obj.callBackInfoLen;
    this.callBackIdLen = obj.callBackIdLen;
    this.minOrderSize = obj.minOrderSize;
    this.priceBitMask = obj.priceBitMask;
  }

  serialize(): Uint8Array {
    return serialize(createMarketInstruction.schema, this);
  }

  /**
   * Returns a TransactionInstruction to create a market
   * @param aaobId Program ID of the AAOB
   * @param market Address of the market
   * @param eventQueue Address of the event queue
   * @param bids Address of the bids slab
   * @param asks Address of asks slab
   * @returns Returns a TransactionInstruction object
   */
  getInstruction(
    aaobId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey
  ) {
    const data = Buffer.from(this.serialize());
    const keys = [
      // Account 1
      {
        pubkey: market,
        isSigner: false,
        isWritable: true,
      },
      // Account 2
      {
        pubkey: eventQueue,
        isSigner: false,
        isWritable: true,
      },
      // Account 3
      {
        pubkey: bids,
        isSigner: false,
        isWritable: true,
      },
      // Account 4
      {
        pubkey: asks,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: aaobId,
      data,
    });
  }
}

export class newOrderInstruction {
  tag: number;
  maxBaseQty: BN;
  maxQuoteQty: BN;
  limitPrice: BN;
  side: Side;
  matchLimit: BN;
  callBackInfo: Uint8Array;
  postOnly: boolean;
  postAllowed: boolean;
  selfTradeBehavior: SelfTradeBehavior;

  static schema: Schema = new Map([
    [
      newOrderInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["maxBaseQty", "u64"],
          ["maxQuoteQty", "u64"],
          ["limitPrice", "u64"],
          ["side", "u8"],
          ["matchLimit", "u64"],
          ["callBackInfo", ["u8"]],
          ["postOnly", "u8"],
          ["postAllowed", "u8"],
          ["selfTradeBehavior", "u8"],
        ],
      },
    ],
  ]);

  constructor(obj: {
    maxBaseQty: BN;
    maxQuoteQty: BN;
    limitPrice: BN;
    side: number;
    matchLimit: BN;
    callBackInfo: Uint8Array;
    postOnly: number;
    postAllowed: number;
    selfTradeBehavior: number;
  }) {
    this.tag = 1;
    this.maxBaseQty = obj.maxBaseQty;
    this.maxQuoteQty = obj.maxQuoteQty;
    this.limitPrice = obj.limitPrice;
    this.side = obj.side as Side;
    this.matchLimit = obj.matchLimit;
    this.callBackInfo = obj.callBackInfo;
    this.postOnly = obj.postOnly === 0;
    this.postAllowed = obj.postAllowed === 0;
    this.selfTradeBehavior = obj.selfTradeBehavior as SelfTradeBehavior;
  }

  serialize(): Uint8Array {
    return serialize(newOrderInstruction.schema, this);
  }

  /**
   * Returns a TransactionInstruction to place an order
   * @param aaobId Program ID of the AAOB
   * @param market Address of the market
   * @param eventQueue Address of the event queue
   * @param bids Address of the bids slab
   * @param asks Address of the asks slab
   * @param authority Address of the market authority
   * @returns Returns a TransactionInstruction object
   */
  getInstruction(
    aaobId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
    authority: PublicKey
  ) {
    const data = Buffer.from(this.serialize());
    const keys = [
      // Account 1
      {
        pubkey: market,
        isSigner: false,
        isWritable: true,
      },
      // Account 2
      {
        pubkey: eventQueue,
        isSigner: false,
        isWritable: true,
      },
      // Account 3
      {
        pubkey: bids,
        isSigner: false,
        isWritable: true,
      },
      // Account 4
      {
        pubkey: asks,
        isSigner: false,
        isWritable: true,
      },
      // Account 5
      {
        pubkey: authority,
        isSigner: true,
        isWritable: false,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: aaobId,
      data,
    });
  }
}

export class consumeEventInstruction {
  tag: number;
  numberToConsumer: BN;

  static schema: Schema = new Map([
    [
      consumeEventInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["numberToConsumer", "u64"],
        ],
      },
    ],
  ]);

  constructor(obj: { numberToConsumer: BN }) {
    this.tag = 2;
    this.numberToConsumer = obj.numberToConsumer;
  }

  serialize(): Uint8Array {
    return serialize(consumeEventInstruction.schema, this);
  }

  /**
   * Returns a TransactionInstruction to consume events
   * @param aaobId Program ID of the AAOB
   * @param market Address of the market
   * @param eventQueue Address of the event queue
   * @param authority Address of the market authority
   * @param rewardTarget Reward address of the cranker
   * @returns Returns a TransactionInstruction object
   */
  getInstruction(
    aaobId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    authority: PublicKey,
    rewardTarget: PublicKey
  ) {
    const data = Buffer.from(this.serialize());
    const keys = [
      // Account 1
      {
        pubkey: market,
        isSigner: false,
        isWritable: true,
      },
      // Account 2
      {
        pubkey: eventQueue,
        isSigner: false,
        isWritable: true,
      },
      // Account 3
      {
        pubkey: authority,
        isSigner: true,
        isWritable: false,
      },
      // Account 4
      {
        pubkey: rewardTarget,
        isSigner: false,
        isWritable: true,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: aaobId,
      data,
    });
  }
}

export class cancelOrderInstruction {
  tag: number;
  orderId: BN;

  static schema: Schema = new Map([
    [
      cancelOrderInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["orderId", "u128"],
        ],
      },
    ],
  ]);

  constructor(obj: { orderId: BN }) {
    this.tag = 3;
    this.orderId = obj.orderId;
  }

  serialize(): Uint8Array {
    return serialize(cancelOrderInstruction.schema, this);
  }

  /**
   * Returns a TransactionInstruction to cancel an order
   * @param aaobId Program ID of the AAOB
   * @param market Address of the market
   * @param eventQueue Address of the event queue
   * @param bids Address of the bids slab
   * @param asks Address of the asks slab
   * @param authority Address of the market authority
   * @returns Returns a TransactionInstruction object
   */
  getInstruction(
    aaobId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
    authority: PublicKey
  ) {
    const data = Buffer.from(this.serialize());
    const keys = [
      // Account 1
      {
        pubkey: market,
        isSigner: false,
        isWritable: true,
      },
      // Account 2
      {
        pubkey: eventQueue,
        isSigner: false,
        isWritable: true,
      },
      // Account 3
      {
        pubkey: bids,
        isSigner: false,
        isWritable: true,
      },
      // Account 4
      {
        pubkey: asks,
        isSigner: false,
        isWritable: true,
      },
      // Account 5
      {
        pubkey: authority,
        isSigner: true,
        isWritable: false,
      },
    ];

    return new TransactionInstruction({
      keys,
      programId: aaobId,
      data,
    });
  }
}
