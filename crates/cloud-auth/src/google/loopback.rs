//! 一次性 OAuth 回环请求的解析与应答。
use crate::CloudAuthError;
use oauth2::CsrfToken;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

pub(super) async fn read_callback(
    stream: &mut TcpStream,
    expected: &CsrfToken,
) -> Result<Option<String>, CloudAuthError> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 1024];
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let Ok(n) = stream.read(&mut buffer).await else {
            return Ok(None);
        };
        if n == 0 || bytes.len() + n > 8192 {
            return Ok(None);
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(None);
    };
    let mut line = text.lines().next().unwrap_or_default().split_whitespace();
    if line.next() != Some("GET") {
        return Ok(None);
    }
    let target = line.next().unwrap_or_default();
    if !target.starts_with("/oauth/callback?") {
        return Ok(None);
    }
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Ok(None);
    };
    let pairs: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = pairs.iter().filter(|(k, _)| k == "state").collect();
    if states.len() != 1 || CsrfToken::new(states[0].1.to_string()) != *expected {
        return Ok(None);
    }
    if pairs.iter().any(|(k, _)| k == "error") {
        return Err(CloudAuthError::Denied);
    }
    let codes: Vec<_> = pairs.iter().filter(|(k, _)| k == "code").collect();
    if codes.len() != 1 || codes[0].1.is_empty() {
        return Ok(None);
    }
    Ok(Some(codes[0].1.to_string()))
}

pub(super) async fn respond(stream: &mut TcpStream, body: &str, status: u16) {
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        if status == 200 { "OK" } else { "Bad Request" },
        body.len()
    );
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        stream.write_all(response.as_bytes()),
    )
    .await;
}
