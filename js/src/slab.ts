import { PublicKey } from "@solana/web3.js";
import { Schema, deserialize, BinaryReader } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";

///////////////////////////////////////////////
////// Nodes and Slab
///////////////////////////////////////////////

export class BytesSlab {
  buffer: Buffer | Uint8Array;

  constructor(buf: Uint8Array) {
    this.buffer = buf;
  }

  borshDeserialize(reader: BinaryReader) {
    this.buffer = reader.buf.slice(reader.offset);
  }
}

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
