/**
 * sol-sec-proxy Client Integration Example (TypeScript)
 *
 * Demonstrates routing Solana transactions through sol-sec-proxy
 * for automatic pre-execution firewalling, priority fee savings,
 * and structured Anchor error diagnostics.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionMessage,
  VersionedTransaction,
  ComputeBudgetProgram,
} from "@solana/web3.js";

// Point the connection to the local or sidecar sol-sec-proxy daemon
const PROXY_RPC_URL = process.env.SOLANA_PROXY_URL || "http://127.0.0.1:8899";
const connection = new Connection(PROXY_RPC_URL, "confirmed");

interface InterceptedSimulationRevert {
  code: number;
  message: string;
  data: {
    is_valid: boolean;
    decoded_error: {
      instruction_index: number | null;
      error_code: number;
      category: string;
      error_name: string;
      message: string;
      logs_snippet: string[];
    };
    avoided_wasted_fee_lamports: number;
    logs: string[];
  };
}

async function runExample() {
  console.log(`Connecting to sol-sec-proxy at: ${PROXY_RPC_URL}`);

  // Generate ephemeral keypair for demonstration
  const payer = Keypair.generate();
  const recipient = Keypair.generate().publicKey;

  console.log(`Payer Pubkey: ${payer.publicKey.toBase58()}`);

  // Fetch recent blockhash through proxy (proxied transparently to upstream RPC)
  const { blockhash } = await connection.getLatestBlockhash("confirmed");

  // Build sample transaction with priority fee and compute unit limit
  const instructions = [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 25_000 }),
    SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: recipient,
      lamports: 1_000_000, // 0.001 SOL
    }),
  ];

  const messageV0 = new TransactionMessage({
    payerKey: payer.publicKey,
    recentBlockhash: blockhash,
    instructions,
  }).compileToV0Message();

  const transaction = new VersionedTransaction(messageV0);
  transaction.sign([payer]);

  console.log("\nBroadcasting transaction through sol-sec-proxy...");

  try {
    const rawTx = transaction.serialize();
    const txSignature = await connection.sendRawTransaction(rawTx, {
      skipPreflight: false,
      maxRetries: 0,
    });

    console.log(`Transaction succeeded and broadcast to cluster!`);
    console.log(`Signature: ${txSignature}`);
  } catch (error: any) {
    // Check if intercepted and rejected by sol-sec-proxy
    if (error && error.message && error.message.includes("Transaction simulation failed")) {
      console.log("\n[FIREWALL TRIGGERED] Transaction intercepted by sol-sec-proxy before broadcast!");
      console.log("No base or priority fees were burned on-chain.");

      try {
        const errorDetails = JSON.parse(error.message);
        console.log(`Category: ${errorDetails.category || "SimulationRevert"}`);
        console.log(`Error Name: ${errorDetails.error_name || "Custom"}`);
        console.log(`Diagnostic: ${errorDetails.message}`);
        console.log(`Avoided Wasted Fee: ${errorDetails.avoided_wasted_fee_lamports || 0} lamports`);
      } catch {
        console.log(`Raw Error Output: ${error.message}`);
      }
    } else {
      console.error("Standard RPC or Network Error:", error);
    }
  }
}

if (require.main === module) {
  runExample().catch((err) => {
    console.error("Execution failed:", err);
    process.exit(1);
  });
}
