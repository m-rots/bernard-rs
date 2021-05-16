use crate::{fetch, Account};
use chrono::serde::ts_seconds;
use chrono::{DateTime, Duration, Utc};
use itertools::join;
use jsonwebtoken::{encode, Algorithm, Header};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{Fetcher, Result};

#[derive(Debug, Serialize, Deserialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: String,
    aud: &'a str,

    #[serde(with = "ts_seconds")]
    exp: DateTime<Utc>,

    #[serde(with = "ts_seconds")]
    iat: DateTime<Utc>,
}

impl<'a> Claims<'a> {
    fn new(iss: &'a str, scope: &Scope) -> Self {
        let iat = Utc::now();

        Self {
            aud: "https://oauth2.googleapis.com/token",
            scope: join(&scope.scopes, " "),
            exp: iat + scope.lifetime,
            iat,
            iss,
        }
    }
}

fn create_jwt(account: &Account, scope: &Scope) -> (String, DateTime<Utc>) {
    let header = Header::new(Algorithm::RS256);
    let claims = Claims::new(&account.client_email, &scope);

    let jwt = encode(&header, &claims, &account.private_key.0).unwrap();
    (jwt, claims.exp)
}

impl Fetcher {
    async fn access_token_inner(self: Arc<Fetcher>, scope: &Scope) -> fetch::Result<AccessToken> {
        let (jwt, exp) = tokio::task::block_in_place(|| create_jwt(&self.account, scope));

        #[derive(Serialize)]
        struct Form<'a> {
            grant_type: &'a str,
            assertion: &'a str,
        }

        let form = Form {
            assertion: &jwt,
            grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
        };

        #[derive(Deserialize)]
        struct Response {
            access_token: String,
        }

        let request = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&form)
            .build()
            .unwrap();

        let Response { access_token } = self.make_request_inner(request).await?;

        Ok(AccessToken {
            token: access_token,
            expiry: exp,
        })
    }
}

impl Account {}

#[derive(Clone, Debug)]
pub(crate) struct AccessToken {
    pub expiry: DateTime<Utc>,
    pub token: String,
}

pub(crate) struct RefreshToken {
    scope: Scope,
    token: Mutex<Option<AccessToken>>,
}

impl RefreshToken {
    pub(crate) fn new(scope: Scope) -> Self {
        Self {
            token: Mutex::new(None),
            scope,
        }
    }

    pub(crate) async fn access_token(&self, fetch: Arc<Fetcher>) -> Result<AccessToken> {
        let mut token_guard = self.token.lock().await;

        // Pretend that we are 10 seconds in the future to prevent possible errors.
        let now = Utc::now() + Duration::seconds(10);

        match token_guard.as_ref() {
            Some(token) if token.expiry > now => Ok(token.clone()),
            _ => {
                let token = fetch.access_token_inner(&self.scope).await?;
                *token_guard = Some(token.clone());
                Ok(token)
            }
        }
    }
}

pub struct Scope {
    lifetime: Duration,
    scopes: HashSet<String>,
}

impl Scope {
    pub fn builder() -> ScopeBuilder {
        ScopeBuilder(Self::default())
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            lifetime: Duration::minutes(60),
            scopes: HashSet::new(),
        }
    }
}

pub struct ScopeBuilder(Scope);

impl ScopeBuilder {
    pub fn build(self) -> Scope {
        self.0
    }

    pub fn scope<S: Into<String>>(mut self, scope: S) -> Self {
        self.0.scopes.insert(scope.into());
        self
    }

    pub fn lifetime(mut self, lifetime: Duration) -> Self {
        self.0.lifetime = lifetime;
        self
    }
}
