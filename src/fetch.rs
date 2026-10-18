//! Block ingestion: trace + receipts per block, raw gzip on disk, sha256 manifest,
//! extracted rows. Raw files already on disk are reused, so a rerun costs nothing.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::extract::{extract_block_with, TxRow, V4Mode};
use crate::rpc::{parse_json, Rpc};

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks a `sha256sum`-style seal file (`<hex>  <path>` per line). Paths are relative
/// to the repository root `root`.
pub fn verify_seal(seal: &Path, root: &Path) -> Result<()> {
    let text =
        fs::read_to_string(seal).with_context(|| format!("no seal at {}", seal.display()))?;
    let mut checked = 0;
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let (hash, path) = line
            .split_once("  ")
            .with_context(|| format!("bad seal line: {line}"))?;
        let bytes = fs::read(root.join(path.trim()))
            .with_context(|| format!("sealed file missing: {path}"))?;
        if sha256_hex(&bytes) != hash.trim() {
            bail!("sealed file changed since sealing: {path}");
        }
        checked += 1;
    }
    if checked == 0 {
        bail!("seal file lists no files");
    }
    Ok(())
}

/// Post-seal gate: the preregistration and the block sample must still match the seal;
/// any other sealed file (code, lockfile, theory) may have changed. Returns the paths of
/// the sealed files that changed or are missing, so the caller can report the deviation.
pub fn verify_seal_post(seal: &Path, root: &Path) -> Result<Vec<String>> {
    let text =
        fs::read_to_string(seal).with_context(|| format!("no seal at {}", seal.display()))?;
    let (mut changed, mut checked) = (Vec::new(), 0);
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let (hash, path) = line
            .split_once("  ")
            .with_context(|| format!("bad seal line: {line}"))?;
        let path = path.trim();
        let intact = fs::read(root.join(path)).is_ok_and(|b| sha256_hex(&b) == hash.trim());
        if !intact {
            if path.contains("preregistro") || path.contains("muestra_bloques") {
                bail!("preregistro or sample changed since sealing (or missing): {path}");
            }
            changed.push(path.to_string());
        }
        checked += 1;
    }
    if checked == 0 {
        bail!("seal file lists no files");
    }
    Ok(changed)
}

fn write_gz(path: &Path, body: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let mut enc = GzEncoder::new(File::create(&tmp)?, Compression::default());
    enc.write_all(body.as_bytes())?;
    enc.finish()?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn read_gz(path: &Path) -> Result<String> {
    let mut s = String::new();
    GzDecoder::new(File::open(path)?).read_to_string(&mut s)?;
    Ok(s)
}

/// Returns the raw body for `kind` of `block`, from disk if present, else from RPC.
fn raw(rpc: Option<&Rpc>, dir: &Path, block: u64, kind: &str) -> Result<(String, bool)> {
    let path = dir.join(format!("{block}.{kind}.json.gz"));
    if path.exists() {
        return Ok((read_gz(&path)?, false));
    }
    let Some(rpc) = rpc else {
        bail!("block {block}: {kind} not on disk and no RPC configured");
    };
    let tag = format!("0x{block:x}");
    let (body, _) = match kind {
        "trace" => rpc.call(
            "debug_traceBlockByNumber",
            json!([tag, {"tracer": "callTracer"}]),
        )?,
        "receipts" => rpc.call("eth_getBlockReceipts", json!([tag]))?,
        _ => bail!("unknown kind {kind}"),
    };
    write_gz(&path, &body)?;
    Ok((body, true))
}

fn result_of(body: &str) -> Result<Value> {
    let mut v = parse_json(body)?;
    Ok(v.get_mut("result").map(Value::take).unwrap_or(Value::Null))
}

/// Rows, manifest entries and number of RPC calls made for one block.
type BlockResult = (Vec<TxRow>, Vec<(u64, String, String)>, usize);

pub struct FetchOutcome {
    pub rows: Vec<TxRow>,
    pub manifest: Vec<(u64, String, String)>,
    pub fetched_calls: usize,
    pub failures: Vec<(u64, String)>,
}

/// `pool_manager`: lowercase PoolManager address to detect Uniswap v4 pools, or `None`
/// for the address-only pools of the original extractor.
pub fn fetch_blocks(
    rpc: Option<Rpc>,
    raw_dir: &Path,
    blocks: &[u64],
    workers: usize,
    pool_manager: Option<&str>,
) -> Result<FetchOutcome> {
    fs::create_dir_all(raw_dir)?;
    let queue: Arc<Mutex<std::vec::IntoIter<u64>>> =
        Arc::new(Mutex::new(Vec::from(blocks).into_iter()));
    let out = Arc::new(Mutex::new(FetchOutcome {
        rows: Vec::new(),
        manifest: Vec::new(),
        fetched_calls: 0,
        failures: Vec::new(),
    }));
    let rpc = Arc::new(rpc);
    let raw_dir: Arc<PathBuf> = Arc::new(raw_dir.to_path_buf());
    let pool_manager: Arc<Option<String>> = Arc::new(pool_manager.map(str::to_ascii_lowercase));
    let total = blocks.len();
    let done = Arc::new(Mutex::new(0usize));

    let handles: Vec<_> = (0..workers.max(1))
        .map(|_| {
            let (queue, out, rpc, raw_dir, done, pool_manager) = (
                queue.clone(),
                out.clone(),
                rpc.clone(),
                raw_dir.clone(),
                done.clone(),
                pool_manager.clone(),
            );
            // Dropping a very deep `Value` recurses, so workers get a roomy stack.
            thread::Builder::new()
                .stack_size(256 << 20)
                .spawn(move || loop {
                    let Some(block) = queue.lock().expect("queue").next() else {
                        break;
                    };
                    let res = (|| -> Result<BlockResult> {
                        let (trace, t_new) = raw(rpc.as_ref().as_ref(), &raw_dir, block, "trace")?;
                        let (rcpt, r_new) =
                            raw(rpc.as_ref().as_ref(), &raw_dir, block, "receipts")?;
                        let v4 = pool_manager
                            .as_deref()
                            .map(|pm| V4Mode { pool_manager: pm });
                        let rows =
                            extract_block_with(block, &result_of(&trace)?, &result_of(&rcpt)?, v4)?;
                        let manifest = vec![
                            (block, "trace".to_string(), sha256_hex(trace.as_bytes())),
                            (block, "receipts".to_string(), sha256_hex(rcpt.as_bytes())),
                        ];
                        Ok((rows, manifest, t_new as usize + r_new as usize))
                    })();
                    let mut o = out.lock().expect("out");
                    match res {
                        Ok((rows, manifest, calls)) => {
                            o.rows.extend(rows);
                            o.manifest.extend(manifest);
                            o.fetched_calls += calls;
                        }
                        Err(e) => o.failures.push((block, format!("{e:#}"))),
                    }
                    drop(o);
                    let mut d = done.lock().expect("done");
                    *d += 1;
                    if *d % 50 == 0 || *d == total {
                        eprintln!("fetch: {}/{} blocks", *d, total);
                    }
                })
                .expect("spawn worker")
        })
        .collect();
    for h in handles {
        h.join().expect("worker panicked");
    }
    let mut o = Arc::try_unwrap(out)
        .map_err(|_| anyhow::anyhow!("workers still hold results"))?
        .into_inner()
        .expect("out");
    o.rows.sort_by_key(|r| (r.block, r.tx_index));
    o.manifest.sort();
    o.failures.sort();
    Ok(o)
}
