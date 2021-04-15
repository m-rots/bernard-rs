use chrono::serde::ts_seconds;
use chrono::{DateTime, Duration, Utc};
use itertools::join;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashSet};
use std::{convert::TryFrom, io::BufReader};
use std::{fs::File, path::Path};

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

#[derive(Debug, Deserialize)]
#[serde(try_from = "String")]
struct PrivateKey(EncodingKey);

impl TryFrom<String> for PrivateKey {
    type Error = jsonwebtoken::errors::Error;

    fn try_from(key: String) -> Result<Self, Self::Error> {
        let key = EncodingKey::from_rsa_pem(key.as_ref())?;
        Ok(Self(key))
    }
}

#[derive(Debug, Deserialize)]
pub struct Account {
    #[serde(skip)]
    client: reqwest::Client,

    client_email: String,
    private_key: PrivateKey,
}

impl Account {
    pub fn from_file<P: AsRef<Path>>(file_name: P) -> Self {
        let file = File::open(file_name).unwrap();
        let reader = BufReader::new(file);

        serde_json::from_reader(reader).unwrap()
    }

    fn create_jwt(&self, scope: &Scope) -> (String, DateTime<Utc>) {
        let header = Header::new(Algorithm::RS256);
        let claims = Claims::new(&self.client_email, &scope);

        let jwt = encode(&header, &claims, &self.private_key.0).unwrap();
        (jwt, claims.exp)
    }

    async fn access_token(&self, scope: &Scope) -> AccessToken {
        let (jwt, exp) = tokio::task::block_in_place(|| self.create_jwt(scope));

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

        let Response { access_token } = self
            .client
            .post("https://oauth2.googleapis.com/token")
            .form(&form)
            .send()
            .await
            .expect("Error making HTTP request")
            .error_for_status()
            .expect("Invalid status code")
            .json()
            .await
            .expect("Error decoding JSON response");

        AccessToken {
            token: access_token,
            expiry: exp,
        }
    }

    pub async fn refresh_token(&self, scope: Scope) -> RefreshToken {
        RefreshToken::new(self, scope).await
    }
}

#[derive(Debug)]
pub struct AccessToken {
    pub expiry: DateTime<Utc>,
    pub token: String,
}

pub struct RefreshToken {
    scope: Scope,
    token: AccessToken,
}

impl RefreshToken {
    pub async fn new(account: &Account, scope: Scope) -> Self {
        let token = account.access_token(&scope).await;
        Self { token, scope }
    }

    async fn refresh(&mut self, account: &Account) {
        self.token = account.access_token(&self.scope).await;
    }

    pub async fn access_token<'a>(&'a mut self, account: &'a Account) -> &'a AccessToken {
        let now = Utc::now();

        match self.token.expiry.cmp(&now) {
            Ordering::Greater => &self.token,
            _ => {
                self.refresh(account).await;
                &self.token
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
