//! CSV 在线清洗接口集成测试
//!
//! 通过真实 HTTP 层（`router(state).oneshot`）验证 `POST /api/v1/etl/clean-csv`：
//! - 正确解析 multipart 上传的 CSV 与 `date_columns` / `code_columns` 参数；
//! - 脏日期被标准化为 `YYYY-MM-DD`，非法日期进入 `failed_rows`；
//! - 代码列经 `lookup` 逻辑校验，非法代码进入 `code_issues`；
//! - 返回 `summary` / `failed_rows` / `code_issues` / `preview` 结构完整。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use drafftink_backend::api::router;
use drafftink_backend::auth::mobile::MobileAuth;
use drafftink_backend::auth::ratelimit::LoginRateLimiter;
use drafftink_backend::auth::refresh::MemoryRefreshTokenStore;
use drafftink_backend::config::BackendConfig;
use drafftink_backend::db::{Database, SledDb};
use drafftink_backend::recording::LiveHub;
use drafftink_backend::state::AppState;
use drafftink_backend::storage::LocalStorage;
use drafftink_backend::workflow::WorkflowStore;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

fn make_state() -> AppState {
    let db: Arc<dyn Database> = Arc::new(
        SledDb::open(
            &std::env::temp_dir().join(format!("drafftink_etl_test_{}", Uuid::new_v4())),
        )
        .unwrap(),
    );
    let storage: Arc<dyn drafftink_backend::storage::Storage> = Arc::new(
        LocalStorage::new(
            &std::env::temp_dir().join(format!("drafftink_etl_store_{}", Uuid::new_v4())),
        )
        .unwrap(),
    );
    AppState {
        db,
        storage,
        config: BackendConfig::default(),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        login_ratelimit: Arc::new(LoginRateLimiter::new(10, std::time::Duration::from_secs(60))),
        refresh_store: Arc::new(MemoryRefreshTokenStore::new()),
        workflow: WorkflowStore::new(),
        mobile_auth: MobileAuth::new(),
        live: LiveHub::new(),
    }
}

/// 手工构造一个 multipart/form-data 请求体（无需额外依赖）。
fn build_multipart() -> (String, Vec<u8>) {
    let boundary = "----drafftinketlboundary";
    let csv = "student_id,gender,birth_date,enroll_date,school_type\n\
S001,1,2008.9.1,2024/9/1,211\n\
S002,2,2009/13/01,2024.8.31,311\n\
S003,3,2010-03-05,2019-09-01,999\n\
S004,,2010/1/1,2019/9/1,211\n";

    let mut body: Vec<u8> = Vec::new();
    // 文件字段
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"file\"; filename=\"test.csv\"\r\n");
    body.extend_from_slice(b"Content-Type: text/csv\r\n\r\n");
    body.extend_from_slice(csv.as_bytes());
    body.extend_from_slice(b"\r\n");
    // date_columns 字段
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"date_columns\"\r\n\r\n");
    body.extend_from_slice(b"birth_date,enroll_date");
    body.extend_from_slice(b"\r\n");
    // code_columns 字段
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"code_columns\"\r\n\r\n");
    body.extend_from_slice(b"gender:gender,school_type:school_type");
    body.extend_from_slice(b"\r\n");
    // 结束边界
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

async fn post_clean_csv() -> Value {
    let state = make_state();
    let (ct, body) = build_multipart();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/etl/clean-csv")
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(body))
        .unwrap();
    let resp = router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_clean_csv_summary_and_dates() {
    let json = post_clean_csv().await;
    assert_eq!(json["summary"]["total"], 4);
    // 仅 S002 的 birth_date 非法 → 1 行失败，3 行成功
    assert_eq!(json["summary"]["failed"], 1);
    assert_eq!(json["summary"]["success"], 3);

    // failed_rows 含 S002 的 birth_date
    let failed = json["failed_rows"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["row"], 2);
    assert_eq!(failed[0]["column"], "birth_date");
    assert_eq!(failed[0]["raw"], "2009/13/01");
    assert_eq!(failed[0]["reason"], "日期格式非法");

    // 首行日期已标准化
    let preview = json["preview"].as_array().unwrap();
    assert_eq!(preview.len(), 4);
    assert_eq!(preview[0]["student_id"], "S001");
    assert_eq!(preview[0]["birth_date"], "2008-09-01");
    assert_eq!(preview[0]["enroll_date"], "2024-09-01");
}

#[tokio::test]
async fn test_clean_csv_code_validation() {
    let json = post_clean_csv().await;
    let issues = json["code_issues"].as_array().unwrap();
    // S003：gender=9 与 school_type=999 均非法
    assert_eq!(issues.len(), 2);
    let rows: Vec<u64> = issues.iter().map(|i| i["row"].as_u64().unwrap()).collect();
    assert!(rows.contains(&3));
    let tables: Vec<&str> = issues.iter().map(|i| i["table"].as_str().unwrap()).collect();
    assert!(tables.contains(&"gender"));
    assert!(tables.contains(&"school_type"));
    // S001 / S002 的合法代码不应出现在 code_issues
    assert!(!rows.contains(&1));
    assert!(!rows.contains(&2));
}

#[tokio::test]
async fn test_clean_csv_missing_file_returns_400() {
    let state = make_state();
    // 仅上传字段、不传 file
    let boundary = "----missfile";
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"date_columns\"\r\n\r\n");
    body.extend_from_slice(b"birth_date");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/etl/clean-csv")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
