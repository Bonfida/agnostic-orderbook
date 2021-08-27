import { Connection, PublicKey } from "@solana/web3.js";
import { Schema, deserialize, deserializeUnchecked, BinaryReader } from "borsh";
import BN from "bn.js";

///////////////////////////////////////////////
////// Market State
///////////////////////////////////////////////

export enum AccountTag {
  Initialized = 0,
  Market = 1,
  EventQueue = 2,
  Bids = 3,
  Asks = 4,
}

export enum EventType {
  Fill = 0,
  Out = 1,
}

export class BytesSlab {
  buffer: Buffer | Uint8Array;

  constructor(buf: Uint8Array) {
    this.buffer = buf;
  }

  borshDeserialize(reader: BinaryReader) {
    this.buffer = reader.buf.slice(reader.offset);
  }
}

export class MarketState {
  tag: AccountTag;
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
    tag: number;
    callerAuthority: Uint8Array;
    eventQueue: Uint8Array;
    bids: Uint8Array;
    asks: Uint8Array;
    callBackInfoLen: BN;
  }) {
    this.tag = arg.tag as AccountTag;
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
  tag: AccountTag;
  head: BN;
  count: BN;
  eventSize: BN;
  register: number[];
  seqNum: BN;

  constructor(arg: {
    tag: number;
    head: BN;
    count: BN;
    eventSize: BN;
    register: number[];
    seqNum: BN;
  }) {
    this.tag = arg.tag as AccountTag;
    this.head = arg.head;
    this.count = arg.count;
    this.eventSize = arg.eventSize;
    this.register = arg.register;
    this.seqNum = arg.seqNum;
  }
}

export enum Side {
  Bid = 0,
  Ask = 1,
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
    this.takerSide = arg.takerSide as Side;
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
    this.side = arg.side as Side;
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
          ["buffer", BytesSlab],
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
          ["seqNum", "u64"],
          ["register", ["u8"]],
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

  parseEvent(idx: number) {
    let offset =
      (idx * this.header.eventSize.toNumber() + this.header.head.toNumber()) %
      this.buffer.length;
    let data = Buffer.from(this.buffer.slice(offset));
    switch (data[0]) {
      case EventType.Fill:
        return deserializeUnchecked(
          EventQueue.schema,
          EventFill,
          data
        ) as EventFill;
      case EventType.Out:
        return deserializeUnchecked(
          EventQueue.schema,
          EventOut,
          data
        ) as EventOut;
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
  inner?: InnerNode;
  leaf?: LeafNode;
  free?: FreeNode;
  lastFree?: FreeNode;

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

  constructor(arg: {
    inner?: InnerNode;
    leaf?: LeafNode;
    free?: FreeNode;
    lastFree?: FreeNode;
  }) {
    this.inner = arg.inner;
    this.leaf = arg.leaf;
    this.free = arg.free;
    this.lastFree = arg.lastFree;
  }

  getNode(): InnerNode | LeafNode | FreeNode {
    if (!!this.inner) {
      return this.inner;
    }
    if (!!this.leaf) {
      return this.leaf;
    }
    if (!!this.free) {
      return this.free;
    }
    return this.lastFree as FreeNode;
  }

  static parse(data: Buffer) {
    return deserialize(this.schema, Node, data) as Node;
  }
}

export class SlabHeader {
  accountTag: AccountTag;
  bumpIndex: BN;
  freeListLen: BN;
  freeListHead: number;
  rootNode: number;
  leafCount: BN;
  marketAddress: PublicKey;

  constructor(arg: {
    accountTag: number;
    bumpIndex: BN;
    freeListLen: BN;
    freeListHead: number;
    rootNode: number;
    leafCount: BN;
    marketAddress: Uint8Array;
  }) {
    this.accountTag = arg.accountTag as AccountTag;
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
          ["accountTag", "u8"],
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
          ["buffer", BytesSlab],
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
