use crate::config::ProxyConfig;
use crate::interceptor::{InterceptionOutcome, SimulationInterceptor};
use crate::rpc_client::SolanaRpcClient;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::CONTENT_TYPE;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct ProxyServer {
    config: ProxyConfig,
    interceptor: Arc<SimulationInterceptor>,
    rpc_client: Arc<SolanaRpcClient>,
}

impl ProxyServer {
    pub fn new(config: ProxyConfig) -> Self {
        assert!(config.port > 0, "Port must be non-zero");
        assert!(
            !config.upstream_rpc_url.is_empty(),
            "Upstream URL invariant"
        );

        let interceptor = Arc::new(SimulationInterceptor::new(config.clone()));
        let rpc_client = Arc::new(SolanaRpcClient::new(
            &config.upstream_rpc_url,
            config.timeout_ms,
        ));

        Self {
            config,
            interceptor,
            rpc_client,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        assert!(self.config.port > 0, "Port invariant in run");
        assert!(!self.config.host.is_empty(), "Host invariant in run");

        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        println!("[sol-sec-proxy] Listening on http://{}", addr);
        println!(
            "[sol-sec-proxy] Relaying to upstream: {}",
            self.config.upstream_rpc_url
        );

        let auto_server = auto::Builder::new(TokioExecutor::new());

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);

            let interceptor = Arc::clone(&self.interceptor);
            let rpc_client = Arc::clone(&self.rpc_client);

            let server_clone = auto_server.clone();
            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let interceptor = Arc::clone(&interceptor);
                    let rpc_client = Arc::clone(&rpc_client);
                    async move { handle_request(req, interceptor, rpc_client).await }
                });

                if let Err(err) = server_clone.serve_connection(io, service).await {
                    eprintln!("[sol-sec-proxy] Connection error: {:?}", err);
                }
            });
        }
    }
}

pub async fn handle_request(
    req: Request<Incoming>,
    interceptor: Arc<SimulationInterceptor>,
    rpc_client: Arc<SolanaRpcClient>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    assert!(
        Arc::strong_count(&interceptor) > 0,
        "Interceptor reference count invariant"
    );
    assert!(
        Arc::strong_count(&rpc_client) > 0,
        "RPC client reference count invariant"
    );

    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(boxed_body("Only POST JSON-RPC requests are supported"))
            .unwrap());
    }

    let whole_body = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(boxed_body("Failed to read request body"))
                .unwrap());
        }
    };

    let json_req: serde_json::Value = match serde_json::from_slice(&whole_body) {
        Ok(v) => v,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(boxed_body("Malformed JSON payload"))
                .unwrap());
        }
    };

    let method = json_req
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let req_id = json_req.get("id").cloned().unwrap_or(serde_json::json!(1));

    if method == "sendTransaction" {
        let resp_val = handle_send_transaction(&json_req, req_id, &interceptor, &rpc_client).await;
        let bytes = serde_json::to_vec(&resp_val).unwrap_or_default();
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(boxed_body(bytes))
            .unwrap());
    }

    // Default: Transparent upstream proxy
    match rpc_client.forward_raw_json_rpc(&json_req).await {
        Ok(upstream_resp) => {
            let bytes = serde_json::to_vec(&upstream_resp).unwrap_or_default();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, "application/json")
                .body(boxed_body(bytes))
                .unwrap())
        }
        Err(err) => {
            let err_payload = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {
                    "code": -32000,
                    "message": format!("Upstream relay error: {}", err)
                }
            });
            let bytes = serde_json::to_vec(&err_payload).unwrap_or_default();
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header(CONTENT_TYPE, "application/json")
                .body(boxed_body(bytes))
                .unwrap())
        }
    }
}

async fn handle_send_transaction(
    json_req: &serde_json::Value,
    req_id: serde_json::Value,
    interceptor: &SimulationInterceptor,
    rpc_client: &SolanaRpcClient,
) -> serde_json::Value {
    assert!(json_req.is_object(), "JSON request invariant");
    assert!(!req_id.is_null(), "Request ID invariant");

    let encoded_tx = json_req
        .get("params")
        .and_then(|p| p.get(0))
        .and_then(|t| t.as_str());

    let encoded_str = match encoded_tx {
        Some(s) if !s.is_empty() => s,
        _ => {
            return serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": { "code": -32602, "message": "Invalid params: missing encoded transaction" }
            });
        }
    };

    match interceptor.process_transaction(encoded_str).await {
        Ok((_, InterceptionOutcome::Reverted(rev))) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {
                    "code": -32002,
                    "message": format!("Pre-flight firewall: {}", rev.decoded_error.message),
                    "data": {
                        "category": rev.decoded_error.category,
                        "errorName": rev.decoded_error.error_name,
                        "instructionIndex": rev.decoded_error.instruction_index,
                        "avoidedWastedFeeLamports": rev.avoided_wasted_fee_lamports,
                        "logs": rev.logs
                    }
                }
            })
        }
        Ok((_, InterceptionOutcome::Passed(_))) => rpc_client
            .forward_raw_json_rpc(json_req)
            .await
            .unwrap_or_else(|e| {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": { "code": -32000, "message": format!("Upstream forward error: {}", e) }
                })
            }),
        Err(e) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": { "code": -32603, "message": format!("Pre-flight simulation error: {}", e) }
            })
        }
    }
}

fn boxed_body<B: Into<Bytes>>(body: B) -> BoxBody<Bytes, Infallible> {
    Full::new(body.into()).boxed()
}
