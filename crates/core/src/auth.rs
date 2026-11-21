//! OAuth 2.0 for desktop apps: loopback redirect + PKCE, with the user's own "Desktop app" client.
//!
//! - The Google password is never requested: consent happens in the system browser.
//! - The user brings `client_secret.json` from their own Google Cloud project; it is not part of AutoUpload.
//! - Tokens live in the config folder, in the same JSON format as google-auth (`Credentials.to_json()`), so the
//!   tokens of AutoUpload 1.x keep working.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::fsutil::{protect, write_atomic};
use crate::paths::config_dir;

pub const SCOPES: [&str; 3] = [
    "https://www.googleapis.com/auth/youtube.upload", // videos.insert, thumbnails.set
    "https://www.googleapis.com/auth/youtube.force-ssl", // captions.insert, playlistItems.insert
    "https://www.googleapis.com/auth/youtube.readonly", // channels.list, videos.list
];
const DEFAULT_AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URI: &str = "https://oauth2.googleapis.com/revoke";
const LOGIN_TIMEOUT_SECS: i64 = 300;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct AuthError(pub String);

type R<T> = Result<T, AuthError>;

fn err<T>(s: impl Into<String>) -> R<T> {
    Err(AuthError(s.into()))
}

fn d_auth() -> String {
    DEFAULT_AUTH_URI.into()
}
fn d_token() -> String {
    DEFAULT_TOKEN_URI.into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientInfo {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "d_auth")]
    pub auth_uri: String,
    #[serde(default = "d_token")]
    pub token_uri: String,
}

#[derive(Debug, Clone)]
pub enum ClientKind {
    Desktop(ClientInfo),
    Web,
    Invalid,
    Missing,
}

/// Reads the OAuth client file. Only its shape is inspected; nothing is printed.
pub fn read_client(path: &Path) -> ClientKind {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ClientKind::Missing;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return ClientKind::Invalid;
    };
    if let Some(inst) = v.get("installed") {
        return serde_json::from_value(inst.clone())
            .map(ClientKind::Desktop)
            .unwrap_or(ClientKind::Invalid);
    }
    if v.get("web").is_some() {
        return ClientKind::Web;
    }
    ClientKind::Invalid
}

/// Token file, with the keys google-auth writes; unknown keys are preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default = "d_token")]
    pub token_uri: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Token {
    pub fn expiry_time(&self) -> Option<DateTime<Utc>> {
        let s = self.expiry.as_deref()?.trim_end_matches('Z');
        let s = s.split('+').next().unwrap_or(s);
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
            .ok()
            .map(|n| Utc.from_utc_datetime(&n))
    }

    fn set_expiry(&mut self, t: DateTime<Utc>) {
        self.expiry = Some(t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string());
    }

    fn fresh(&self) -> bool {
        self.token.is_some()
            && self
                .expiry_time()
                .map(|e| e > Utc::now() + Duration::seconds(120))
                .unwrap_or(false)
    }
}

pub fn token_path(profile: &str) -> PathBuf {
    config_dir().join(format!("token-{profile}.json"))
}

pub fn read_token(profile: &str) -> Option<Token> {
    std::fs::read_to_string(token_path(profile))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

fn save_token(profile: &str, tok: &Token) -> R<()> {
    let p = token_path(profile);
    let text = serde_json::to_string(tok).map_err(|e| AuthError(e.to_string()))?;
    write_atomic(&p, &text).map_err(|e| {
        AuthError(format!(
            "could not save the sign-in in {}: {e}",
            p.display()
        ))
    })?;
    protect(&p);
    Ok(())
}

fn http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(format!("AutoUpload/{}", crate::VERSION))
        .build()
        .expect("http client")
}

/// A signed-in profile that hands out fresh access tokens, refreshing them when needed.
pub struct Session {
    profile: String,
    token: Token,
}

impl Session {
    pub fn load(profile: &str) -> R<Session> {
        let Some(token) = read_token(profile) else {
            return err(format!("no session: run `autoupload auth {profile} login`"));
        };
        Ok(Session {
            profile: profile.to_string(),
            token,
        })
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn bearer(&mut self) -> R<String> {
        if !self.token.fresh() {
            self.refresh()?;
        }
        Ok(self.token.token.clone().unwrap_or_default())
    }

    pub fn refresh(&mut self) -> R<()> {
        let Some(rt) = self.token.refresh_token.clone() else {
            return err(format!(
                "invalid sign-in: run `autoupload auth {} login` again",
                self.profile
            ));
        };
        let resp = http()
            .post(&self.token.token_uri)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", rt.as_str()),
                ("client_id", self.token.client_id.as_str()),
                ("client_secret", self.token.client_secret.as_str()),
            ])
            .send()
            .map_err(|e| AuthError(format!("could not reach Google to renew the sign-in: {e}")))?;
        let status = resp.status();
        let body: Value = resp.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let code = body["error"].as_str().unwrap_or("");
            if code == "invalid_grant" || code == "unauthorized_client" || status.as_u16() == 401 {
                return err(format!(
                    "the permission expired or was revoked: run `autoupload auth {} login` again",
                    self.profile
                ));
            }
            return err(format!(
                "Google refused to renew the sign-in ({status}): {code}"
            ));
        }
        self.token.token = body["access_token"].as_str().map(String::from);
        let secs = body["expires_in"].as_i64().unwrap_or(3600);
        self.token.set_expiry(Utc::now() + Duration::seconds(secs));
        if let Some(new_rt) = body["refresh_token"].as_str() {
            self.token.refresh_token = Some(new_rt.to_string());
        }
        save_token(&self.profile, &self.token)
    }
}

fn random_string(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// Signs in through the browser and saves the token. `open_url` opens the consent page (the system browser).
pub fn login(client_secret: &Path, profile: &str, open_url: &dyn Fn(&str)) -> R<Token> {
    let info = match read_client(client_secret) {
        ClientKind::Desktop(i) => i,
        ClientKind::Missing => {
            return err(format!(
                "missing {}. Create an OAuth client (Desktop app) in Google Cloud and save it there; guide: {}#client",
                client_secret.display(),
                crate::GUIDE_URL
            ))
        }
        ClientKind::Web => return err("the client file is a Web application client: create a client of type Desktop app"),
        ClientKind::Invalid => return err(format!("{} is not an OAuth client file", client_secret.display())),
    };
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AuthError(format!("could not open a local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AuthError(e.to_string()))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_string(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_string(24);
    let mut url =
        url::Url::parse(&info.auth_uri).map_err(|e| AuthError(format!("bad auth_uri: {e}")))?;
    url.query_pairs_mut()
        .append_pair("client_id", &info.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", &SCOPES.join(" "))
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("state", &state);
    open_url(url.as_str());
    let code = wait_for_code(&listener, &state)?;

    let resp = http()
        .post(&info.token_uri)
        .form(&[
            ("code", code.as_str()),
            ("client_id", info.client_id.as_str()),
            ("client_secret", info.client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .map_err(|e| AuthError(format!("could not reach Google: {e}")))?;
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !status.is_success() {
        return err(format!(
            "Google refused the sign-in ({status}): {}",
            body["error"].as_str().unwrap_or("")
        ));
    }
    let granted: Vec<String> = body["scope"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .map(String::from)
        .collect();
    let missing: Vec<&str> = SCOPES
        .iter()
        .copied()
        .filter(|s| !granted.iter().any(|g| g == s))
        .collect();
    if !missing.is_empty() {
        return err(format!(
            "not all permissions were granted: {missing:?}. Sign in again and allow all three"
        ));
    }
    let mut tok = Token {
        token: body["access_token"].as_str().map(String::from),
        refresh_token: body["refresh_token"].as_str().map(String::from),
        token_uri: info.token_uri.clone(),
        client_id: info.client_id.clone(),
        client_secret: info.client_secret.clone(),
        scopes: SCOPES.iter().map(|s| s.to_string()).collect(),
        expiry: None,
        extra: Map::new(),
    };
    tok.extra.insert(
        "universe_domain".into(),
        Value::String("googleapis.com".into()),
    );
    tok.set_expiry(Utc::now() + Duration::seconds(body["expires_in"].as_i64().unwrap_or(3600)));
    if tok.refresh_token.is_none() {
        return err("Google did not return a refresh token; remove AutoUpload at myaccount.google.com/permissions and sign in again");
    }
    save_token(profile, &tok)?;
    Ok(tok)
}

const PAGE_OK: &str = "<!doctype html><meta charset=utf-8><title>AutoUpload</title>\
<body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;line-height:1.5\">\
<h1>AutoUpload is connected</h1><p>You can close this tab and go back to AutoUpload.</p></body>";
const PAGE_FAIL: &str = "<!doctype html><meta charset=utf-8><title>AutoUpload</title>\
<body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;line-height:1.5\">\
<h1>Sign-in was not completed</h1><p>Go back to AutoUpload to try again.</p></body>";

fn respond(mut s: &TcpStream, status: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn wait_for_code(listener: &TcpListener, state: &str) -> R<String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| AuthError(e.to_string()))?;
    let deadline = Utc::now() + Duration::seconds(LOGIN_TIMEOUT_SECS);
    while Utc::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                let mut buf = [0u8; 8192];
                let n = (&stream).read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let target = req.split_whitespace().nth(1).unwrap_or("/");
                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
                let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();
                let get = |k: &str| pairs.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone());
                if let Some(e) = get("error") {
                    respond(&stream, "200 OK", PAGE_FAIL);
                    return err(format!("sign-in was cancelled or refused: {e}"));
                }
                let Some(code) = get("code") else {
                    respond(&stream, "404 Not Found", "");
                    continue; // favicon and similar
                };
                if get("state").as_deref() != Some(state) {
                    respond(&stream, "400 Bad Request", PAGE_FAIL);
                    return err("the sign-in answer did not match this request; try again");
                }
                respond(&stream, "200 OK", PAGE_OK);
                return Ok(code);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(e) => return err(format!("local sign-in listener failed: {e}")),
        }
    }
    err("the sign-in was not completed within 5 minutes")
}

/// Revokes the token at Google (if possible) and deletes the local copy. Returns false if there was no session.
pub fn logout(profile: &str, revoke: bool) -> bool {
    let p = token_path(profile);
    let Some(tok) = read_token(profile) else {
        return std::fs::remove_file(&p).is_ok();
    };
    if revoke {
        if let Some(t) = tok.refresh_token.as_deref().or(tok.token.as_deref()) {
            let _ = http().post(REVOKE_URI).form(&[("token", t)]).send();
        }
    }
    let _ = std::fs::remove_file(&p);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_auth_token_round_trip() {
        let text = r#"{"token": "ya29.x", "refresh_token": "1//r", "token_uri": "https://oauth2.googleapis.com/token",
            "client_id": "id.apps.googleusercontent.com", "client_secret": "s", "scopes": ["a"],
            "universe_domain": "googleapis.com", "account": "", "expiry": "2026-11-21T18:44:31.123456Z"}"#;
        let t: Token = serde_json::from_str(text).unwrap();
        assert_eq!(
            t.expiry_time().unwrap(),
            Utc.with_ymd_and_hms(2026, 11, 21, 18, 44, 31).unwrap()
                + Duration::microseconds(123456)
        );
        let back: Value = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back["universe_domain"], "googleapis.com");
        assert_eq!(back["account"], "");
        assert_eq!(back["expiry"], "2026-11-21T18:44:31.123456Z");
    }

    #[test]
    fn client_kinds() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.json");
        assert!(matches!(read_client(&p), ClientKind::Missing));
        std::fs::write(&p, r#"{"web": {"client_id": "x"}}"#).unwrap();
        assert!(matches!(read_client(&p), ClientKind::Web));
        std::fs::write(
            &p,
            r#"{"installed": {"client_id": "x", "client_secret": "y"}}"#,
        )
        .unwrap();
        assert!(matches!(read_client(&p), ClientKind::Desktop(_)));
        std::fs::write(&p, "nope").unwrap();
        assert!(matches!(read_client(&p), ClientKind::Invalid));
    }

    #[test]
    fn loopback_returns_code_and_checks_state() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET /favicon.ico HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                s,
                "GET /?state=abc&code=4%2Fxyz HTTP/1.1\r\nHost: x\r\n\r\n"
            )
            .unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        });
        assert_eq!(wait_for_code(&l, "abc").unwrap(), "4/xyz");
        assert!(h.join().unwrap().contains("AutoUpload is connected"));
    }
}
