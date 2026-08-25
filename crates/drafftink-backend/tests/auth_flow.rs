//! 鉴权与 RBAC 集成测试
//!
//! 覆盖成功指标：
//! - 登录成功并签发令牌；`/api/auth/me` 需认证。
//! - RBAC：学生无法访问管理员接口（403），未认证返回 401。
//! - 数据隔离：跨租户访问被拒绝（403）；篡改令牌签名无效（401）。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use drafftink_backend::api::router;
use drafftink_backend::auth::mobile::MobileAuth;
use drafftink_backend::auth::password::hash_password;
use drafftink_backend::auth::ratelimit::LoginRateLimiter;
use drafftink_backend::auth::refresh::MemoryRefreshTokenStore;
use drafftink_backend::config::BackendConfig;
use drafftink_backend::db::{Database, SledDb};
use drafftink_backend::recording::LiveHub;
use drafftink_backend::state::AppState;
use drafftink_backend::storage::LocalStorage;
use drafftink_backend::workflow::WorkflowStore;
use drafftink_core::{Role, User};
use serde::Deserialize;
use tower::ServiceExt;
use uuid::Uuid;

fn make_state() -> AppState {
    let db: Arc<dyn Database> = Arc::new(
        SledDb::open(&std::env::temp_dir().join(format!("drafftink_af_test_{}", Uuid::new_v4())))
            .unwrap(),
    );
    let storage: Arc<dyn drafftink_backend::storage::Storage> = Arc::new(
        LocalStorage::new(
            &std::env::temp_dir().join(format!("drafftink_af_store_{}", Uuid::new_v4())),
        )
        .unwrap(),
    );
    AppState {
        db: db.clone(),
        storage,
        config: BackendConfig::default(),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        login_ratelimit: Arc::new(LoginRateLimiter::new(
            10,
            std::time::Duration::from_secs(60),
        )),
        refresh_store: Arc::new(MemoryRefreshTokenStore::new()),
        workflow: WorkflowStore::new(),
        mobile_auth: MobileAuth::new(),
        live: LiveHub::new(),
    }
}

fn seed_user(state: &AppState, username: &str, role: Role, tenant: Uuid, pwd: &str) -> Uuid {
    let user = User {
        id: Uuid::new_v4(),
        username: username.to_string(),
        display_name: username.to_string(),
        role,
        class_id: None,
        tenant_id: tenant,
        password_hash: hash_password(pwd),
        created_at: chrono::Utc::now(),
        active: true,
    };
    state.db.save_user(&user).unwrap();
    user.id
}

async fn login(state: &AppState, username: &str, pwd: &str) -> (StatusCode, Option<String>) {
    let body =
        serde_json::json!({ "username": username, "password": pwd, "device_fp": "fp" }).to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = router(state.clone()).oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    if status == StatusCode::OK {
        #[derive(Deserialize)]
        struct LR {
            access_token: String,
        }
        let token = serde_json::from_slice::<LR>(&bytes)
            .ok()
            .map(|l| l.access_token);
        (status, token)
    } else {
        (status, None)
    }
}

async fn authed_get(state: &AppState, uri: &str, token: &str) -> StatusCode {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    router(state.clone()).oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn test_login_success_returns_tokens() {
    let state = make_state();
    seed_user(&state, "teacher_a", Role::Teacher, Uuid::new_v4(), "pw_a");
    let (status, token) = login(&state, "teacher_a", "pw_a").await;
    assert_eq!(status, StatusCode::OK);
    assert!(token.is_some());
}

#[tokio::test]
async fn test_login_wrong_password_401() {
    let state = make_state();
    seed_user(&state, "teacher_b", Role::Teacher, Uuid::new_v4(), "pw_b");
    let (status, _) = login(&state, "teacher_b", "wrong").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_me_requires_auth() {
    let state = make_state();
    seed_user(&state, "teacher_c", Role::Teacher, Uuid::new_v4(), "pw_c");
    // 无令牌访问 /api/auth/me 应被拒绝
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .body(Body::empty())
        .unwrap();
    let status = router(state.clone()).oneshot(req).await.unwrap().status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_rbac_admin_only() {
    let state = make_state();
    let tenant = Uuid::new_v4();
    seed_user(&state, "admin_x", Role::Admin, tenant, "pw_admin");
    seed_user(&state, "student_x", Role::Student, tenant, "pw_stu");

    let (_, admin_token) = login(&state, "admin_x", "pw_admin").await;
    let (_, stu_token) = login(&state, "student_x", "pw_stu").await;
    let admin_token = admin_token.unwrap();
    let stu_token = stu_token.unwrap();

    // 管理员可访问
    assert_eq!(
        authed_get(&state, "/api/admin/schools/anything", &admin_token).await,
        StatusCode::OK
    );
    // 学生被拒绝（RBAC）
    assert_eq!(
        authed_get(&state, "/api/admin/schools/anything", &stu_token).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn test_tenant_isolation() {
    let state = make_state();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    seed_user(&state, "teacher_t1", Role::Teacher, t1, "pw1");
    seed_user(&state, "teacher_t2", Role::Teacher, t2, "pw2");

    let (_, tok_t1) = login(&state, "teacher_t1", "pw1").await;
    let tok_t1 = tok_t1.unwrap();

    // 同租户可访问
    assert_eq!(
        authed_get(&state, &format!("/api/tenant/{t1}"), &tok_t1).await,
        StatusCode::OK
    );
    // 跨租户访问被拒绝（数据隔离）
    assert_eq!(
        authed_get(&state, &format!("/api/tenant/{t2}"), &tok_t1).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn test_forged_token_rejected() {
    // 用错误密钥伪造一个篡改了 tenant_id 的令牌，应被签名校验拒绝
    use chrono::Utc;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let mut claims = BTreeMap::new();
    claims.insert("sub".to_string(), "fake".to_string());
    claims.insert("role".to_string(), "admin".to_string());
    claims.insert(
        "tenant_id".to_string(),
        "11111111-1111-1111-1111-111111111111".to_string(),
    );
    claims.insert("typ".to_string(), "access".to_string());
    claims.insert("iat".to_string(), Utc::now().timestamp().to_string());
    claims.insert(
        "exp".to_string(),
        (Utc::now().timestamp() + 3600).to_string(),
    );
    claims.insert("jti".to_string(), Uuid::new_v4().to_string());
    let bad = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(b"wrong-secret-not-the-server-one"),
    )
    .unwrap();

    let state = make_state();
    let status = authed_get(&state, "/api/auth/me", &bad).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
