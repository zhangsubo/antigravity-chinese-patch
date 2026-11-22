use crate::custom_dict::LoadedCustomDict;
use crate::patcher::{DESKTOP_RUNTIME_JS, PAYLOAD_SHA256, SWARM_RUNTIME_JS};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;
use tungstenite::{client, Message};

const CDP_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_HTTP_BODY: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct CdpTarget {
    #[allow(dead_code)]
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub ws_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    MainDesktop,
    SwarmFrame,
    Ignore,
}

#[derive(Debug, Clone, Default)]
pub struct PollCycleReport {
    #[allow(dead_code)]
    pub targets_seen: usize,
    pub main_injected: usize,
    pub main_refreshed: usize,
    pub swarm_injected: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub fn extract_embedded_fingerprint(script: &str, fn_prefix: &str) -> String {
    if let Some(pos) = script.rfind(fn_prefix) {
        let tail = &script[pos..];
        if let Some(fp_idx) = tail.find("fingerprint:") {
            let after = tail[fp_idx + "fingerprint:".len()..].trim_start();
            if let Some(rest) = after.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    return rest[..end].to_string();
                }
            }
        }
    }
    PAYLOAD_SHA256[..12].to_string()
}

pub fn desktop_fingerprint() -> String {
    extract_embedded_fingerprint(DESKTOP_RUNTIME_JS, "mountDesktopRuntime(__BUNDLE_DATA__")
}

pub fn swarm_fingerprint() -> String {
    extract_embedded_fingerprint(SWARM_RUNTIME_JS, "mountSwarmRuntime(__SWARM_DATA__")
}

pub fn sanitize_url_for_log(raw: &str) -> String {
    let no_frag = raw.split('#').next().unwrap_or(raw);
    let no_query = no_frag.split('?').next().unwrap_or(no_frag);
    if let Some((scheme, rest)) = no_query.split_once("://") {
        if let Some((host_port, path)) = rest.split_once('/') {
            let clean_host_port = host_port.rsplit('@').next().unwrap_or(host_port);
            return format!("{scheme}://{clean_host_port}/{path}");
        }
        let clean_host_port = rest.rsplit('@').next().unwrap_or(rest);
        return format!("{scheme}://{clean_host_port}");
    }
    no_query.to_string()
}

pub fn is_swarm_plugin_url(raw: &str) -> bool {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("plugin") {
        return false;
    }
    let before_frag = rest.split('#').next().unwrap_or(rest);
    let before_query = before_frag.split('?').next().unwrap_or(before_frag);
    let (authority, path) = match before_query.split_once('/') {
        Some((a, p)) => (a, p),
        None => (before_query, ""),
    };
    if authority.contains('@') || authority.contains(':') {
        return false;
    }
    if !path.is_empty() {
        return false;
    }
    let lower = authority.to_ascii_lowercase();
    let Some(suffix) = lower.strip_prefix("swarm--swarm_board-") else {
        return false;
    };
    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn is_local_desktop_origin(raw: &str) -> bool {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return false;
    };
    let scheme_lower = scheme.to_ascii_lowercase();
    if scheme_lower == "vscode-file" {
        return rest.starts_with("vscode-app/") || rest == "vscode-app";
    }
    if scheme_lower != "http" && scheme_lower != "https" {
        return false;
    }
    let before_frag = rest.split('#').next().unwrap_or(rest);
    let before_query = before_frag.split('?').next().unwrap_or(before_frag);
    let authority = before_query.split('/').next().unwrap_or(before_query);
    if authority.contains('@') {
        return false;
    }
    let host = if let Some(after_bracket) = authority.strip_prefix('[') {
        let Some((ipv6, tail)) = after_bracket.split_once(']') else {
            return false;
        };
        if !tail.is_empty() && !is_valid_port_suffix(tail) {
            return false;
        }
        ipv6.to_ascii_lowercase()
    } else if let Some((h, p)) = authority.split_once(':') {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        h.to_ascii_lowercase()
    } else {
        authority.to_ascii_lowercase()
    };

    host == "127.0.0.1" || host == "localhost" || host == "::1"
}

fn is_valid_port_suffix(suffix: &str) -> bool {
    let Some(port) = suffix.strip_prefix(':') else {
        return false;
    };
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
}

pub fn validate_ws_url(ws_url: &str, expected_port: u16) -> bool {
    let Some(rest) = ws_url.strip_prefix("ws://") else {
        return false;
    };
    let Some((host_port, path)) = rest.split_once('/') else {
        return false;
    };
    if host_port.contains('@') || !path.starts_with("devtools/") {
        return false;
    }
    let lower_host_port = host_port.to_ascii_lowercase();
    lower_host_port == format!("127.0.0.1:{expected_port}")
        || lower_host_port == format!("localhost:{expected_port}")
        || lower_host_port == format!("[::1]:{expected_port}")
}

pub fn classify_target(target: &CdpTarget, expected_port: u16) -> TargetKind {
    let Some(ws_url) = target.ws_url.as_deref() else {
        return TargetKind::Ignore;
    };
    if !validate_ws_url(ws_url, expected_port) {
        return TargetKind::Ignore;
    }
    if is_swarm_plugin_url(&target.url) {
        return TargetKind::SwarmFrame;
    }
    let allowed_type = matches!(target.target_type.as_str(), "page" | "app" | "webview");
    if allowed_type && is_local_desktop_origin(&target.url) {
        return TargetKind::MainDesktop;
    }
    TargetKind::Ignore
}

pub fn fetch_targets(port: u16) -> Result<Vec<CdpTarget>, String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&addr, CDP_TIMEOUT)
        .map_err(|e| format!("Failed to connect to CDP port {port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(CDP_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CDP_TIMEOUT));

    let req = format!(
        "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("Failed to send /json/list request: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("Failed to read CDP HTTP status: {e}"))?;
    if !status_line.starts_with("HTTP/1.") || !status_line.contains(" 200 ") {
        return Err(format!("Unexpected CDP /json/list status: {}", status_line.trim()));
    }

    let mut content_length: Option<usize> = None;
    let mut is_chunked = false;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed to read CDP HTTP headers: {e}"))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, val)) = line.split_once(':') {
            let name_l = name.trim().to_ascii_lowercase();
            let val_t = val.trim();
            if name_l == "content-length" {
                content_length = val_t.parse::<usize>().ok();
            } else if name_l == "transfer-encoding"
                && val_t.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            }
        }
    }

    let body = if is_chunked {
        read_chunked_body(&mut reader)?
    } else if let Some(len) = content_length {
        if len > MAX_HTTP_BODY {
            return Err("CDP /json/list response exceeds size limit".to_string());
        }
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .map_err(|e| format!("Failed to read CDP body: {e}"))?;
        buf
    } else {
        let mut buf = Vec::new();
        reader
            .take(MAX_HTTP_BODY as u64)
            .read_to_end(&mut buf)
            .map_err(|e| format!("Failed to read CDP body: {e}"))?;
        buf
    };

    serde_json::from_slice(&body)
        .map_err(|e| format!("Invalid CDP /json/list JSON payload: {e}"))
}

fn read_chunked_body<R: BufRead>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        reader
            .read_line(&mut size_line)
            .map_err(|e| format!("Failed reading chunk size: {e}"))?;
        let hex_part = size_line.trim().split(';').next().unwrap_or("").trim();
        let chunk_len = usize::from_str_radix(hex_part, 16)
            .map_err(|_| "Invalid HTTP chunk size from CDP endpoint".to_string())?;
        if chunk_len == 0 {
            break;
        }
        if out.len().saturating_add(chunk_len) > MAX_HTTP_BODY {
            return Err("CDP chunked body exceeds size limit".to_string());
        }
        let mut chunk = vec![0u8; chunk_len];
        reader
            .read_exact(&mut chunk)
            .map_err(|e| format!("Failed reading HTTP chunk body: {e}"))?;
        out.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader
            .read_exact(&mut crlf)
            .map_err(|e| format!("Failed reading HTTP chunk trailer: {e}"))?;
    }
    Ok(out)
}

fn eval_cdp_js(ws_url: &str, port: u16, expression: &str) -> Result<Value, String> {
    if !validate_ws_url(ws_url, port) {
        return Err("Refused non-loopback or unexpected CDP WebSocket URL".to_string());
    }
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let stream = TcpStream::connect_timeout(&addr, CDP_TIMEOUT)
        .map_err(|e| format!("CDP WebSocket TCP connect failed: {e}"))?;
    let _ = stream.set_read_timeout(Some(CDP_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CDP_TIMEOUT));

    let (mut socket, _) = client(ws_url, stream)
        .map_err(|e| format!("CDP WebSocket handshake failed: {e}"))?;

    let req = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": expression,
            "returnByValue": true,
        }
    });
    socket
        .send(Message::Text(req.to_string()))
        .map_err(|e| format!("CDP WebSocket send failed: {e}"))?;

    let result = loop {
        let msg = socket
            .read()
            .map_err(|e| format!("CDP WebSocket read failed: {e}"))?;
        match msg {
            Message::Text(text) => {
                let parsed: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("Invalid CDP JSON message: {e}"))?;
                if parsed.get("id").and_then(Value::as_u64) != Some(1) {
                    continue;
                }
                if let Some(err) = parsed.get("error") {
                    return Err(format!("CDP error: {err}"));
                }
                let res_obj = parsed.get("result").cloned().unwrap_or(Value::Null);
                if let Some(exc) = res_obj.get("exceptionDetails") {
                    let msg = exc
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("Runtime.evaluate exception");
                    return Err(format!("CDP JS exception: {msg}"));
                }
                break res_obj
                    .get("result")
                    .and_then(|r| r.get("value"))
                    .cloned()
                    .unwrap_or(Value::Null);
            }
            Message::Ping(payload) => {
                let _ = socket.send(Message::Pong(payload));
            }
            Message::Close(_) => {
                return Err("CDP WebSocket closed before response".to_string());
            }
            _ => {}
        }
    };

    let _ = socket.close(None);
    Ok(result)
}

pub fn poll_and_inject(
    port: u16,
    custom_dict: &LoadedCustomDict,
) -> Result<PollCycleReport, String> {
    let targets = fetch_targets(port)?;
    let mut report = PollCycleReport {
        targets_seen: targets.len(),
        ..Default::default()
    };

    let desktop_fp = desktop_fingerprint();
    let swarm_fp = swarm_fingerprint();
    let desktop_fp_json = serde_json::to_string(&desktop_fp).unwrap();
    let swarm_fp_json = serde_json::to_string(&swarm_fp).unwrap();
    let custom_hash_json = serde_json::to_string(&custom_dict.sha256).unwrap();
    let custom_entries_json = serde_json::to_string(&custom_dict.entries)
        .map_err(|e| format!("Failed to encode custom translations: {e}"))?;

    let main_probe = format!(
        "(() => {{\n\
           if (typeof document === 'undefined' || !document.body) return 'not_ready';\n\
           if (document.querySelector('[data-agy-zh-control]') && !window.__antigravity_chinese_patch__) return 'static_active';\n\
           const isHost = Boolean(\n\
             window.electronNative || window.electronUpdater || window.nativeStorage ||\n\
             window.__antigravity_chinese_patch__ ||\n\
             (typeof document.title === 'string' && document.title.includes('Antigravity'))\n\
           );\n\
           if (!isHost) return 'not_host';\n\
           const cur = window.__antigravity_chinese_patch__;\n\
           if (cur && cur.fingerprint === {desktop_fp_json}) {{\n\
             return window.__AGY_ZH_CUSTOM_HASH__ === {custom_hash_json} ? 'already_injected' : 'refresh_custom';\n\
           }}\n\
           return 'inject';\n\
         }})()"
    );

    let swarm_probe = format!(
        "(() => {{\n\
           if (typeof document === 'undefined' || !document.body) return 'not_ready';\n\
           if (!document.querySelector('[data-testid=\"swarm-tabs\"]') || !document.querySelector('[data-testid=\"swarm-empty-state\"]')) return 'not_swarm';\n\
           const cur = window.__antigravity_chinese_patch_swarm__;\n\
           if (cur && cur.fingerprint === {swarm_fp_json}) return 'already_injected';\n\
           return 'inject';\n\
         }})()"
    );

    // Read only the language switch from desktop pages, and only while a
    // verified native sheet is open. Skip ambiguous multi-window preferences.
    let sheets: Vec<_> = targets.iter().filter(|target| {
        target.target_type == "page"
            && target.ws_url.as_deref().is_some_and(|ws| validate_ws_url(ws, port))
            && crate::passkeys::is_native_sheet(&target.url)
    }).collect();
    if !sheets.is_empty() {
        let mut states = targets.iter()
            .filter(|t| classify_target(t, port) == TargetKind::MainDesktop)
            .filter_map(|t| eval_cdp_js(t.ws_url.as_deref()?, port,
                "window.__antigravity_chinese_patch__?.isEnabled()").ok()?.as_bool());
        if let Some(enabled) = states.next() {
            if states.all(|state| state == enabled) {
                let render = include_str!("../dist/passkey-status.js");
                let expression = format!("{render}({enabled})");
                for sheet in sheets {
                    if eval_cdp_js(sheet.ws_url.as_deref().unwrap(), port, &expression).is_err() {
                        report.errors.push("Passkeys status renderer unavailable".to_owned());
                    }
                }
            }
        }
    }

    for target in targets {
        let kind = classify_target(&target, port);
        if kind == TargetKind::Ignore {
            report.skipped += 1;
            continue;
        }
        let ws_url = target.ws_url.as_deref().unwrap();
        let safe_url = sanitize_url_for_log(&target.url);

        match kind {
            TargetKind::MainDesktop => match eval_cdp_js(ws_url, port, &main_probe) {
                Ok(Value::String(state)) if state == "inject" => {
                    let payload = format!(
                        "window.__AGY_ZH_MODE__ = \"cdp\";\n\
                         window.__agyZhCustomTranslations = {custom_entries_json};\n\
                         window.__AGY_ZH_CUSTOM_HASH__ = {custom_hash_json};\n\
                         {DESKTOP_RUNTIME_JS}"
                    );
                    match eval_cdp_js(ws_url, port, &payload) {
                        Ok(_) => report.main_injected += 1,
                        Err(e) => report.errors.push(format!("{safe_url}: {e}")),
                    }
                }
                Ok(Value::String(state)) if state == "refresh_custom" => {
                    let payload = format!(
                        "window.__agyZhCustomTranslations = {custom_entries_json};\n\
                         window.__AGY_ZH_CUSTOM_HASH__ = {custom_hash_json};\n\
                         window.__antigravity_chinese_patch__?.refresh?.();"
                    );
                    match eval_cdp_js(ws_url, port, &payload) {
                        Ok(_) => report.main_refreshed += 1,
                        Err(e) => report.errors.push(format!("{safe_url}: {e}")),
                    }
                }
                Ok(_) => {
                    report.skipped += 1;
                }
                Err(e) => {
                    report.errors.push(format!("{safe_url}: {e}"));
                }
            },
            TargetKind::SwarmFrame => match eval_cdp_js(ws_url, port, &swarm_probe) {
                Ok(Value::String(state)) if state == "inject" => {
                    match eval_cdp_js(ws_url, port, SWARM_RUNTIME_JS) {
                        Ok(_) => report.swarm_injected += 1,
                        Err(e) => report.errors.push(format!("{safe_url}: {e}")),
                    }
                }
                Ok(_) => {
                    report.skipped += 1;
                }
                Err(e) => {
                    report.errors.push(format!("{safe_url}: {e}"));
                }
            },
            TargetKind::Ignore => {}
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use tungstenite::accept;

    #[test]
    fn target_filtering_and_url_sanitization_are_strict() {
        assert_eq!(
            sanitize_url_for_log("plugin://swarm--swarm_board-abc123/?token=secret#frag"),
            "plugin://swarm--swarm_board-abc123/"
        );
        assert!(is_swarm_plugin_url(
            "plugin://swarm--swarm_board-9334/?token=secret"
        ));
        assert!(!is_swarm_plugin_url("plugin://other-plugin/"));
        assert!(!is_swarm_plugin_url(
            "plugin://user@swarm--swarm_board-9334/"
        ));
        assert!(!is_swarm_plugin_url(
            "plugin://swarm--swarm_board-9334/evil"
        ));

        let port = 9222;
        assert_eq!(
            classify_target(
                &CdpTarget {
                    id: "1".into(),
                    target_type: "page".into(),
                    url: "http://127.0.0.1:41234/".into(),
                    ws_url: Some("ws://127.0.0.1:9222/devtools/page/1".into()),
                },
                port
            ),
            TargetKind::MainDesktop
        );
        assert_eq!(
            classify_target(
                &CdpTarget {
                    id: "2".into(),
                    target_type: "iframe".into(),
                    url: "plugin://swarm--swarm_board-xyz9/?token=topsecret".into(),
                    ws_url: Some("ws://127.0.0.1:9222/devtools/page/2".into()),
                },
                port
            ),
            TargetKind::SwarmFrame
        );
        assert_eq!(
            classify_target(
                &CdpTarget {
                    id: "3".into(),
                    target_type: "page".into(),
                    url: "https://accounts.google.com/signin".into(),
                    ws_url: Some("ws://127.0.0.1:9222/devtools/page/3".into()),
                },
                port
            ),
            TargetKind::Ignore
        );
        assert_eq!(
            classify_target(
                &CdpTarget {
                    id: "4".into(),
                    target_type: "page".into(),
                    url: "http://127.0.0.1:41234/".into(),
                    ws_url: Some("ws://evil.example.com:9222/devtools/page/4".into()),
                },
                port
            ),
            TargetKind::Ignore
        );
    }

    #[test]
    fn mock_cdp_server_roundtrip_injects_main_and_swarm_idempotently() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let injected_log: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let main_done = Arc::new(Mutex::new(false));
        let swarm_done = Arc::new(Mutex::new(false));
        let log_srv = Arc::clone(&injected_log);
        let main_srv = Arc::clone(&main_done);
        let swarm_srv = Arc::clone(&swarm_done);

        let srv_thread = thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut peek_buf = [0u8; 512];
                let n = stream.peek(&mut peek_buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&peek_buf[..n]).into_owned();
                if head.starts_with("GET /json/list ") {
                    let mut reader = BufReader::new(stream);
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                    }
                    let body = json!([
                        {
                            "id": "main-1",
                            "type": "page",
                            "url": "http://127.0.0.1:50000/",
                            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/main-1")
                        },
                        {
                            "id": "swarm-1",
                            "type": "iframe",
                            "url": "plugin://swarm--swarm_board-abc123/?token=topsecret",
                            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/swarm-1")
                        },
                        {
                            "id": "ext-1",
                            "type": "page",
                            "url": "https://accounts.google.com/",
                            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/ext-1")
                        }
                    ])
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let mut raw = reader.into_inner();
                    let _ = raw.write_all(resp.as_bytes());
                    continue;
                }

                let target_id = if head.contains("/devtools/page/main-1") {
                    "main-1"
                } else if head.contains("/devtools/page/swarm-1") {
                    "swarm-1"
                } else {
                    "other"
                };

                let mut ws = accept(stream).unwrap();
                while let Ok(msg) = ws.read() {
                    if let Message::Text(txt) = msg {
                        let req: Value = serde_json::from_str(&txt).unwrap();
                        let id = req["id"].as_u64().unwrap();
                        let expr = req["params"]["expression"].as_str().unwrap();
                        let reply_val = if expr.contains("document.querySelector('[data-agy-zh-control]')")
                        {
                            if *main_srv.lock().unwrap() {
                                "already_injected"
                            } else {
                                "inject"
                            }
                        } else if expr.contains("swarm-empty-state")
                            && !expr.contains("mountSwarmRuntime")
                        {
                            if *swarm_srv.lock().unwrap() {
                                "already_injected"
                            } else {
                                "inject"
                            }
                        } else {
                            if target_id == "main-1" {
                                *main_srv.lock().unwrap() = true;
                            } else if target_id == "swarm-1" {
                                *swarm_srv.lock().unwrap() = true;
                            }
                            log_srv
                                .lock()
                                .unwrap()
                                .push((target_id.to_string(), expr.chars().take(120).collect()));
                            "ok"
                        };
                        let resp = json!({
                            "id": id,
                            "result": { "result": { "type": "string", "value": reply_val } }
                        });
                        ws.send(Message::Text(resp.to_string())).unwrap();
                    }
                }
            }
        });

        let dict = LoadedCustomDict::empty();
        let first = poll_and_inject(port, &dict).unwrap();
        assert_eq!(first.targets_seen, 3);
        assert_eq!(first.main_injected, 1);
        assert_eq!(first.swarm_injected, 1);
        assert_eq!(first.skipped, 1);
        assert!(first.errors.is_empty());

        // Second poll must short-circuit via already_injected
        let second = poll_and_inject(port, &dict).unwrap();
        assert_eq!(second.main_injected, 0);
        assert_eq!(second.swarm_injected, 0);
        assert_eq!(second.skipped, 3);

        let recorded = injected_log.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].0, "main-1");
        assert!(recorded[0].1.contains("__AGY_ZH_MODE__ = \"cdp\""));
        assert_eq!(recorded[1].0, "swarm-1");
        assert!(recorded[1].1.contains("Antigravity Chinese Patch Swarm Runtime"));

        drop(srv_thread);
    }

    #[test]
    fn fetch_targets_supports_chunked_transfer_encoding() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = json!([{
            "id": "c1",
            "type": "page",
            "url": "http://127.0.0.1:50000/",
            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/c1")
        }])
        .to_string();

        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let _ = stream.read(&mut buf);
                let mid = body.len() / 2;
                let part1 = &body.as_bytes()[..mid];
                let part2 = &body.as_bytes()[mid..];
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                    part1.len(),
                    String::from_utf8_lossy(part1),
                    part2.len(),
                    String::from_utf8_lossy(part2)
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });

        let targets = fetch_targets(port).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "c1");
        handle.join().unwrap();
    }
}
