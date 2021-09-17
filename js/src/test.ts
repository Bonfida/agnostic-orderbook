import { Connection, PublicKey } from "@solana/web3.js";

import { Slab, SlabHeader } from "./slab";

const URL = "https://api.devnet.solana.com";

const connection = new Connection(URL);

const test = async () => {
  const accountInfo = await connection.getAccountInfo(
    new PublicKey("2z5uy4RNtXrEYwgTuXvosnW2MAUrMPtKmr2bqHGPiwQb")
  );

  if (!accountInfo?.data) {
    return;
  }

  const { data } = accountInfo;

  const slab = new Slab({
    header: SlabHeader.deserialize(data.slice(0, SlabHeader.LEN)),
    callBackInfoLen: 33,
    data,
  });
  console.log("Test", slab.getL2Depth(10, false));
};

test();
