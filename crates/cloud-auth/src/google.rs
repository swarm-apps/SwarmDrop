mod loopback;
use crate::{
    ClientCredentials, CloudAccountProvider, CloudAuthError, ConnectSession, PendingConnect,
    ProviderId, RevokeOutcome, TokenMaterial, credentials::now,
};
use async_trait::async_trait;
use loopback::{read_callback, respond};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
    basic::BasicClient,
};
use std::time::Duration;
use tokio::net::TcpListener;

type GoogleClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

pub struct GoogleProvider {
    http: reqwest::Client,
    auth_url: String,
    token_url: String,
    revoke_url: String,
    timeout: Duration,
}
impl GoogleProvider {
    pub fn new() -> Result<Self, CloudAuthError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| CloudAuthError::Unavailable)?,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            timeout: Duration::from_secs(300),
        })
    }
    fn client(&self, credentials: &ClientCredentials) -> Result<GoogleClient, CloudAuthError> {
        credentials.validate()?;
        Ok(
            BasicClient::new(ClientId::new(credentials.client_id.clone()))
                .set_client_secret(ClientSecret::new(credentials.client_secret.clone()))
                .set_auth_type(AuthType::RequestBody)
                .set_auth_uri(
                    AuthUrl::new(self.auth_url.clone())
                        .map_err(|_| CloudAuthError::InvalidConfiguration)?,
                )
                .set_token_uri(
                    TokenUrl::new(self.token_url.clone())
                        .map_err(|_| CloudAuthError::InvalidConfiguration)?,
                ),
        )
    }
}

#[async_trait]
impl CloudAccountProvider for GoogleProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::GoogleDrive
    }
    async fn start_connect(
        &self,
        credentials: ClientCredentials,
    ) -> Result<PendingConnect, CloudAuthError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| CloudAuthError::Unavailable)?;
        let port = listener
            .local_addr()
            .map_err(|_| CloudAuthError::Unavailable)?
            .port();
        let client = self.client(&credentials)?.set_redirect_uri(
            RedirectUrl::new(format!("http://127.0.0.1:{port}/oauth/callback"))
                .map_err(|_| CloudAuthError::InvalidConfiguration)?,
        );
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/drive.file".into(),
            ))
            .set_pkce_challenge(challenge)
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent")
            .url();
        let http = self.http.clone();
        let timeout = self.timeout;
        Ok(PendingConnect {
            session: ConnectSession {
                id: uuid::Uuid::new_v4().to_string(),
                authorization_url: url.to_string(),
            },
            completion: Box::pin(async move {
                tokio::time::timeout(timeout, async move {
                    loop {
                        let (mut stream, _) = listener
                            .accept()
                            .await
                            .map_err(|_| CloudAuthError::Unavailable)?;
                        let callback = tokio::time::timeout(
                            Duration::from_secs(3),
                            read_callback(&mut stream, &state),
                        )
                        .await;
                        match callback {
                            Ok(Ok(Some(code))) => {
                                let result = client
                                    .exchange_code(AuthorizationCode::new(code))
                                    .set_pkce_verifier(verifier)
                                    .request_async(&http)
                                    .await
                                    .map_err(|_| CloudAuthError::Denied)
                                    .and_then(|token| material(&token, None));
                                respond(
                                    &mut stream,
                                    if result.is_ok() {
                                        "授权已完成，可以返回 SwarmDrop。"
                                    } else {
                                        "授权未完成，请返回 SwarmDrop 查看状态。"
                                    },
                                    200,
                                )
                                .await;
                                return result;
                            }
                            Ok(Err(CloudAuthError::Denied)) => {
                                respond(&mut stream, "授权已取消，可以返回 SwarmDrop。", 200).await;
                                return Err(CloudAuthError::Denied);
                            }
                            _ => respond(&mut stream, "无效的授权回调。", 400).await,
                        }
                    }
                })
                .await
                .map_err(|_| CloudAuthError::Timeout)?
            }),
        })
    }
    async fn refresh(
        &self,
        credentials: &ClientCredentials,
        token: &TokenMaterial,
    ) -> Result<TokenMaterial, CloudAuthError> {
        let result = self
            .client(credentials)?
            .exchange_refresh_token(&RefreshToken::new(token.refresh_token.clone()))
            .request_async(&self.http)
            .await;
        match result {
            Ok(next) => material(&next, Some(&token.refresh_token)),
            Err(oauth2::RequestTokenError::ServerResponse(error))
                if error.error() == &oauth2::basic::BasicErrorResponseType::InvalidGrant =>
            {
                Err(CloudAuthError::ReconnectRequired)
            }
            Err(oauth2::RequestTokenError::ServerResponse(_)) => {
                Err(CloudAuthError::InvalidConfiguration)
            }
            Err(_) => Err(CloudAuthError::Unavailable),
        }
    }
    async fn revoke(&self, token: &TokenMaterial) -> RevokeOutcome {
        let revoked = match self
            .http
            .post(&self.revoke_url)
            .form(&[("token", &token.refresh_token)])
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        };
        RevokeOutcome { revoked }
    }
}

fn material(
    token: &oauth2::basic::BasicTokenResponse,
    previous_refresh: Option<&str>,
) -> Result<TokenMaterial, CloudAuthError> {
    let refresh_token = token
        .refresh_token()
        .map(|t| t.secret().as_str())
        .or(previous_refresh)
        .ok_or(CloudAuthError::ReconnectRequired)?;
    let expires = token.expires_in().ok_or(CloudAuthError::Denied)?.as_secs();
    if token.access_token().secret().is_empty() || refresh_token.is_empty() || expires == 0 {
        return Err(CloudAuthError::Denied);
    }
    Ok(TokenMaterial {
        access_token: token.access_token().secret().clone(),
        refresh_token: refresh_token.into(),
        expires_at: now().saturating_add(expires),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length: ")
                                    .and_then(|s| s.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (url, handle)
    }
    fn credentials() -> ClientCredentials {
        ClientCredentials {
            client_id: "test-client".into(),
            client_secret: "client-secret-sentinel".into(),
        }
    }

    #[tokio::test]
    async fn callback_rejects_wrong_state_then_exchanges_pkce_and_requests_offline_access() {
        let (endpoint, requests) = server(vec![(200, r#"{"access_token":"access-sentinel","refresh_token":"refresh-sentinel","token_type":"Bearer","expires_in":3600}"#)]).await;
        let mut provider = GoogleProvider::new().unwrap();
        provider.token_url = endpoint;
        let pending = provider.start_connect(credentials()).await.unwrap();
        let auth = url::Url::parse(&pending.session.authorization_url).unwrap();
        let query: std::collections::BTreeMap<_, _> = auth
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(query["access_type"], "offline");
        assert_eq!(query["prompt"], "consent");
        assert_eq!(query["code_challenge_method"], "S256");
        let completion = tokio::spawn(pending.completion);
        let mut callback = url::Url::parse(&query["redirect_uri"]).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("state", "wrong")
            .append_pair("code", "code-sentinel");
        assert_eq!(reqwest::get(callback.clone()).await.unwrap().status(), 400);
        callback.set_query(None);
        callback
            .query_pairs_mut()
            .append_pair("state", &query["state"])
            .append_pair("code", "code-sentinel");
        assert_eq!(reqwest::get(callback).await.unwrap().status(), 200);
        let token = completion.await.unwrap().unwrap();
        assert_eq!(token.refresh_token, "refresh-sentinel");
        let requests = requests.await.unwrap();
        assert!(requests[0].contains("code_verifier="));
        assert!(requests[0].contains("grant_type=authorization_code"));
    }

    #[tokio::test]
    async fn callback_timeout_closes_listener() {
        let mut provider = GoogleProvider::new().unwrap();
        provider.timeout = Duration::from_millis(10);
        let pending = provider.start_connect(credentials()).await.unwrap();
        assert!(matches!(
            pending.completion.await,
            Err(CloudAuthError::Timeout)
        ));
    }

    #[tokio::test]
    async fn refresh_preserves_google_refresh_token_and_redacts_server_failure() {
        let (endpoint, requests) = server(vec![(200, r#"{"access_token":"new-access","token_type":"Bearer","expires_in":3600}"#), (400, r#"{"error":"invalid_grant","error_description":"refresh-sentinel must not appear"}"#), (503, "{}")] ).await;
        let mut provider = GoogleProvider::new().unwrap();
        provider.token_url = endpoint.clone();
        provider.revoke_url = endpoint;
        let old = TokenMaterial {
            access_token: "old".into(),
            refresh_token: "refresh-sentinel".into(),
            expires_at: 0,
        };
        assert_eq!(
            provider
                .refresh(&credentials(), &old)
                .await
                .unwrap()
                .refresh_token,
            "refresh-sentinel"
        );
        let error = provider.refresh(&credentials(), &old).await.err().unwrap();
        assert_eq!(error, CloudAuthError::ReconnectRequired);
        assert!(!format!("{error:?} {error}").contains("sentinel"));
        assert!(!provider.revoke(&old).await.revoked);
        assert_eq!(requests.await.unwrap().len(), 3);
    }
}
