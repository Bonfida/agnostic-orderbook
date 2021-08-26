import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize, deserializeUnchecked, serialize } from "borsh";
import BN from "bn.js";

///////////////////////////////////////////////
////// Market State
///////////////////////////////////////////////

export class MarketState {
  accountFlags: number;
  callerAuthority: PublicKey;
  eventQueue: PublicKey;
  bids: PublicKey;
  asks: PublicKey;
  callBackInfoLen: BN;

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
          ["callBackInfoLen", "u64"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    accountFlags: number;
    callerAuthority: Uint8Array;
    eventQueue: Uint8Array;
    bids: Uint8Array;
    asks: Uint8Array;
    callBackInfoLen: BN;
  }) {
    this.accountFlags = arg.accountFlags;
    this.callerAuthority = new PublicKey(arg.callerAuthority);
    this.eventQueue = new PublicKey(arg.eventQueue);
    this.bids = new PublicKey(arg.bids);
    this.asks = new PublicKey(arg.asks);
    this.callBackInfoLen = arg.callBackInfoLen;
  }

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

  async loadBidsSlab(connection: Connection) {
    const bidsInfo = await connection.getAccountInfo(this.bids);
    if (!bidsInfo?.data) {
      throw new Error("Invalid bids account");
    }
    return deserialize(Slab.schema, Slab, bidsInfo.data) as Slab;
  }

  async loadAsksSlab(connection: Connection) {
    const asksInfo = await connection.getAccountInfo(this.asks);
    if (!asksInfo?.data) {
      throw new Error("Invalid asks account");
    }
    return deserialize(Slab.schema, Slab, asksInfo.data) as Slab;
  }
}

///////////////////////////////////////////////
////// Event Queue
///////////////////////////////////////////////

export class EventQueueHeader {
  accountFlags: BN;
  head: BN;
  count: BN;
  eventSize: BN;
  registerSize: BN;
  seqNum: BN;

  constructor(arg: {
    accountFlags: BN;
    head: BN;
    count: BN;
    eventSize: BN;
    registerSize: BN;
    seqNum: BN;
  }) {
    this.accountFlags = arg.accountFlags;
    this.head = arg.head;
    this.count = arg.count;
    this.eventSize = arg.eventSize;
    this.registerSize = arg.registerSize;
    this.seqNum = arg.seqNum;
  }
}

export enum Side {
  Bid,
  Ask,
}

export class EventFill {
  takerSide: Side;
  makerOrderId: BN;
  quoteSize: BN;
  assetSize: BN;
  makerCallbackInfo: number[];
  takerCallbackInfo: number[];

  constructor(arg: {
    takerSide: number;
    makerOrderId: BN;
    quoteSize: BN;
    assetSize: BN;
    makerCallbackInfo: number[];
    takerCallbackInfo: number[];
  }) {
    this.takerSide = arg.takerSide == 0 ? Side.Bid : Side.Ask;
    this.makerOrderId = arg.makerOrderId;
    this.quoteSize = arg.quoteSize;
    this.assetSize = arg.assetSize;
    this.makerCallbackInfo = arg.makerCallbackInfo;
    this.takerCallbackInfo = arg.takerCallbackInfo;
  }
}

export class EventOut {
  side: Side;
  orderId: BN;
  assetSize: BN;
  callBackInfo: number[];

  constructor(arg: {
    side: number;
    orderId: BN;
    assetSize: BN;
    callBackInfo: number[];
  }) {
    this.side = arg.side == 0 ? Side.Bid : Side.Ask;
    this.orderId = arg.orderId;
    this.assetSize = arg.assetSize;
    this.callBackInfo = arg.callBackInfo;
  }
}

export class EventQueue {
  header: EventQueueHeader;
  buffer: number[];
  callBackInfoLen: number;

  static LEN_FILL = 1 + 16 + 8 + 8 + 1 + 1;
  static LEN_OUT = 1 + 16 + 8 + 8 + 1;

  // @ts-ignore
  static schema: Schema = new Map([
    [
      EventQueue,
      {
        kind: "struct",
        fields: [
          ["header", EventQueueHeader],
          ["buffer", ["u8"]],
          ["callBackInfoLen", "u8"],
        ],
      },
    ],
    [
      EventQueueHeader,
      {
        kind: "struct",
        fields: [
          ["accountFlags", "u64"],
          ["head", "u64"],
          ["count", "u64"],
          ["eventSize", "u64"],
          ["registerSize", "u64"],
          ["seqNum", "u64"],
        ],
      },
    ],
    [
      EventFill,
      {
        kind: "struct",
        fields: [
          ["side", "u8"],
          ["makerOrderId", "u128"],
          ["quoteSize", "u64"],
          ["assetSize", "u64"],
          ["makerCallbackInfo", ["u8"]],
          ["takerCallbackInfo", ["u8"]],
        ],
      },
    ],
    [
      EventOut,
      {
        kind: "struct",
        fields: [
          ["side", "u8"],
          ["orderId", "u128"],
          ["quoteSize", "u64"],
          ["assetSize", "u64"],
          ["callBackInfo", ["u8"]],
        ],
      },
    ],
  ]);

  constructor(arg: {
    header: EventQueueHeader;
    buffer: number[];
    callBackInfoLen: number;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;
  }

  static parse(data: Buffer) {
    return deserializeUnchecked(this.schema, EventQueue, data) as EventQueue;
  }

  static async load(connection: Connection, address: PublicKey) {
    const accountInfo = await connection.getAccountInfo(address);
    if (!accountInfo?.data) {
      throw new Error("Invalid address provided");
    }
    return this.parse(accountInfo.data);
  }

  static parseEvent(data: Buffer) {
    switch (data.length) {
      case this.LEN_FILL:
        return deserialize(this.schema, EventFill, data) as EventFill;
      case this.LEN_OUT:
        return deserialize(this.schema, EventOut, data) as EventOut;
      default:
        throw new Error("Invalid data provided");
    }
  }

  static parseEventQueueHeader(data: Buffer) {
    return deserialize(this.schema, EventQueueHeader, data) as EventQueueHeader;
  }
}

///////////////////////////////////////////////
////// Nodes and Slab
///////////////////////////////////////////////

export class InnerNode {
  prefixLen: number;
  key: BN;
  children: number[];

  constructor(arg: { prefixLen: number; key: BN; children: number[] }) {
    this.prefixLen = arg.prefixLen;
    this.key = arg.key;
    this.children = arg.children;
  }
}

export class LeafNode {
  key: BN;
  callBackInfo: number[];
  assetQuantity: BN;

  constructor(arg: { key: BN; callBackInfo: number[]; assetQuantity: BN }) {
    this.key = arg.key;
    this.callBackInfo = arg.callBackInfo;
    this.assetQuantity = arg.assetQuantity;
  }
}

export class FreeNode {
  next: number;

  constructor(arg: { next: number }) {
    this.next = arg.next;
  }
}

export class Node {
  // @ts-ignore
  static schema: Schema = new Map([
    [
      InnerNode,
      {
        kind: "struct",
        fields: [
          ["prefixLen", "u32"],
          ["key", "u128"],
          ["children", [2]],
        ],
      },
    ],
    [
      LeafNode,
      {
        kind: "struct",
        fields: [
          ["key", "u128"],
          ["callBackInfo", ["u8"]],
          ["assetQuantity", "u64"],
        ],
      },
    ],
    [
      FreeNode,
      {
        kind: "struct",
        fields: [["next", "u32"]],
      },
    ],
    [
      Node,
      {
        kind: "enum",
        values: [
          ["uninitialized", [0]],
          ["inner", InnerNode],
          ["leaf", LeafNode],
          ["free", FreeNode],
          ["lastFree", FreeNode],
        ],
      },
    ],
  ]);

  static parse(data: Buffer) {
    return deserialize(this.schema, Node, data) as Node;
  }
}

export class SlabHeader {
  bumpIndex: BN;
  freeListLen: BN;
  freeListHead: number;
  rootNode: number;
  leafCount: BN;
  marketAddress: PublicKey;

  constructor(arg: {
    bumpIndex: BN;
    freeListLen: BN;
    freeListHead: number;
    rootNode: number;
    leafCount: BN;
    marketAddress: Uint8Array;
  }) {
    this.bumpIndex = arg.bumpIndex;
    this.freeListLen = arg.freeListLen;
    this.freeListHead = arg.freeListHead;
    this.rootNode = arg.rootNode;
    this.leafCount = arg.leafCount;
    this.marketAddress = new PublicKey(arg.marketAddress);
  }
}

export class Slab {
  header: SlabHeader;
  buffer: number[];
  callBackInfoLen: number;
  slotSize: number;

  // @ts-ignore
  static schema: Schema = new Map([
    [
      SlabHeader,
      {
        kind: "struct",
        values: [
          ["bumpIndex", "u64"],
          ["freeListLen", "u64"],
          ["freeListHead", "u32"],
          ["rootNode", "u32"],
          ["leafCount", "u64"],
          ["marketAddress", [32]],
        ],
      },
    ],
    [
      Slab,
      {
        kind: "struct",
        values: [
          ["header", SlabHeader],
          ["buffer", ["u8"]],
          ["callBackInfoLen", "u8"],
          ["slotSize", "u8"],
        ],
      },
    ],
  ]);

  constructor(arg: {
    header: SlabHeader;
    buffer: number[];
    callBackInfoLen: number;
    slotSize: number;
  }) {
    this.header = arg.header;
    this.buffer = arg.buffer;
    this.callBackInfoLen = arg.callBackInfoLen;
    this.slotSize = arg.slotSize;
  }
}
