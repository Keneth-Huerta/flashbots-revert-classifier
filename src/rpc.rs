//! Minimal synchronous JSON-RPC client. The API key only lives in the URL and is never logged.

use std::io::Read;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::chain::Chain;

/// Parses JSON without serde_json's recursion limit (128). Deep call traces exceed it;
/// `serde_stacker` grows the stack on demand so depth is bounded by memory, not by the
/// native stack.
pub fn parse_json(text: &str) -> Result<Value> {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let v = Value::deserialize(serde_stacker::Deserializer::new(&mut de))?;
    de.end()?;
    Ok(v)
}

pub struct Rpc {
    url: String,
    agent: ureq::Agent,
}

impl Rpc {
    pub fn alchemy_base(api_key: &str) -> Self {
        Self::alchemy(Chain::Base, api_key)
    }

    pub fn alchemy(chain: Chain, api_key: &str) -> Self {
        Self::new(format!(
            "https://{}.g.alchemy.com/v2/{api_key}",
            chain.alchemy_network()
        ))
    }

    pub fn new(url: String) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(300))
            .build();
        Rpc { url, agent }
    }

    /// Calls `method` and returns the raw response body plus its parsed `result`.
    /// Retries transient failures (HTTP 429/5xx, transport errors) with backoff.
    pub fn call(&self, method: &str, params: Value) -> Result<(String, Value)> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let mut last = String::new();
        for attempt in 0..6u32 {
            if attempt > 0 {
                sleep(Duration::from_millis(500 * 2u64.pow(attempt)));
            }
            match self.agent.post(&self.url).send_json(body.clone()) {
                Ok(resp) => {
                    // A body cut off mid-read (timeout, reset) is transient: retry it.
                    // `into_string` caps the body at 10 MB, which giant blocks exceed
                    // (EXPL-14): read the reader to the end instead.
                    let mut text = String::new();
                    if let Err(e) = resp.into_reader().read_to_string(&mut text) {
                        last = format!("{method}: reading response failed ({})", e.kind());
                        continue;
                    }
                    let parsed = parse_json(&text)?;
                    if let Some(err) = parsed.get("error") {
                        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
                        // -32005 / 429-like: rate limited; anything else is final.
                        if code == -32005 || code == 429 {
                            last = format!("{method}: rate limited");
                            continue;
                        }
                        bail!("{method}: RPC error {err}");
                    }
                    let result = parsed
                        .get("result")
                        .cloned()
                        .ok_or_else(|| anyhow!("{method}: response without result"))?;
                    return Ok((text, result));
                }
                Err(ureq::Error::Status(code, _)) if code == 429 || code >= 500 => {
                    last = format!("{method}: HTTP {code}");
                }
                Err(ureq::Error::Status(code, _)) => bail!("{method}: HTTP {code}"),
                Err(ureq::Error::Transport(t)) => {
                    // Transport errors can echo the URL; keep only the kind.
                    last = format!("{method}: transport error ({:?})", t.kind());
                }
            }
        }
        bail!("{last} after retries")
    }
}
