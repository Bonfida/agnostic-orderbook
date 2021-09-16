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
  ]);

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
  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new LeafNode({
      key: new BN(data.slice(0, 16), "le"),
      callBackInfo: [...data.slice(16, 16 + callbackInfoLen)],
      assetQuantity: new BN(
        data.slice(16 + callbackInfoLen, 24 + callbackInfoLen),
        "le"
      ),
    });
  }
}

export class FreeNode {
  next: number;

  static schema: Schema = new Map([
    [
      FreeNode,
      {
        kind: "struct",
        fields: [["next", "u32"]],
      },
    ],
  ]);

  constructor(arg: { next: number }) {
    this.next = arg.next;
  }
}

export function parseNode(
  callbackinfoLen: number,
  data: Buffer
): undefined | FreeNode | LeafNode | InnerNode {
  switch (data[0]) {
    case 0:
      throw "node is unitialized";
    case 1:
      return deserialize(InnerNode.schema, InnerNode, data.slice(1));
    case 2:
      return LeafNode.deserialize(callbackinfoLen, data.slice(1));
    case 3:
      return deserialize(FreeNode.schema, FreeNode, data.slice(1));
    case 4:
      return deserialize(FreeNode.schema, FreeNode, data.slice(1));
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

  static LEN: number = 65;

  static schema: Schema = new Map([
    [
      SlabHeader,
      {
        kind: "struct",
        fields: [
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
  ]);

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

  static deserialize(data: Buffer) {
    return deserialize(this.schema, SlabHeader, data);
  }
}

export class Slab {
  header: SlabHeader;
  callBackInfoLen: number;
  slotSize: number;

  // @ts-ignore
  static schema: Schema = new Map([
    [
      SlabHeader,
      {
        kind: "struct",
        fields: [
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
        fields: [["header", SlabHeader]],
      },
    ],
  ]);

  constructor(arg: {
    header: SlabHeader;
    callBackInfoLen: number;
    slotSize: number;
  }) {
    this.header = arg.header;
    this.callBackInfoLen = arg.callBackInfoLen;
    this.slotSize = arg.slotSize;
  }
}
