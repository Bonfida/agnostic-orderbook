// This file is auto-generated. DO NOT EDIT
import BN from "bn.js";
import { Schema, serialize } from "borsh";
import { PublicKey, TransactionInstruction } from "@solana/web3.js";

export interface AccountKey {
  pubkey: PublicKey;
  isSigner: boolean;
  isWritable: boolean;
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
  constructor(obj: {
    orderId: BN;
  }) {
    this.tag = 3
    this.orderId = obj.orderId;
  }
  serialize(): Uint8Array {
    return serialize(cancelOrderInstruction.schema, this);
  }
  getInstruction(
    programId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
    authority: PublicKey,
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys: AccountKey[] = [];
    keys.push({
      pubkey: market,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: eventQueue,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: bids,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: asks,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: authority,
      isSigner: true,
      isWritable: false,
    });
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
export class newOrderInstruction {
  tag: number;
  maxBaseQty: BN;
  maxQuoteQty: BN;
  limitPrice: BN;
  side: number;
  matchLimit: BN;
  callbackInfo: number;
  postOnly: number;
  postAllowed: number;
  selfTradeBehavior: number;
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
          ["callbackInfo", "u8"],
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
    callbackInfo: number;
    postOnly: number;
    postAllowed: number;
    selfTradeBehavior: number;
  }) {
    this.tag = 1
    this.maxBaseQty = obj.maxBaseQty;
    this.maxQuoteQty = obj.maxQuoteQty;
    this.limitPrice = obj.limitPrice;
    this.side = obj.side;
    this.matchLimit = obj.matchLimit;
    this.callbackInfo = obj.callbackInfo;
    this.postOnly = obj.postOnly;
    this.postAllowed = obj.postAllowed;
    this.selfTradeBehavior = obj.selfTradeBehavior;
  }
  serialize(): Uint8Array {
    return serialize(newOrderInstruction.schema, this);
  }
  getInstruction(
    programId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
    authority: PublicKey,
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys: AccountKey[] = [];
    keys.push({
      pubkey: market,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: eventQueue,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: bids,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: asks,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: authority,
      isSigner: true,
      isWritable: false,
    });
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
export class consumeEventsInstruction {
  tag: number;
  numberOfEntriesToConsume: BN;
  static schema: Schema = new Map([
    [
      consumeEventsInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["numberOfEntriesToConsume", "u64"],
        ],
      },
    ],
  ]);
  constructor(obj: {
    numberOfEntriesToConsume: BN;
  }) {
    this.tag = 2
    this.numberOfEntriesToConsume = obj.numberOfEntriesToConsume;
  }
  serialize(): Uint8Array {
    return serialize(consumeEventsInstruction.schema, this);
  }
  getInstruction(
    programId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    authority: PublicKey,
    rewardTarget: PublicKey,
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys: AccountKey[] = [];
    keys.push({
      pubkey: market,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: eventQueue,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: authority,
      isSigner: true,
      isWritable: false,
    });
    keys.push({
      pubkey: rewardTarget,
      isSigner: false,
      isWritable: true,
    });
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
export class createMarketInstruction {
  tag: number;
  callerAuthority: Uint8Array;
  callbackInfoLen: BN;
  callbackIdLen: BN;
  minBaseOrderSize: BN;
  tickSize: BN;
  crankerReward: BN;
  static schema: Schema = new Map([
    [
      createMarketInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
          ["callerAuthority", [32]],
          ["callbackInfoLen", "u64"],
          ["callbackIdLen", "u64"],
          ["minBaseOrderSize", "u64"],
          ["tickSize", "u64"],
          ["crankerReward", "u64"],
        ],
      },
    ],
  ]);
  constructor(obj: {
    callerAuthority: Uint8Array;
    callbackInfoLen: BN;
    callbackIdLen: BN;
    minBaseOrderSize: BN;
    tickSize: BN;
    crankerReward: BN;
  }) {
    this.tag = 0
    this.callerAuthority = obj.callerAuthority;
    this.callbackInfoLen = obj.callbackInfoLen;
    this.callbackIdLen = obj.callbackIdLen;
    this.minBaseOrderSize = obj.minBaseOrderSize;
    this.tickSize = obj.tickSize;
    this.crankerReward = obj.crankerReward;
  }
  serialize(): Uint8Array {
    return serialize(createMarketInstruction.schema, this);
  }
  getInstruction(
    programId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys: AccountKey[] = [];
    keys.push({
      pubkey: market,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: eventQueue,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: bids,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: asks,
      isSigner: false,
      isWritable: true,
    });
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
export class closeMarketInstruction {
  tag: number;
  static schema: Schema = new Map([
    [
      closeMarketInstruction,
      {
        kind: "struct",
        fields: [
          ["tag", "u8"],
        ],
      },
    ],
  ]);
  constructor() {
    this.tag = 4
  }
  serialize(): Uint8Array {
    return serialize(closeMarketInstruction.schema, this);
  }
  getInstruction(
    programId: PublicKey,
    market: PublicKey,
    eventQueue: PublicKey,
    bids: PublicKey,
    asks: PublicKey,
    authority: PublicKey,
    lamportsTargetAccount: PublicKey,
  ): TransactionInstruction {
    const data = Buffer.from(this.serialize());
    let keys: AccountKey[] = [];
    keys.push({
      pubkey: market,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: eventQueue,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: bids,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: asks,
      isSigner: false,
      isWritable: true,
    });
    keys.push({
      pubkey: authority,
      isSigner: true,
      isWritable: false,
    });
    keys.push({
      pubkey: lamportsTargetAccount,
      isSigner: false,
      isWritable: true,
    });
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
