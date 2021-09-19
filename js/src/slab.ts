import { PublicKey } from "@solana/web3.js";
import { Schema, BinaryReader, deserializeUnchecked } from "borsh";
import BN from "bn.js";
import { AccountTag } from "./market_state";
import { Price } from "./types";
import { find_max, find_min, find_l2_depth } from "dex-wasm";

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
  static CHILD_OFFSET = 20;
  static CHILD_SIZE = 4;

  static schema: Schema = new Map([
    [
      InnerNode,
      {
        kind: "struct",
        fields: [
          ["prefixLen", "u32"],
          ["key", "u128"],
          ["children", ["u32", 2]],
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
  baseQuantity: BN;

  constructor(arg: { key: BN; callBackInfo: number[]; baseQuantity: BN }) {
    this.key = arg.key;
    this.callBackInfo = arg.callBackInfo;
    this.baseQuantity = arg.baseQuantity;
  }
  static deserialize(callbackInfoLen: number, data: Buffer) {
    return new LeafNode({
      key: new BN(data.slice(0, 16), "le"),
      callBackInfo: [...data.slice(16, 16 + callbackInfoLen)],
      baseQuantity: new BN(
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

/**
 * Deserializes a node buffer
 * @param callbackinfoLen Length of the callback info
 * @param data Buffer to deserialize
 * @returns Returns a node
 */
export function parseNode(
  callbackinfoLen: number,
  data: Buffer
): undefined | FreeNode | LeafNode | InnerNode {
  switch (data[0]) {
    case 0:
      throw new Error("node is unitialized");
    case 1:
      return deserializeUnchecked(InnerNode.schema, InnerNode, data.slice(1));
    case 2:
      return LeafNode.deserialize(callbackinfoLen, data.slice(1));
    case 3:
      return deserializeUnchecked(FreeNode.schema, FreeNode, data.slice(1));
    case 4:
      return deserializeUnchecked(FreeNode.schema, FreeNode, data.slice(1));
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
    return deserializeUnchecked(this.schema, SlabHeader, data) as SlabHeader;
  }
}

export class Slab {
  header: SlabHeader;
  callBackInfoLen: number;
  slotSize: number;
  data: Buffer;

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
    data: Buffer;
  }) {
    this.header = arg.header;
    this.callBackInfoLen = arg.callBackInfoLen;
    this.slotSize = Math.max(arg.callBackInfoLen + 8 + 16 + 1, 32);
    this.data = arg.data;
  }

  /**
   * Returns a node by its key
   * @param key Key of the node to fetch
   * @returns A node LeafNode object
   */
  getNodeByKey(key: number) {
    let pointer = this.header.rootNode;
    let offset = SlabHeader.LEN;
    while (true) {
      let node = parseNode(
        this.callBackInfoLen,
        this.data.slice(
          offset + pointer * this.slotSize,
          offset + (pointer + 1) * this.slotSize
        )
      );
      if (node instanceof InnerNode) {
        const critBitMaks = (1 << 127) >> node.prefixLen;
        let critBit = key & critBitMaks;
        pointer = node.children[critBit];
      }
      if (node instanceof LeafNode) {
        return node;
      }
    }
  }

  /**
   * Return min or max node of the critbit tree
   * @param max Boolean (false for best asks and true for best bids)
   * @returns Returns the min or max node of the Slab
   */
  getMinMax(max: boolean) {
    let pointer;
    if (max) {
      pointer = find_max(
        this.data,
        BigInt(this.callBackInfoLen),
        BigInt(this.slotSize)
      );
    } else {
      pointer = find_min(
        this.data,
        BigInt(this.callBackInfoLen),
        BigInt(this.slotSize)
      );
    }
    let offset = SlabHeader.LEN;
    if (!pointer) {
      throw new Error("Empty slab");
    }
    let node = parseNode(
      this.callBackInfoLen,
      this.data.slice(
        offset + pointer * this.slotSize,
        offset + (pointer + 1) * this.slotSize
      )
    );
    return node;
  }

  /**
   * Walkdown the critbit tree
   * @param descending
   * @returns
   */
  *items(descending = false): Generator<{
    key: BN;
    callBackInfo: number[];
    baseQuantity: BN;
  }> {
    if (this.header.leafCount.eq(new BN(0))) {
      return;
    }
    const stack = [this.header.rootNode];
    while (stack.length > 0) {
      const pointer = stack.pop();
      if (pointer === undefined) throw new Error("unreachable!");
      let offset = SlabHeader.LEN + pointer * this.slotSize;
      const node = parseNode(
        this.callBackInfoLen,
        this.data.slice(offset, offset + this.slotSize)
      );
      if (node instanceof LeafNode) {
        yield node;
      } else if (node instanceof InnerNode) {
        if (descending) {
          stack.push(node.children[0], node.children[1]);
        } else {
          stack.push(node.children[1], node.children[0]);
        }
      }
    }
  }

  [Symbol.iterator]() {
    return this.items(false);
  }

  /**
   * Returns an array of [price, size] given a certain depth
   * @param depth Depth to fetch
   * @param max Boolean (false for asks and true for bids)
   * @returns Returns an array made of [price, size] elements
   */
  getL2Depth(depth: number, increasing: boolean): Price[] {
    let raw = find_l2_depth(
      this.data,
      BigInt(this.callBackInfoLen),
      BigInt(this.slotSize),
      BigInt(depth),
      increasing
    );
    let result: Price[] = [];
    for (let i = 0; i < raw.length / 2; i++) {
      result.push({
        size: Number(raw[2 * i]),
        price: Number(raw[2 * i + 1]) / 2 ** 32,
      });
    }
    return result;
  }

  /**
   * Returns the top maxNbOrders (not aggregated by price)
   * @param maxNbOrders
   * @param max Boolean (false for asks and true for bids)
   * @returns Returns an array of LeafNode object
   */
  getMinMaxNodes(maxNbOrders: number, max: boolean) {
    const minMaxOrders: LeafNode[] = [];
    for (const leafNode of this.items(!max)) {
      if (minMaxOrders.length === maxNbOrders) {
        break;
      }
      minMaxOrders.push(leafNode);
    }
    return minMaxOrders;
  }
}
