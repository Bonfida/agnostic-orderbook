import { PublicKey } from "@solana/web3.js";
import {
  Schema,
  deserialize,
  BinaryReader,
  deserializeUnchecked,
  serialize,
} from "borsh";
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
    return deserialize(this.schema, SlabHeader, data);
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

  // Get a specific node (i.e fetch 1 order)
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

  getMinMax(max: boolean) {
    let pointer = this.header.rootNode;
    let offset = SlabHeader.LEN;
    let critBit = max ? 1 : 0;
    while (true) {
      let node = parseNode(
        this.callBackInfoLen,
        this.data.slice(
          offset + pointer * this.slotSize,
          offset + (pointer + 1) * this.slotSize
        )
      );
      if (node instanceof InnerNode) {
        pointer = node.children[critBit];
        if (!pointer) pointer = node.children[(critBit + 1) % 2];
      }
      if (node instanceof LeafNode) {
        return node;
      }
    }
  }

  // Get the atmost max_nb_nodes smallest or biggest
  // nodes according to the critbit order
  getMinMaxNodes(max: boolean, max_nb_nodes: number) {
    let data_copy = Buffer.alloc(this.data.length);
    this.data.copy(data_copy);

    let minMaxNodes: LeafNode[] = [];

    // Perform a minMax descent, keeping track of parent innner node
    // and deleting found leafNode from the data_copy tree.
    // Iterate max_nb_nodes times to find the minMaxnodes.
    for (let i = 0; i++; i < max_nb_nodes) {
      let parentPointer: number;
      let parentNode: InnerNode;
      let grandParentPointer = -1;

      let pointer = this.header.rootNode;
      let critBit = max ? 1 : 0;
      let direction = critBit;
      let gpre_direction = critBit; // grand parent direction

      let offset = SlabHeader.LEN + pointer * this.slotSize;
      // Parse root node
      let node = parseNode(
        this.callBackInfoLen,
        this.data.slice(offset, offset + this.slotSize)
      );

      if (node instanceof LeafNode) {
        minMaxNodes.push(node);
        return minMaxNodes;
      }

      if (node instanceof InnerNode) {
        parentNode = node;
        parentPointer = pointer;

        while (true) {
          let offset = SlabHeader.LEN + pointer * this.slotSize;
          let node = parseNode(
            this.callBackInfoLen,
            this.data.slice(offset, offset + this.slotSize)
          );

          if (node instanceof LeafNode) {
            minMaxNodes.push(node);
            // Cut the found leaf node from the tree
            if (parentNode.children[(direction + 1) % 2] == 0) {
              // Cutting the last child of an inner node
              if (grandParentPointer === -1) {
                // Cutting the last leaf child of the root
                return minMaxNodes;
              } else {
                // Cutting the parent directly
                let grandParentOffset =
                  SlabHeader.LEN +
                  grandParentPointer * this.slotSize +
                  InnerNode.CHILD_OFFSET +
                  InnerNode.CHILD_SIZE * gpre_direction;
                data_copy.fill(
                  0,
                  grandParentOffset,
                  grandParentOffset + InnerNode.CHILD_SIZE
                );
              }
            } else {
              // Cutting the leafnode
              let parentOffset =
                SlabHeader.LEN +
                parentPointer * this.slotSize +
                InnerNode.CHILD_OFFSET +
                InnerNode.CHILD_SIZE * direction;
              data_copy.fill(
                0,
                parentOffset,
                parentOffset + InnerNode.CHILD_SIZE
              );
            }

            break;
          }

          if (node instanceof InnerNode) {
            gpre_direction = direction;
            if (!pointer) {
              direction = (critBit + 1) % 2;
            } else {
              direction = critBit;
            }
            pointer = node.children[direction];
            grandParentPointer = parentPointer;
            parentPointer = pointer;
            parentNode = node;
          }
        }
      }
    }
  }
}
