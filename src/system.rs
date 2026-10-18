//! System and builder transactions (dossier ADR 005 §1.3, rule D1/D2). The list is data
//! (`datos/tx_sistema.csv`, columns `cadena,rol,direccion,fuente`), not code: a row says
//! that on `cadena`, a transaction whose sender (`rol` = `remitente`) or destination
//! (`rol` = `destino`) is `direccion` is infrastructure and not part of the analysed
//! population. The filter runs before bots, entities and pools are defined.
//!
//! Matching is by address only: the rows file does not carry the selector, so a
//! destination entry excludes every call to that address (the entries of the list are
//! infrastructure contracts with no other use).

use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::chain::Chain;
use crate::extract::TxRow;

/// Default location of the list, relative to the working directory.
pub const DEFAULT_LIST: &str = "datos/tx_sistema.csv";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemFilter {
    senders: HashSet<String>,
    destinations: HashSet<String>,
}

fn valid_address(a: &str) -> bool {
    a.len() == 42 && a.starts_with("0x") && a[2..].bytes().all(|c| c.is_ascii_hexdigit())
}

impl SystemFilter {
    /// Parses the list text and keeps the entries of `chain`. Fails on an unknown chain,
    /// an unknown role or a malformed address, so a typo cannot silently disable a rule.
    pub fn parse(text: &str, chain: Chain) -> Result<SystemFilter> {
        let mut rd = csv::ReaderBuilder::new()
            .comment(Some(b'#'))
            .from_reader(text.as_bytes());
        let headers = rd.headers()?.clone();
        let col = |name: &str| {
            headers
                .iter()
                .position(|h| h == name)
                .with_context(|| format!("system list: missing column {name}"))
        };
        let (c_chain, c_role, c_addr) = (col("cadena")?, col("rol")?, col("direccion")?);
        let mut f = SystemFilter::default();
        for (i, rec) in rd.records().enumerate() {
            let rec = rec?;
            let line = i + 2;
            let name = rec[c_chain].trim();
            let Some(c) = Chain::parse(name) else {
                bail!("system list line {line}: unknown chain {name:?}");
            };
            let addr = rec[c_addr].trim().to_ascii_lowercase();
            if !valid_address(&addr) {
                bail!("system list line {line}: malformed address {addr:?}");
            }
            let role = rec[c_role].trim();
            let set = match role {
                "remitente" => &mut f.senders,
                "destino" => &mut f.destinations,
                _ => bail!("system list line {line}: unknown role {role:?} (remitente|destino)"),
            };
            if c == chain {
                set.insert(addr);
            }
        }
        Ok(f)
    }

    pub fn load(path: &Path, chain: Chain) -> Result<SystemFilter> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("no system list at {}", path.display()))?;
        Self::parse(&text, chain)
    }

    pub fn len(&self) -> usize {
        self.senders.len() + self.destinations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn matches(&self, r: &TxRow) -> bool {
        self.senders.contains(&r.from.to_ascii_lowercase())
            || self.destinations.contains(&r.to.to_ascii_lowercase())
    }

    /// Number of rows that are system transactions.
    pub fn count(&self, rows: &[TxRow]) -> usize {
        rows.iter().filter(|r| self.matches(r)).count()
    }

    /// Rows matched by each entry: `(role, address, rows)`, sorted. A row that matches two
    /// entries (sender and destination) counts under both.
    pub fn counts(&self, rows: &[TxRow]) -> Vec<(&'static str, String, usize)> {
        let mut out = Vec::new();
        for (role, set, field) in [
            ("remitente", &self.senders, true),
            ("destino", &self.destinations, false),
        ] {
            for a in set {
                let n = rows
                    .iter()
                    .filter(|r| {
                        let v = if field { &r.from } else { &r.to };
                        v.eq_ignore_ascii_case(a)
                    })
                    .count();
                out.push((role, a.clone(), n));
            }
        }
        out.sort();
        out
    }

    /// Rows that are not system transactions, and how many were dropped.
    pub fn apply(&self, rows: &[TxRow]) -> (Vec<TxRow>, usize) {
        let kept: Vec<TxRow> = rows.iter().filter(|r| !self.matches(r)).cloned().collect();
        let dropped = rows.len() - kept.len();
        (kept, dropped)
    }
}
