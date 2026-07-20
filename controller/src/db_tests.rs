//! MariaDB-backed command and HTTP integration tests. They are ignored by the
//! fast offline suite and run explicitly in CI against a privileged disposable
//! database; `sqlx::test` creates and drops one isolated schema per test.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use foundry_shared::dto::{CreateDeploymentRequest, DeployTarget, TaskPayload};
use foundry_shared::{GitlabInstanceId, RegistryTagId, ServerId, SlotId, TaskType, UserId};
use sqlx::MySqlPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::crypto::SecretBox;
use crate::error::AppError;
use crate::state::AppState;

fn state(pool: MySqlPool) -> AppState {
    AppState {
        pool,
        secrets: SecretBox::from_base64_key("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
            .expect("test key"),
        http: reqwest::Client::new(),
        public_url: Arc::from("https://foundry.test"),
        admin_emails: Arc::from(Vec::<String>::new()),
        apps_domain: None,
        progress: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        shells: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    }
}

async fn insert_admin(pool: &MySqlPool) -> Result<UserId, sqlx::Error> {
    let id = UserId::new();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query(
        "INSERT INTO users \
         (id, display_name, email, is_admin, created_at, updated_at) \
         VALUES (?, 'Test Admin', 'admin@foundry.test', 1, ?, ?)",
    )
    .bind(id.0)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(id)
}

struct RuntimeFixture {
    admin: UserId,
    server_id: ServerId,
    instance_id: GitlabInstanceId,
    tag_id: RegistryTagId,
    slot_id: SlotId,
    gpu_uuid: String,
}

async fn insert_runtime_fixture(pool: &MySqlPool) -> RuntimeFixture {
    let admin = insert_admin(pool).await.unwrap();
    let (server_id, _, _) = crate::repos::servers::create_with_enrollment(
        pool,
        "gpu-runtime",
        admin,
        Some("127.0.0.1"),
    )
    .await
    .unwrap();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query("UPDATE servers SET status = 'ONLINE', docker_ok = 1 WHERE id = ?")
        .bind(server_id.0)
        .execute(pool)
        .await
        .unwrap();

    let instance_id: GitlabInstanceId = Uuid::now_v7().into();
    sqlx::query(
        "INSERT INTO gitlab_instances \
         (id, name, base_url, registry_url, oauth_client_id, oauth_client_secret, enabled, \
          created_at, updated_at) \
         VALUES (?, 'integration', 'https://gitlab.test', 'registry.test', 'client', ?, 1, ?, ?)",
    )
    .bind(instance_id.0)
    .bind(Vec::from("encrypted-test-secret".as_bytes()))
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let project_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO gitlab_projects \
         (id, gitlab_instance_id, gitlab_project_id, path_with_namespace, name, created_at, updated_at) \
         VALUES (?, ?, 1, 'team/model', 'model', ?, ?)",
    )
    .bind(project_id)
    .bind(instance_id.0)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let repository_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO registry_repositories \
         (id, gitlab_project_id, gitlab_repository_id, path, created_at, updated_at) \
         VALUES (?, ?, 1, 'team/model', ?, ?)",
    )
    .bind(repository_id)
    .bind(project_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let tag_id: RegistryTagId = Uuid::now_v7().into();
    sqlx::query(
        "INSERT INTO registry_tags \
         (id, registry_repository_id, name, created_at, updated_at) \
         VALUES (?, ?, 'v1', ?, ?)",
    )
    .bind(tag_id.0)
    .bind(repository_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let gpu_id = Uuid::now_v7();
    let gpu_uuid = "GPU-integration-0".to_string();
    sqlx::query(
        "INSERT INTO gpus \
         (id, server_id, gpu_uuid, display_index, model, memory_mb, mig_enabled, \
          last_seen_at, created_at, updated_at) \
         VALUES (?, ?, ?, 0, 'Test GPU', 49152, 0, ?, ?, ?)",
    )
    .bind(gpu_id)
    .bind(server_id.0)
    .bind(&gpu_uuid)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    let slot_id: SlotId = Uuid::now_v7().into();
    sqlx::query(
        "INSERT INTO gpu_slots \
         (id, gpu_id, slot_type, name, capacity_mb, state, last_seen_at, created_at, updated_at) \
         VALUES (?, ?, 'FULL_GPU', '0', 49152, 'FREE', ?, ?, ?)",
    )
    .bind(slot_id.0)
    .bind(gpu_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    RuntimeFixture {
        admin,
        server_id,
        instance_id,
        tag_id,
        slot_id,
        gpu_uuid,
    }
}

fn deployment_request(fixture: &RuntimeFixture) -> CreateDeploymentRequest {
    CreateDeploymentRequest {
        target: DeployTarget::Slot {
            slot_id: fixture.slot_id,
        },
        registry_tag_id: fixture.tag_id,
        name: Some("integration-deploy".into()),
        ports: Vec::new(),
        env: Vec::new(),
        volumes: Vec::new(),
        mem_limit_mb: None,
    }
}

async fn insert_external_container(pool: &MySqlPool, fixture: &RuntimeFixture, id: &str) {
    let now = chrono::Utc::now().naive_utc();
    sqlx::query(
        "INSERT INTO server_containers \
         (id, server_id, container_id, name, image, state, status, managed, ports, gpu_uuids, mounts, reported_at) \
         VALUES (?, ?, ?, 'external-trainer', 'external:v1', 'running', 'Up', 0, '[]', ?, '[]', ?)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.server_id.0)
    .bind(id)
    .bind(serde_json::to_string(&vec![&fixture.gpu_uuid]).unwrap())
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn health_router_reports_migrated_database(pool: MySqlPool) {
    let app = crate::routes::router(state(pool));
    let response = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["database"], "up");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));

    let protected = app
        .oneshot(Request::get("/api/servers").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(protected.into_body(), 16 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "unauthorized");
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn deployment_command_commits_reservation_task_event_and_audit(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    let created = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &deployment_request(&fixture),
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        Some("127.0.0.1"),
    )
    .await
    .unwrap();

    let state_and_slot: (String, String) = sqlx::query_as(
        "SELECT d.state, gs.state FROM deployments d \
         JOIN deployment_slots ds ON ds.deployment_id = d.id \
         JOIN gpu_slots gs ON gs.id = ds.gpu_slot_id WHERE d.id = ?",
    )
    .bind(created.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let task_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_tasks WHERE deployment_id = ? AND state = 'QUEUED'",
    )
    .bind(created.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deployment_events WHERE deployment_id = ?")
            .bind(created.id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE subject_id = ? AND action = 'DEPLOYMENT_CREATED'",
    )
    .bind(created.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state_and_slot, ("VALIDATING".into(), "RESERVED".into()));
    assert_eq!((task_count, event_count, audit_count), (1, 1, 1));

    let deployments = crate::repos::deployments::list(&pool).await.unwrap();
    let servers = crate::repos::servers::list(&pool).await.unwrap();
    assert_eq!(deployments[0].slot_ids, vec![fixture.slot_id]);
    assert_eq!(servers[0].gpus[0].slots[0].id, fixture.slot_id);
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn running_external_gpu_is_an_authoritative_no_write_rejection(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    insert_external_container(&pool, &fixture, "external-blocker").await;
    let error = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &deployment_request(&fixture),
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, AppError::BadRequest(message) if message.contains("external-trainer")));
    let writes: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM deployments) AS deployments, \
           (SELECT COUNT(*) FROM agent_tasks) AS tasks, \
           (SELECT COUNT(*) FROM audit_logs WHERE action = 'DEPLOYMENT_CREATED') AS audits",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(writes, (0, 0, 0));
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn active_adoption_is_unique_in_repository_and_database(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    let container_id = "external-adopted";
    insert_external_container(&pool, &fixture, container_id).await;
    let deployment_id = crate::repos::deployments::adopt(
        &pool,
        fixture.server_id,
        container_id,
        fixture.admin,
        Some("127.0.0.1"),
    )
    .await
    .unwrap();
    let duplicate = crate::repos::deployments::adopt(
        &pool,
        fixture.server_id,
        container_id,
        fixture.admin,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(duplicate, AppError::BadRequest(_)));

    let now = chrono::Utc::now().naive_utc();
    let db_duplicate = sqlx::query(
        "INSERT INTO deployments \
         (id, gpu_slot_id, server_id, image_ref, created_by, state, container_name, \
          container_id, adopted_container_id, started_at, created_at, updated_at) \
         VALUES (?, ?, ?, 'external:v1', ?, 'RUNNING', 'duplicate', ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7())
    .bind(fixture.slot_id.0)
    .bind(fixture.server_id.0)
    .bind(fixture.admin.0)
    .bind(container_id)
    .bind(container_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(matches!(db_duplicate, sqlx::Error::Database(db) if db.is_unique_violation()));

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE subject_id = ? AND action = 'CONTAINER_ADOPTED'",
    )
    .bind(deployment_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn enrollment_consumption_credential_and_audit_commit_together(pool: MySqlPool) {
    let admin = insert_admin(&pool).await.unwrap();
    let (server_id, token, _) = crate::repos::servers::create_with_enrollment(
        &pool,
        "gpu-integration",
        admin,
        Some("127.0.0.1"),
    )
    .await
    .unwrap();
    let enrolled = crate::repos::servers::enroll(
        &pool,
        &token,
        "gpu-integration.local",
        env!("CARGO_PKG_VERSION"),
        Some("Test OS"),
    )
    .await
    .unwrap();

    assert_eq!(enrolled.server_id, server_id);
    let credential_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM server_agents WHERE server_id = ?")
            .bind(server_id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE subject_id = ? AND action = 'AGENT_ENROLLED'",
    )
    .bind(server_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let used_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enrollment_tokens WHERE server_id = ? AND used_at IS NOT NULL",
    )
    .bind(server_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((credential_count, audit_count, used_count), (1, 1, 1));
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn guarded_server_removal_reports_counts_and_preserves_history(pool: MySqlPool) {
    let admin = insert_admin(&pool).await.unwrap();
    let (server_id, _, _) =
        crate::repos::servers::create_with_enrollment(&pool, "gpu-used", admin, None)
            .await
            .unwrap();
    let mut tx = pool.begin().await.unwrap();
    crate::repos::tasks::enqueue(
        &mut tx,
        server_id,
        None,
        TaskType::RefreshInventory,
        &TaskPayload::None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let error = crate::repos::servers::delete_unused(&pool, server_id, admin, None)
        .await
        .unwrap_err();
    match error {
        AppError::Conflict { details, .. } => assert_eq!(details["dependencies"]["tasks"], 1),
        other => panic!("expected conflict, got {other:?}"),
    }
    let still_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM servers WHERE id = ?")
        .bind(server_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still_exists, 1);
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn never_used_server_is_deleted_with_an_audit_record(pool: MySqlPool) {
    let admin = insert_admin(&pool).await.unwrap();
    let (server_id, _, _) =
        crate::repos::servers::create_with_enrollment(&pool, "gpu-unused", admin, None)
            .await
            .unwrap();
    crate::repos::servers::delete_unused(&pool, server_id, admin, None)
        .await
        .unwrap();

    let server_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM servers WHERE id = ?")
        .bind(server_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE subject_id = ? AND action = 'SERVER_DELETED'",
    )
    .bind(server_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((server_count, audit_count), (0, 1));
}
