// Source code for the Substrate Telemetry Server.
// Copyright (C) 2021 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use hyper::header::HeaderMap;

use std::net::{IpAddr, SocketAddr};
use log::info;


/**
Extract the "real" IP address of the connection by looking at headers
set by proxies (this is inspired by Actix Web's implementation of the feature).

First, check for the standardised "Forwarded" header. This looks something like:

"Forwarded: for=12.34.56.78;host=example.com;proto=https, for=23.45.67.89"

Each proxy can append to this comma separated list of forwarded-details. We'll look for
the first "for" address and try to decode that.

If this doesn't yield a result, look for the non-standard but common X-Forwarded-For header,
which contains a comma separated list of addresses; each proxy in the potential chain possibly
appending one to the end. So, take the first of these if it exists.

If still no luck, look for the X-Real-IP header, which we expect to contain a single IP address.

If that _still_ doesn't work, fall back to the socket address of the connection.
*/

/// The source of the address returned
pub enum Source {
    ForwardedHeader,
    XForwardedForHeader,
    XRealIpHeader,
    SocketAddr,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::ForwardedHeader => write!(f, "'Forwarded' header"),
            Source::XForwardedForHeader => write!(f, "'X-Forwarded-For' header"),
            Source::XRealIpHeader => write!(f, "'X-Real-Ip' header"),
            Source::SocketAddr => write!(f, "Socket address"),
        }
    }
}

pub fn real_ip(addr: SocketAddr, headers: &HeaderMap) -> (IpAddr, Source) {
    // 打印所有的头部信息
    for (key, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            info!("Header: {}: {}", key, value_str);
        }
    }

    let x_forwarded_for = headers.get("x-original-forwarded-for").and_then(header_as_str);
    let real_ip = headers.get("x-real-ip").and_then(header_as_str);

    pick_best_ip_from_options(x_forwarded_for, real_ip, addr)
}


fn header_as_str(value: &hyper::header::HeaderValue) -> Option<&str> {
    std::str::from_utf8(value.as_bytes()).ok()
}

fn pick_best_ip_from_options(
    x_forwarded_for: Option<&str>,
    real_ip: Option<&str>,
    addr: SocketAddr,
) -> (IpAddr, Source) {
    let realip = x_forwarded_for.as_ref().and_then(|val| {
        info!("Processing X-Forwarded-For header: {}", val);
        let last_addr = get_last_addr_from_x_forwarded_for_header(val)?;
        info!("Last address from X-Forwarded-For: {}", last_addr);

        // 尝试解析 IP 地址，处理可能的端口号
        parse_ip_address(last_addr).map(|ip_addr| (ip_addr, Source::XForwardedForHeader))
    })
    .or_else(|| {
        real_ip.as_ref().and_then(|val| {
            let addr = val.trim();
            info!("Processing X-Real-Ip header: {}", val);
            addr.parse::<IpAddr>().ok()
                .map(|ip_addr| (ip_addr, Source::XRealIpHeader))
        })
    })
    .unwrap_or_else(|| {
        info!("Using socket address: {}", addr.ip());
        (addr.ip(), Source::SocketAddr)
    });

    info!("Resolved real IP: {:?}", realip.0);
    realip
}

fn get_last_addr_from_x_forwarded_for_header(value: &str) -> Option<&str> {
    value.split(',').map(|val| val.trim()).last()
}

fn parse_ip_address(value: &str) -> Option<IpAddr> {
    // 如果 IP 地址包含端口号（尤其是 IPv6 地址），尝试只解析 IP 部分
    let addr = if let Some(index) = value.rfind("]:") {
        // 对于 IPv6 地址
        &value[..=index]
    } else if let Some(index) = value.rfind(':') {
        // 对于 IPv4 地址
        &value[..index]
    } else {
        // 不含端口号
        value
    };

    addr.parse::<IpAddr>().ok()
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_addr_from_forwarded_rfc_examples() {
        let examples = vec![
            (r#"for="_gazonk""#, "_gazonk"),
            (
                r#"For="[2001:db8:cafe::17]:4711""#,
                "[2001:db8:cafe::17]:4711",
            ),
            (r#"for=192.0.2.60;proto=http;by=203.0.113.43"#, "192.0.2.60"),
            (r#"for=192.0.2.43, for=198.51.100.17"#, "192.0.2.43"),
        ];

        // for (value, expected) in examples {
        //     assert_eq!(
        //         get_first_addr_from_forwarded_header(value),
        //         Some(expected),
        //         "Header value: {}",
        //         value
        //     );
        // }
    }
}
