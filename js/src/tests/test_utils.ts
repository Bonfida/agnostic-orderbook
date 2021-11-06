import * as web3 from "@solana/web3.js";
import * as path from "path";
import { readFileSync, writeSync, closeSync } from "fs";
import { ChildProcess, spawn, execSync } from "child_process";
import tmp from "tmp";

// Spawns a local solana test validator. Caller is responsible for killing the
// process.
export async function spawnLocalSolana(): Promise<ChildProcess> {
  const ledger = tmp.dirSync();
  return spawn("solana-test-validator", ["-l", ledger.name]);
}

// Returns a keypair and key file name.
export function initializePayer(): [web3.Keypair, string] {
  const key = new web3.Keypair();
  const tmpobj = tmp.fileSync();
  writeSync(tmpobj.fd, JSON.stringify(Array.from(key.secretKey)));
  closeSync(tmpobj.fd);
  return [key, tmpobj.name];
}

// Deploys the agnostic order book program. Fees are paid with the fee payer
// whose key is in the given key file.
export function deployProgram(payerKeyFile: string): web3.PublicKey {
  const programDirectory = path.join(
    path.dirname(__filename),
    "../../../program"
  );
  const agnosticOrderbookSo = path.join(
    programDirectory,
    "target/deploy/agnostic_orderbook.so"
  );
  const keyfile = path.join(
    path.dirname(agnosticOrderbookSo),
    "agnostic_orderbook-keypair.json"
  );
  execSync("cargo build-bpf", { cwd: programDirectory });
  const bytes = readFileSync(keyfile, "utf-8");
  const keypair = web3.Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(bytes))
  );
  execSync(
    [
      "solana program deploy",
      agnosticOrderbookSo,
      "--program-id",
      keyfile,
      "-u localhost",
      "-k",
      payerKeyFile,
      "--commitment finalized",
    ].join(" ")
  );
  spawn("solana", ["logs", "-u", "localhost"], { stdio: "inherit" });
  return keypair.publicKey;
}

// Funds the given account. Sleeps until the connection is ready.
export async function airdropPayer(
  connection: web3.Connection,
  key: web3.PublicKey
) {
  while (true) {
    try {
      const signature = await connection.requestAirdrop(
        key,
        100 * web3.LAMPORTS_PER_SOL
      );
      await connection.confirmTransaction(signature, "finalized");
      return;
    } catch (e) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
      continue;
    }
  }
}
