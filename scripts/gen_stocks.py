#!/usr/bin/env python3
"""Regenerate src/stock_list.rs from the official Coinbase Tokenized Stocks API.
Source: https://docs.base.org/sdks/tokenized-stocks/api-reference/list-tokenized-stocks"""
import json, urllib.request, datetime
toks, tok = [], None
while True:
    url = "https://api.coinbase.com/v1/tokenized-stocks?page_size=500" + (f"&page_token={tok}" if tok else "")
    d = json.load(urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "curl/8"})))
    toks += d["tokens"]; tok = d.get("next_page_token")
    if not tok: break
toks.sort(key=lambda t: t["contract_address"].lower())
out = ["//! Coinbase-issued tokenized stocks on Base (B20 Asset variant).", "//!",
       "//! GENERATED from the official Coinbase Tokenized Stocks API",
       "//! `GET https://api.coinbase.com/v1/tokenized-stocks?page_size=500`",
       "//! (documented at https://docs.base.org/sdks/tokenized-stocks/api-reference/list-tokenized-stocks),",
       f"//! fetched {datetime.date.today()}, {len(toks)} tokens. Cross-checked against https://www.base.org/stocks and",
       "//! https://docs.base.org/specifications/b20/tokenized-stocks-on-base.",
       "//! Regenerate with `scripts/gen_stocks.py`; tokens created later are also picked up",
       "//! automatically (see `is_coinbase_issuer`) and via the `extra_stock_tokens` param.", "",
       "/// (address lowercase hex without 0x, ticker, name, decimals, ISIN)",
       "pub const COINBASE_STOCKS: &[(&str, &str, &str, u32, &str)] = &["]
for t in toks:
    name = t["name"].replace('"', '\\"')
    out.append(f'    ("{t["contract_address"].lower()[2:]}", "{t["symbol"]}", "{name}", {t["decimals"]}, "{t.get("isin") or ""}"),')
out.append("];")
open("src/stock_list.rs", "w").write("\n".join(out) + "\n")
print(len(toks), "tokens")
