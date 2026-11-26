use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Core {
    Xray,
    Sudoku,
}

impl Core {
    pub fn name(self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::Sudoku => "sudoku",
        }
    }
}

pub fn parse(link: &str, local_port: u16) -> Result<(Core, Value), &'static str> {
    if link.is_empty() || link.len() > 16384 {
        return Err("connection key is missing or too long");
    }
    if let Some(encoded) = link.strip_prefix("sudoku://") {
        return sudoku(encoded, local_port).map(|config| (Core::Sudoku, config));
    }
    xray(link, local_port).map(|config| (Core::Xray, config))
}

fn xray(link: &str, local_port: u16) -> Result<Value, &'static str> {
    const INVALID_LINK: &str = "a vless:// or sudoku:// connection key is required";
    let uri = url::Url::parse(link).map_err(|_| INVALID_LINK)?;
    let host = uri
        .host_str()
        .filter(|s| !s.is_empty())
        .ok_or(INVALID_LINK)?;
    let host = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if uri.scheme() != "vless" || uri.username().is_empty() {
        return Err(INVALID_LINK);
    }
    let id = percent_encoding::percent_decode_str(uri.username()).decode_utf8_lossy();
    let port = uri
        .port()
        .filter(|&p| p != 0)
        .ok_or("connection port must be 1–65535")?;
    let query: Vec<_> = uri.query_pairs().collect();
    let option = |name| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_ref())
    };

    let network = match option("type") {
        "" => "tcp",
        "splithttp" => "xhttp",
        network => network,
    };
    let security = or_default(option("security"), "none");
    let mut stream = json!({"network": network, "security": security});
    let sni = or_default(option("sni"), host);
    match security {
        "none" => {}
        "tls" => {
            let mut tls = json!({"serverName": sni, "allowInsecure": false});
            if !option("fp").is_empty() {
                tls["fingerprint"] = json!(option("fp"));
            }
            if !option("alpn").is_empty() {
                tls["alpn"] = json!(option("alpn").split(',').collect::<Vec<_>>());
            }
            stream["tlsSettings"] = tls;
        }
        "reality" => {
            if option("pbk").is_empty() {
                return Err("Reality public key is missing");
            }
            stream["realitySettings"] = json!({
                "serverName": sni,
                "fingerprint": or_default(option("fp"), "chrome"),
                "publicKey": option("pbk"),
                "shortId": option("sid"),
                "spiderX": option("spx"),
            });
        }
        _ => return Err("unsupported VLESS security"),
    }
    match network {
        "tcp" | "raw" => {}
        "xhttp" => {
            let mut settings = json!({
                "mode": or_default(option("mode"), "auto"),
                "host": option("host"),
                "path": option("path"),
            });
            let extra = option("extra");
            if !extra.is_empty() {
                let extra: Value =
                    serde_json::from_str(extra).map_err(|_| "invalid XHTTP extra JSON")?;
                if !extra.is_object() && !extra.is_null() {
                    return Err("invalid XHTTP extra JSON");
                }
                settings["extra"] = extra;
            }
            stream["xhttpSettings"] = settings;
        }
        "ws" => {
            stream["wsSettings"] =
                json!({"path": option("path"), "headers": {"Host": option("host")}});
        }
        "grpc" => {
            stream["grpcSettings"] = json!({
                "serviceName": option("serviceName"),
                "multiMode": option("mode") == "multi",
            });
        }
        _ => return Err("unsupported VLESS transport"),
    }

    let mut user = json!({"id": id, "encryption": or_default(option("encryption"), "none")});
    if !option("flow").is_empty() {
        user["flow"] = json!(option("flow"));
    }
    Ok(json!({
        "log": {"loglevel": "warning"},
        "inbounds": [{
            "listen": "127.0.0.1",
            "port": local_port,
            "protocol": "socks",
            "settings": {"auth": "noauth", "udp": false},
        }],
        "outbounds": [{
            "tag": "proxy",
            "protocol": "vless",
            "settings": {"vnext": [{"address": host, "port": port, "users": [user]}]},
            "streamSettings": stream,
        }],
    }))
}

fn sudoku(encoded: &str, local_port: u16) -> Result<Value, &'static str> {
    let encoded = encoded.replace(['\r', '\n'], "");
    let bytes = [
        general_purpose::URL_SAFE_NO_PAD,
        general_purpose::URL_SAFE,
        general_purpose::STANDARD_NO_PAD,
        general_purpose::STANDARD,
    ]
    .iter()
    .find_map(|encoding| encoding.decode(&encoded).ok())
    .ok_or("invalid Sudoku Base64 key")?;

    const INVALID_KEY: &str = "Sudoku key must contain server, port and key";
    let key: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID_KEY)?;
    let string = |name: &str| key[name].as_str().unwrap_or("");
    let boolean = |name: &str| key[name].as_bool().unwrap_or(false);
    let host = string("h");
    let port = key["p"].as_f64().map(|p| p.trunc()).unwrap_or(0.0);
    let secret = string("k");
    if host.is_empty() || !(1.0..=65535.0).contains(&port) || secret.is_empty() {
        return Err(INVALID_KEY);
    }
    if host.len() > 253 || host.contains(['/', '\\', '\0', '\r', '\n']) {
        return Err("invalid Sudoku host");
    }
    if hex::decode(secret).is_err() {
        return Err("invalid Sudoku private key");
    }
    let port = port as u16;
    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let ascii = match string("a") {
        "" | "entropy" => "prefer_entropy",
        "ascii" => "prefer_ascii",
        value => value,
    };
    let tables = if key["ts"].is_null() {
        json!([])
    } else {
        key["ts"].clone()
    };
    Ok(json!({
        "mode": "client",
        "transport": "tcp",
        "local_port": local_port,
        "server_address": address,
        "key": secret,
        "aead": or_default(string("e"), "none"),
        "padding_min": 5,
        "padding_max": 15,
        "ascii": ascii,
        "enable_pure_downlink": !boolean("x"),
        "custom_table": string("t"),
        "custom_tables": tables,
        "proxy_mode": "global",
        "rule_urls": ["global"],
        "httpmask": {
            "disable": boolean("hd"),
            "mode": string("hm"),
            "tls": boolean("ht"),
            "host": string("hh"),
            "multiplex": string("hx"),
            "path_root": string("hy"),
        },
    }))
}

fn or_default<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() {
        default
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xray_keeps_the_listener_local_and_supports_stable_transports() {
        for transport in ["tcp", "raw", "ws", "grpc", "xhttp", "splithttp"] {
            let key = format!("vless://test%2Did@[::1]:443?security=tls&type={transport}&path=%2Fvpn&alpn=h2,http%2F1.1");
            let (core, config) = parse(&key, 17891).unwrap();
            assert_eq!(core, Core::Xray);
            assert_eq!(config["inbounds"][0]["listen"], "127.0.0.1");
            assert_eq!(config["inbounds"][0]["settings"]["udp"], false);
            assert_eq!(
                config["outbounds"][0]["settings"]["vnext"][0]["address"],
                "::1"
            );
            assert_eq!(
                config["outbounds"][0]["settings"]["vnext"][0]["users"][0]["id"],
                "test-id"
            );
            let stream = &config["outbounds"][0]["streamSettings"];
            assert_eq!(
                stream["network"],
                if transport == "splithttp" {
                    "xhttp"
                } else {
                    transport
                }
            );
            assert_eq!(stream["tlsSettings"]["allowInsecure"], false);
            assert_eq!(stream["tlsSettings"]["alpn"], json!(["h2", "http/1.1"]));
        }
    }

    #[test]
    fn sudoku_accepts_stable_key_encodings_and_http_mask_options() {
        let source = json!({"h":"::1","p":443,"k":"ab".repeat(32),"a":"ascii","x":true,"ht":true,"hm":"ws","hx":"on"});
        for encoding in [
            general_purpose::URL_SAFE_NO_PAD,
            general_purpose::URL_SAFE,
            general_purpose::STANDARD_NO_PAD,
            general_purpose::STANDARD,
        ] {
            let (core, config) = parse(
                &format!("sudoku://{}", encoding.encode(source.to_string())),
                17891,
            )
            .unwrap();
            assert_eq!(core, Core::Sudoku);
            assert_eq!(config["local_port"], 17891);
            assert_eq!(config["server_address"], "[::1]:443");
            assert_eq!(config["ascii"], "prefer_ascii");
            assert_eq!(config["enable_pure_downlink"], false);
            assert_eq!(config["httpmask"]["mode"], "ws");
            assert_eq!(config["httpmask"]["tls"], true);
            assert_eq!(config["custom_tables"], json!([]));
        }
    }

    #[test]
    fn rejects_invalid_connection_keys() {
        for key in [
            "",
            "http://example.com",
            "vless://key@example.com:0",
            "vless://key@example.com:443?security=reality",
            "vless://key@example.com:443?type=xhttp&extra=123",
            "sudoku://bad",
        ] {
            assert!(parse(key, 17891).is_err());
        }
    }
}
