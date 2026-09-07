"""
sol-sec-proxy Client Integration Example (Python)

Demonstrates sending transactions through sol-sec-proxy using standard
JSON-RPC 2.0 calls over HTTP.
"""

import json
import os
import urllib.request
import urllib.error

PROXY_URL = os.getenv("SOLANA_PROXY_URL", "http://127.0.0.1:8899")


def call_rpc(method: str, params: list) -> dict:
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    }
    req = urllib.request.Request(
        PROXY_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )

    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8")
        try:
            return json.loads(body)
        except Exception:
            return {"error": {"code": e.code, "message": body}}


def main():
    print(f"Connecting to sol-sec-proxy at: {PROXY_URL}")

    # 1. Transparent read query: getLatestBlockhash
    print("Fetching latest blockhash via proxy...")
    res = call_rpc("getLatestBlockhash", [{"commitment": "confirmed"}])
    if "result" in res:
        blockhash = res["result"]["value"]["blockhash"]
        print(f"Latest Blockhash: {blockhash}")
    else:
        print(f"Blockhash query response: {res}")

    # 2. Transparent simulation test: sendTransaction interception
    # Sample synthetic raw transaction payload (base64)
    dummy_raw_tx = "AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAEDAgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    print("\nBroadcasting transaction via sendTransaction through sol-sec-proxy...")
    tx_res = call_rpc(
        "sendTransaction",
        [dummy_raw_tx, {"encoding": "base64", "skipPreflight": False}],
    )

    if "error" in tx_res:
        err = tx_res["error"]
        print("\n[FIREWALL RESPONSE]")
        print(f"RPC Error Code: {err.get('code')}")
        print(f"Message: {err.get('message')}")
        if "data" in err and isinstance(err["data"], dict):
            diag = err["data"].get("decoded_error", {})
            print(f"Category: {diag.get('category')}")
            print(f"Error Name: {diag.get('error_name')}")
            print(f"Diagnostic: {diag.get('message')}")
            avoided = err["data"].get("avoided_wasted_fee_lamports", 0)
            print(f"Saved Fee (Avoided Capital Loss): {avoided} lamports")
    else:
        print(f"Transaction Result: {tx_res}")


if __name__ == "__main__":
    main()
