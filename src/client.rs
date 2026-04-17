use anyhow::Result;
use reqwest::{Client, Response};
use serde_json::Value;

/// Thin async HTTP client wrapping `reqwest` for the full ElysianDB REST API.
///
/// - Cookie jar is enabled: after `login()`, subsequent requests automatically
///   send the `edb_session` cookie.
/// - Every method returns `Result<Response>` — callers (test suites) handle
///   status checks and deserialization.
///
/// `Clone` is cheap: `reqwest::Client` is internally reference-counted and
/// all clones share the same cookie jar, which lets crash-recovery /
/// edge-case suites spawn concurrent requests from multiple tasks without
/// re-authenticating.
#[derive(Clone)]
pub struct ElysianClient {
    http: Client,
    base_url: String,
    port: u16,
    /// Optional bearer token for token-auth mode tests.
    token: Option<String>,
}

impl ElysianClient {
    /// Create a new client pointing at `http://127.0.0.1:{port}`.
    pub fn new(port: u16) -> Self {
        let http = Client::builder()
            .cookie_store(true)
            .build()
            .expect("Failed to build reqwest client");

        Self {
            http,
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            token: None,
        }
    }

    /// Enable `Authorization: Bearer` header on every request.
    pub fn with_token(mut self, token: &str) -> Self {
        self.token = Some(token.to_string());
        self
    }

    /// Clear the bearer token (revert to cookie-only auth).
    pub fn clear_token(&mut self) {
        self.token = None;
    }

    /// HTTP port this client targets. Used by suites that need to spin up
    /// a second cookie-isolated client against the same instance.
    pub fn port(&self) -> u16 {
        self.port
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    // ------------------------------------------------------------------
    // System
    // ------------------------------------------------------------------

    pub async fn health(&self) -> Result<Response> {
        let req = self.http.get(self.url("/health"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn stats(&self) -> Result<Response> {
        let req = self.http.get(self.url("/stats"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn config(&self) -> Result<Response> {
        let req = self.http.get(self.url("/config"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn save(&self) -> Result<Response> {
        let req = self.http.post(self.url("/save"));
        Ok(self.apply_auth(req).send().await?)
    }

    /// Raw GET against an arbitrary path — used by the edge-case suite to
    /// test trailing-slash equivalence (`/api/{entity}` vs `/api/{entity}/`),
    /// which the typed helpers normalize away.
    pub async fn raw_get(&self, path: &str) -> Result<Response> {
        let req = self.http.get(self.url(path));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn reset(&self) -> Result<Response> {
        let req = self.http.post(self.url("/reset"));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Auth / Security
    // ------------------------------------------------------------------

    pub async fn login(&self, username: &str, password: &str) -> Result<Response> {
        let body = serde_json::json!({
            "username": username,
            "password": password,
        });
        let req = self.http.post(self.url("/api/security/login")).json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn logout(&self) -> Result<Response> {
        let req = self.http.post(self.url("/api/security/logout"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn me(&self) -> Result<Response> {
        let req = self.http.get(self.url("/api/security/me"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn create_user(&self, body: Value) -> Result<Response> {
        let req = self.http.post(self.url("/api/security/user")).json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn list_users(&self) -> Result<Response> {
        let req = self.http.get(self.url("/api/security/user"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn get_user(&self, username: &str) -> Result<Response> {
        let req = self
            .http
            .get(self.url(&format!("/api/security/user/{username}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn delete_user(&self, username: &str) -> Result<Response> {
        let req = self
            .http
            .delete(self.url(&format!("/api/security/user/{username}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn change_password(&self, username: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/security/user/{username}/password")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn change_role(&self, username: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/security/user/{username}/role")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // ACL
    // ------------------------------------------------------------------

    pub async fn get_acl(&self, username: &str, entity: &str) -> Result<Response> {
        let req = self
            .http
            .get(self.url(&format!("/api/acl/{username}/{entity}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn get_all_acls(&self, username: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/acl/{username}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn set_acl(&self, username: &str, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/acl/{username}/{entity}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn reset_acl(&self, username: &str, entity: &str) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/acl/{username}/{entity}/default")));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Entity CRUD
    // ------------------------------------------------------------------

    pub async fn create(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/{entity}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    /// Send a raw POST body to `/api/{entity}` with an explicit content type.
    /// Used to test malformed-JSON rejection (cannot be expressed via `serde_json::Value`).
    pub async fn create_raw(
        &self,
        entity: &str,
        body: &str,
        content_type: &str,
    ) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/{entity}")))
            .header("Content-Type", content_type)
            .body(body.to_string());
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn list(&self, entity: &str, params: &[(&str, &str)]) -> Result<Response> {
        let req = self
            .http
            .get(self.url(&format!("/api/{entity}")))
            .query(params);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn get(&self, entity: &str, id: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/{entity}/{id}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn get_with_params(
        &self,
        entity: &str,
        id: &str,
        params: &[(&str, &str)],
    ) -> Result<Response> {
        let req = self
            .http
            .get(self.url(&format!("/api/{entity}/{id}")))
            .query(params);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn update(&self, entity: &str, id: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/{entity}/{id}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn batch_update(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/{entity}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn delete(&self, entity: &str, id: &str) -> Result<Response> {
        let req = self.http.delete(self.url(&format!("/api/{entity}/{id}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn delete_all(&self, entity: &str) -> Result<Response> {
        let req = self.http.delete(self.url(&format!("/api/{entity}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn count(&self, entity: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/{entity}/count")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn exists(&self, entity: &str, id: &str) -> Result<Response> {
        let req = self
            .http
            .get(self.url(&format!("/api/{entity}/{id}/exists")));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Query
    // ------------------------------------------------------------------

    pub async fn query(&self, body: Value) -> Result<Response> {
        let req = self.http.post(self.url("/api/query")).json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Schema
    // ------------------------------------------------------------------

    pub async fn get_schema(&self, entity: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/{entity}/schema")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn set_schema(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/{entity}/schema")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn create_entity_type(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/{entity}/create")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn list_entity_types(&self) -> Result<Response> {
        let req = self.http.get(self.url("/api/entity/types"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn list_entity_type_names(&self) -> Result<Response> {
        let req = self.http.get(self.url("/api/entity/types/name"));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Transactions
    // ------------------------------------------------------------------

    pub async fn tx_begin(&self) -> Result<Response> {
        let req = self.http.post(self.url("/api/tx/begin"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn tx_write(&self, tx_id: &str, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/tx/{tx_id}/entity/{entity}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn tx_update(
        &self,
        tx_id: &str,
        entity: &str,
        id: &str,
        body: Value,
    ) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/tx/{tx_id}/entity/{entity}/{id}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn tx_delete(&self, tx_id: &str, entity: &str, id: &str) -> Result<Response> {
        let req = self
            .http
            .delete(self.url(&format!("/api/tx/{tx_id}/entity/{entity}/{id}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn tx_commit(&self, tx_id: &str) -> Result<Response> {
        let req = self.http.post(self.url(&format!("/api/tx/{tx_id}/commit")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn tx_rollback(&self, tx_id: &str) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/tx/{tx_id}/rollback")));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // KV (HTTP)
    // ------------------------------------------------------------------

    pub async fn kv_set(&self, key: &str, value: &str, ttl: Option<u64>) -> Result<Response> {
        let mut req = self
            .http
            .put(self.url(&format!("/kv/{key}")))
            .body(value.to_string());
        if let Some(t) = ttl {
            req = req.query(&[("ttl", t.to_string())]);
        }
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn kv_get(&self, key: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/kv/{key}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn kv_mget(&self, keys: &[&str]) -> Result<Response> {
        let joined = keys.join(",");
        let req = self
            .http
            .get(self.url("/kv/mget"))
            .query(&[("keys", joined)]);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn kv_delete(&self, key: &str) -> Result<Response> {
        let req = self.http.delete(self.url(&format!("/kv/{key}")));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Hooks
    // ------------------------------------------------------------------

    pub async fn create_hook(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/hook/{entity}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn list_hooks(&self, entity: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/hook/{entity}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn get_hook(&self, id: &str) -> Result<Response> {
        let req = self.http.get(self.url(&format!("/api/hook/id/{id}")));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn update_hook(&self, id: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .put(self.url(&format!("/api/hook/id/{id}")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn delete_hook(&self, entity: &str, id: &str) -> Result<Response> {
        let req = self
            .http
            .delete(self.url(&format!("/api/hook/{entity}/{id}")));
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Migrations
    // ------------------------------------------------------------------

    pub async fn migrate(&self, entity: &str, body: Value) -> Result<Response> {
        let req = self
            .http
            .post(self.url(&format!("/api/{entity}/migrate")))
            .json(&body);
        Ok(self.apply_auth(req).send().await?)
    }

    // ------------------------------------------------------------------
    // Import / Export
    // ------------------------------------------------------------------

    pub async fn export(&self) -> Result<Response> {
        let req = self.http.get(self.url("/api/export"));
        Ok(self.apply_auth(req).send().await?)
    }

    pub async fn import(&self, body: Value) -> Result<Response> {
        let req = self.http.post(self.url("/api/import")).json(&body);
        Ok(self.apply_auth(req).send().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_correct_base_url() {
        let client = ElysianClient::new(9000);
        assert_eq!(client.base_url, "http://127.0.0.1:9000");
        assert_eq!(client.port(), 9000);
    }

    #[test]
    fn with_token_sets_token() {
        let client = ElysianClient::new(9000).with_token("abc");
        assert_eq!(client.token, Some("abc".to_string()));
    }

    #[test]
    fn clear_token_removes_token() {
        let mut client = ElysianClient::new(9000).with_token("abc");
        client.clear_token();
        assert_eq!(client.token, None);
    }

    #[test]
    fn url_helper_concatenates_path() {
        let client = ElysianClient::new(8080);
        assert_eq!(client.url("/health"), "http://127.0.0.1:8080/health");
        assert_eq!(
            client.url("/api/battle_books"),
            "http://127.0.0.1:8080/api/battle_books"
        );
    }
}
