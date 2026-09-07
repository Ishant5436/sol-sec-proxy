use std::env;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub upstream_rpc_url: String,
    pub host: String,
    pub port: u16,
    pub cu_safety_buffer_percent: u32,
    pub max_cu_limit: u32,
    pub timeout_ms: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            upstream_rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8899,
            cu_safety_buffer_percent: 15,
            max_cu_limit: 1_400_000,
            timeout_ms: 5_000,
        }
    }
}

impl ProxyConfig {
    pub fn from_env() -> Result<Self, String> {
        let _ = dotenvy::dotenv();

        let upstream_rpc_url = env::var("SOLANA_UPSTREAM_RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
        if !upstream_rpc_url.starts_with("http://") && !upstream_rpc_url.starts_with("https://") {
            return Err(
                "Invalid upstream RPC URL: must start with http:// or https://".to_string(),
            );
        }

        let host = env::var("PROXY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = match env::var("PROXY_PORT") {
            Ok(val) => val
                .parse::<u16>()
                .map_err(|e| format!("Invalid PROXY_PORT: {}", e))?,
            Err(_) => 8899,
        };

        let cu_safety_buffer_percent = match env::var("CU_SAFETY_BUFFER_PERCENT") {
            Ok(val) => {
                let pct = val
                    .parse::<u32>()
                    .map_err(|e| format!("Invalid CU_SAFETY_BUFFER_PERCENT: {}", e))?;
                if pct > 100 {
                    return Err("CU safety buffer percent cannot exceed 100%".to_string());
                }
                pct
            }
            Err(_) => 15,
        };

        let max_cu_limit = match env::var("MAX_CU_LIMIT") {
            Ok(val) => {
                let limit = val
                    .parse::<u32>()
                    .map_err(|e| format!("Invalid MAX_CU_LIMIT: {}", e))?;
                if limit == 0 || limit > 1_400_000 {
                    return Err("MAX_CU_LIMIT must be between 1 and 1,400,000".to_string());
                }
                limit
            }
            Err(_) => 1_400_000,
        };

        let timeout_ms = match env::var("RPC_TIMEOUT_MS") {
            Ok(val) => val
                .parse::<u64>()
                .map_err(|e| format!("Invalid RPC_TIMEOUT_MS: {}", e))?,
            Err(_) => 5_000,
        };

        assert!(!upstream_rpc_url.is_empty(), "upstream RPC URL invariant");
        assert!(port > 0, "port invariant");

        Ok(Self {
            upstream_rpc_url,
            host,
            port,
            cu_safety_buffer_percent,
            max_cu_limit,
            timeout_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = ProxyConfig::default();
        assert_eq!(cfg.port, 8899);
        assert_eq!(cfg.cu_safety_buffer_percent, 15);
        assert_eq!(cfg.max_cu_limit, 1_400_000);
        assert!(cfg.upstream_rpc_url.starts_with("https://"));
    }

    #[test]
    fn test_config_invariants() {
        let cfg = ProxyConfig {
            port: 8080,
            cu_safety_buffer_percent: 20,
            ..ProxyConfig::default()
        };
        assert!(cfg.port > 0);
        assert!(cfg.cu_safety_buffer_percent <= 100);
        assert!(cfg.max_cu_limit <= 1_400_000);
    }
}
