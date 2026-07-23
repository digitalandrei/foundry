//! MariaDB-backed command and HTTP integration tests. They are ignored by the
//! fast offline suite and run explicitly in CI against a privileged disposable
//! database; `sqlx::test` creates and drops one isolated schema per test.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use foundry_shared::dto::{CreateDeploymentRequest, DeployTarget, TaskPayload, VolumeSpec};
use foundry_shared::{
    GitlabInstanceId, RegistryTagId, ServerId, SlotId, TaskType, UserId, VolumePlacement,
};
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
        files: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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
    let readiness = serde_json::to_vec(&serde_json::json!({
        "setup_revision": foundry_shared::dto::REQUIRED_SETUP_REVISION,
        "required_setup_revision": foundry_shared::dto::REQUIRED_SETUP_REVISION,
        "checked_at": chrono::Utc::now(),
        "checks": [
            {"code": "docker", "status": "READY", "detail": "test fixture"},
            {"code": "docker_gpu", "status": "READY", "detail": "test fixture"},
            {"code": "storage_write", "status": "READY", "detail": "test fixture"},
            {"code": "capabilities", "status": "READY", "detail": "test fixture"}
        ]
    }))
    .unwrap();
    sqlx::query(
        "UPDATE servers SET status = 'ONLINE', docker_ok = 1, setup_revision = ?, \
         readiness_json = ?, readiness_checked_at = ? WHERE id = ?",
    )
    .bind(foundry_shared::dto::REQUIRED_SETUP_REVISION)
    .bind(readiness)
    .bind(now)
    .bind(server_id.0)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO server_agents \
         (id, server_id, agent_version, token_hash, enrolled_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7())
    .bind(server_id.0)
    .bind(env!("CARGO_PKG_VERSION"))
    .bind(vec![0_u8; 32])
    .bind(now)
    .bind(now)
    .bind(now)
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
async fn concurrent_gitlab_mirror_upserts_converge(pool: MySqlPool) {
    use crate::gitlab::types::{GitlabProject, GitlabRegistryRepository, GitlabRegistryTagDetail};

    let now = chrono::Utc::now().naive_utc();
    let instance_id = GitlabInstanceId::new();
    sqlx::query(
        "INSERT INTO gitlab_instances \
         (id, name, base_url, registry_url, oauth_client_id, oauth_client_secret, enabled, \
          created_at, updated_at) \
         VALUES (?, 'mirror-race', 'https://gitlab.test', 'registry.test', 'client', ?, 1, ?, ?)",
    )
    .bind(instance_id.0)
    .bind(Vec::from("encrypted-test-secret".as_bytes()))
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let project = GitlabProject {
        id: 1,
        name: "comfyui-blank".into(),
        path_with_namespace: "templates/comfyui-blank".into(),
        avatar_url: None,
    };
    let project_results = tokio::join!(
        crate::repos::mirror::upsert_project(&pool, instance_id, &project),
        crate::repos::mirror::upsert_project(&pool, instance_id, &project),
        crate::repos::mirror::upsert_project(&pool, instance_id, &project),
        crate::repos::mirror::upsert_project(&pool, instance_id, &project),
    );
    let project_ids = [
        project_results.0.unwrap(),
        project_results.1.unwrap(),
        project_results.2.unwrap(),
        project_results.3.unwrap(),
    ];
    assert!(project_ids.iter().all(|id| *id == project_ids[0]));

    let repository = GitlabRegistryRepository {
        id: 1,
        path: "templates/comfyui-blank".into(),
    };
    let repository_results = tokio::join!(
        crate::repos::mirror::upsert_repository(&pool, project_ids[0], &repository),
        crate::repos::mirror::upsert_repository(&pool, project_ids[0], &repository),
        crate::repos::mirror::upsert_repository(&pool, project_ids[0], &repository),
        crate::repos::mirror::upsert_repository(&pool, project_ids[0], &repository),
    );
    let repository_ids = [
        repository_results.0.unwrap(),
        repository_results.1.unwrap(),
        repository_results.2.unwrap(),
        repository_results.3.unwrap(),
    ];
    assert!(repository_ids.iter().all(|id| *id == repository_ids[0]));

    let tag = GitlabRegistryTagDetail {
        name: "latest".into(),
        total_size: Some(1024),
        created_at: Some(chrono::Utc::now()),
    };
    let tag_results = tokio::join!(
        crate::repos::mirror::upsert_tag(&pool, repository_ids[0], &tag),
        crate::repos::mirror::upsert_tag(&pool, repository_ids[0], &tag),
        crate::repos::mirror::upsert_tag(&pool, repository_ids[0], &tag),
        crate::repos::mirror::upsert_tag(&pool, repository_ids[0], &tag),
    );
    let tag_ids = [
        tag_results.0.unwrap(),
        tag_results.1.unwrap(),
        tag_results.2.unwrap(),
        tag_results.3.unwrap(),
    ];
    assert!(tag_ids.iter().all(|id| *id == tag_ids[0]));

    // A later self-managed GitLab response may explicitly say zero even
    // though the registry-manifest fallback already supplied a real size.
    // The invalid zero must not erase that cached positive value.
    let zero_size_tag = GitlabRegistryTagDetail {
        name: "latest".into(),
        total_size: Some(0),
        created_at: None,
    };
    crate::repos::mirror::upsert_tag(&pool, repository_ids[0], &zero_size_tag)
        .await
        .unwrap();
    let stored_size: Option<i64> =
        sqlx::query_scalar("SELECT size_bytes FROM registry_tags WHERE id = ?")
            .bind(tag_ids[0].0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_size, Some(1024));

    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           (SELECT COUNT(*) FROM gitlab_projects) AS projects, \
           (SELECT COUNT(*) FROM registry_repositories) AS repositories, \
           (SELECT COUNT(*) FROM registry_tags) AS tags",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 1));
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
async fn metrics_endpoint_exposes_core_gauge_families(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    crate::repos::deployments::create(
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

    let app = crate::routes::router(state(pool));
    let response = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains(&format!(
        "foundry_build_info{{version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(text.contains("foundry_database_up 1"));
    assert!(text.contains("foundry_slots{state=\"RESERVED\"} 1"));
    assert!(text.contains("foundry_deployments{state=\"VALIDATING\"} 1"));
    assert!(text.contains("foundry_agent_tasks{state=\"QUEUED\"} 1"));
    assert!(text.contains("foundry_gitlab_mirror_age_seconds{instance="));
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
async fn exhausted_task_is_abandoned_and_fails_the_deployment(pool: MySqlPool) {
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

    // Claim up to the ceiling; each claim goes stale with no result.
    let mut task_id = None;
    for attempt in 1..=5 {
        let claimed = crate::repos::tasks::claim_next(&pool, fixture.server_id)
            .await
            .unwrap();
        let claimed = claimed.unwrap_or_else(|| panic!("attempt {attempt} should re-dispatch"));
        task_id = Some(claimed.id);
        sqlx::query("UPDATE agent_tasks SET dispatched_at = ? WHERE id = ?")
            .bind((chrono::Utc::now() - chrono::Duration::seconds(400)).naive_utc())
            .bind(claimed.id.0)
            .execute(&pool)
            .await
            .unwrap();
    }
    let task_id = task_id.unwrap();

    // Attempt 6 must not re-dispatch: the ceiling is reached.
    assert!(crate::repos::tasks::claim_next(&pool, fixture.server_id)
        .await
        .unwrap()
        .is_none());

    let abandoned = crate::repos::tasks::abandon_exhausted(&pool).await.unwrap();
    assert_eq!(abandoned, 1);

    let (task_state, deployment_state, error_message): (String, String, Option<String>) =
        sqlx::query_as(
            "SELECT t.state, d.state, d.error_message FROM agent_tasks t \
             JOIN deployments d ON d.id = t.deployment_id WHERE t.id = ?",
        )
        .bind(task_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (task_state.as_str(), deployment_state.as_str()),
        ("FAILED", "FAILED")
    );
    assert!(error_message.unwrap().contains("abandoned by controller"));

    // Deploy failure frees the slot (executor guarantees no leftover container).
    let slot_state: String = sqlx::query_scalar("SELECT state FROM gpu_slots WHERE id = ?")
        .bind(fixture.slot_id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(slot_state, "FREE");

    let (result_count, audit_count): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM agent_task_results WHERE agent_task_id = ? AND success = 0), \
                (SELECT COUNT(*) FROM audit_logs WHERE subject_id = ? AND action = 'TASK_ABANDONED')",
    )
    .bind(task_id.0)
    .bind(task_id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((result_count, audit_count), (1, 1));

    // A sweep with nothing eligible is a no-op, and a late agent report
    // after abandonment is an idempotent no-op (deployment stays FAILED).
    assert_eq!(
        crate::repos::tasks::abandon_exhausted(&pool).await.unwrap(),
        0
    );
    let late = foundry_shared::dto::TaskResultReport {
        task_id,
        success: true,
        container_id: Some("late-container".into()),
        error: None,
        failure_stage: None,
        health_status: None,
        health_detail: None,
        readiness: None,
        storage: None,
    };
    crate::repos::tasks::complete(&pool, fixture.server_id, &late)
        .await
        .unwrap();
    let deployment_state: String = sqlx::query_scalar("SELECT state FROM deployments WHERE id = ?")
        .bind(created.id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(deployment_state, "FAILED");
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn active_deployment_names_are_unique_per_server(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    crate::repos::deployments::create(
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
    .unwrap();

    let now = chrono::Utc::now().naive_utc();
    let gpu_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO gpus \
         (id, server_id, gpu_uuid, display_index, model, memory_mb, mig_enabled, \
          last_seen_at, created_at, updated_at) \
         VALUES (?, ?, 'GPU-integration-1', 1, 'Test GPU', 49152, 0, ?, ?, ?)",
    )
    .bind(gpu_id)
    .bind(fixture.server_id.0)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let second_slot = SlotId::new();
    sqlx::query(
        "INSERT INTO gpu_slots \
         (id, gpu_id, slot_type, name, capacity_mb, state, last_seen_at, created_at, updated_at) \
         VALUES (?, ?, 'FULL_GPU', '1', 49152, 'FREE', ?, ?, ?)",
    )
    .bind(second_slot.0)
    .bind(gpu_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let mut duplicate = deployment_request(&fixture);
    duplicate.target = DeployTarget::Slot {
        slot_id: second_slot,
    };
    let error = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &duplicate,
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, AppError::BadRequest(message) if message.contains("already in use on this server"))
    );
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
async fn placement_volumes_reuse_across_users_and_projects(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    let collaborator = UserId::new();
    let now = chrono::Utc::now().naive_utc();
    sqlx::query(
        "INSERT INTO users
         (id, display_name, email, is_admin, created_at, updated_at)
         VALUES (?, 'Collaborator', 'collaborator@foundry.test', 0, ?, ?)",
    )
    .bind(collaborator.0)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let mut spec = VolumeSpec {
        volume_id: None,
        volume_name: "models".into(),
        container_path: "/models".into(),
        read_only: false,
        placement: VolumePlacement::Server,
        purge_on_redeploy: false,
    };
    let mut tx = pool.begin().await.unwrap();
    let creator_shared = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.slot_id,
        None,
        "comfy1",
        &spec,
        fixture.admin,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let collaborator_shared = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.slot_id,
        None,
        "comfy1",
        &spec,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(creator_shared.id, collaborator_shared.id);
    let creator_leaf = creator_shared
        .path
        .strip_prefix("/storage/containers/.foundry/shared/comfy1/models/")
        .expect("new volume uses the logical hierarchy");
    assert!(Uuid::parse_str(creator_leaf).is_ok());

    let mut tx = pool.begin().await.unwrap();
    let other_project = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.slot_id,
        None,
        "comfy2",
        &spec,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(creator_shared.id, other_project.id);
    let other_leaf = other_project
        .path
        .strip_prefix("/storage/containers/.foundry/shared/comfy2/models/")
        .expect("project name separates physical roots");
    assert!(Uuid::parse_str(other_leaf).is_ok());

    spec.placement = VolumePlacement::Slot;
    let mut tx = pool.begin().await.unwrap();
    let slot_volume = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.slot_id,
        None,
        "comfy1",
        &spec,
        fixture.admin,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(creator_shared.id, slot_volume.id);

    let second_gpu = Uuid::now_v7();
    let second_slot = SlotId::new();
    sqlx::query(
        "INSERT INTO gpus
         (id, server_id, gpu_uuid, display_index, model, memory_mb, mig_enabled,
          last_seen_at, created_at, updated_at)
         VALUES (?, ?, 'GPU-storage-test-1', 1, 'Test GPU', 49152, 0, ?, ?, ?)",
    )
    .bind(second_gpu)
    .bind(fixture.server_id.0)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO gpu_slots
         (id, gpu_id, slot_type, name, capacity_mb, state, last_seen_at, created_at, updated_at)
         VALUES (?, ?, 'FULL_GPU', '1', 49152, 'FREE', ?, ?, ?)",
    )
    .bind(second_slot.0)
    .bind(second_gpu)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    spec.placement = VolumePlacement::Server;
    let mut tx = pool.begin().await.unwrap();
    let server_volume_from_second_slot = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "comfy1",
        &spec,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(creator_shared.id, server_volume_from_second_slot.id);

    spec.placement = VolumePlacement::Slot;
    let mut tx = pool.begin().await.unwrap();
    let other_slot_volume = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "comfy1",
        &spec,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(slot_volume.id, other_slot_volume.id);

    let explicit_shared = VolumeSpec {
        volume_id: Some(creator_shared.id),
        volume_name: "models".into(),
        container_path: "/any-destination".into(),
        read_only: true,
        placement: VolumePlacement::Server,
        purge_on_redeploy: false,
    };
    let mut tx = pool.begin().await.unwrap();
    let cross_project_shared = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "another-deployment-name",
        &explicit_shared,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(cross_project_shared.id, creator_shared.id);

    let mismatched_placement = VolumeSpec {
        placement: VolumePlacement::Slot,
        ..explicit_shared.clone()
    };
    let mut tx = pool.begin().await.unwrap();
    let error = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "another-deployment-name",
        &mismatched_placement,
        collaborator,
        None,
    )
    .await
    .expect_err("a selected volume's placement remains authoritative");
    tx.rollback().await.unwrap();
    assert!(matches!(error, AppError::BadRequest(message) if message.contains("placement")));

    let mismatched_name = VolumeSpec {
        volume_name: "settings".into(),
        ..explicit_shared
    };
    let mut tx = pool.begin().await.unwrap();
    let error = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "another-deployment-name",
        &mismatched_name,
        collaborator,
        None,
    )
    .await
    .expect_err("a selected volume's source name remains authoritative");
    tx.rollback().await.unwrap();
    assert!(matches!(error, AppError::BadRequest(message) if message.contains("mount name")));

    let explicit_slot = VolumeSpec {
        volume_id: Some(slot_volume.id),
        volume_name: "models".into(),
        container_path: "/another-destination".into(),
        read_only: false,
        placement: VolumePlacement::Slot,
        purge_on_redeploy: false,
    };
    let mut tx = pool.begin().await.unwrap();
    let same_slot = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.slot_id,
        None,
        "another-deployment-name",
        &explicit_slot,
        collaborator,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(same_slot.id, slot_volume.id);

    let mut tx = pool.begin().await.unwrap();
    let error = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        second_slot,
        None,
        "another-deployment-name",
        &explicit_slot,
        collaborator,
        None,
    )
    .await
    .expect_err("a slot root cannot be selected for another physical slot");
    tx.rollback().await.unwrap();
    assert!(matches!(error, AppError::Forbidden));

    let visible = crate::repos::volumes::list(&pool, fixture.server_id, None, collaborator, false)
        .await
        .unwrap();
    assert!(visible.iter().any(|volume| volume.id == creator_shared.id));
    assert!(visible.iter().any(|volume| volume.id == slot_volume.id));

    let first_target = crate::repos::volumes::list(
        &pool,
        fixture.server_id,
        Some(fixture.slot_id.0),
        collaborator,
        false,
    )
    .await
    .unwrap();
    assert!(first_target
        .iter()
        .any(|volume| volume.id == creator_shared.id));
    assert!(first_target
        .iter()
        .any(|volume| volume.id == slot_volume.id));
    assert!(first_target
        .iter()
        .all(|volume| volume.id != other_slot_volume.id));

    let second_target = crate::repos::volumes::list(
        &pool,
        fixture.server_id,
        Some(second_slot.0),
        collaborator,
        false,
    )
    .await
    .unwrap();
    assert!(second_target
        .iter()
        .any(|volume| volume.id == creator_shared.id));
    assert!(second_target
        .iter()
        .any(|volume| volume.id == other_slot_volume.id));
    assert!(second_target
        .iter()
        .all(|volume| volume.id != slot_volume.id));
}

#[sqlx::test(migrator = "crate::MIGRATOR")]
#[ignore = "requires privileged disposable MariaDB; CI runs ignored tests"]
async fn shared_volume_mapping_blocks_unsafe_purge_and_reports_attachments(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    sqlx::query("UPDATE gpu_slots SET max_occupants = 2 WHERE id = ?")
        .bind(fixture.slot_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let mut source = deployment_request(&fixture);
    source.name = Some("source".into());
    source.volumes = vec![VolumeSpec {
        volume_id: None,
        volume_name: "models".into(),
        container_path: "/models".into(),
        read_only: false,
        placement: VolumePlacement::Server,
        purge_on_redeploy: false,
    }];
    crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &source,
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let source_volume = crate::repos::volumes::list(
        &pool,
        fixture.server_id,
        Some(fixture.slot_id.0),
        fixture.admin,
        true,
    )
    .await
    .unwrap()
    .into_iter()
    .find(|volume| volume.project_name == "source" && volume.name == "models")
    .expect("source volume exists");

    let mut unsafe_consumer = deployment_request(&fixture);
    unsafe_consumer.name = Some("unsafe-consumer".into());
    unsafe_consumer.volumes = vec![VolumeSpec {
        volume_id: Some(source_volume.id),
        volume_name: "models".into(),
        container_path: "/reuse".into(),
        read_only: true,
        placement: VolumePlacement::Server,
        purge_on_redeploy: true,
    }];
    let error = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &unsafe_consumer,
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        None,
    )
    .await
    .expect_err("an active shared root cannot be marked for purge");
    assert!(
        matches!(error, AppError::BadRequest(message) if message.contains("purge_on_redeploy"))
    );

    let mut consumer = unsafe_consumer;
    consumer.name = Some("consumer".into());
    consumer.volumes[0].purge_on_redeploy = false;
    let consumer_deployment = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &consumer,
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.admin,
        None,
        None,
        None,
    )
    .await
    .expect("a shared root remains mountable without purge");

    let audit_detail: Vec<u8> = sqlx::query_scalar(
        "SELECT detail FROM audit_logs WHERE action = 'DEPLOYMENT_CREATED' AND subject_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(consumer_deployment.id.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let audit: serde_json::Value = serde_json::from_slice(&audit_detail).unwrap();
    let mapping = &audit["mounts"][0];
    assert_eq!(mapping["selection"], "existing");
    assert_eq!(mapping["volume_id"], source_volume.id.to_string());
    assert_eq!(mapping["source"]["project_name"], "source");
    assert_eq!(mapping["container_path"], "/reuse");
    assert_eq!(mapping["read_only"], true);
    assert!(mapping.get("host_path").is_none());

    let listed = crate::repos::volumes::list(
        &pool,
        fixture.server_id,
        Some(fixture.slot_id.0),
        fixture.admin,
        true,
    )
    .await
    .unwrap();
    let mapped = listed
        .iter()
        .find(|volume| volume.id == source_volume.id)
        .expect("shared volume remains listed");
    let mut attached_to = mapped.attached_to.clone();
    attached_to.sort();
    assert_eq!(attached_to, vec!["consumer", "source"]);
    assert_eq!(mapped.attachments.len(), 2);
    assert!(mapped.attachments.iter().any(|attachment| {
        attachment.deployment_name == "source"
            && attachment.container_path == "/models"
            && !attachment.read_only
            && !attachment.purge_on_redeploy
    }));
    assert!(mapped.attachments.iter().any(|attachment| {
        attachment.deployment_name == "consumer"
            && attachment.container_path == "/reuse"
            && attachment.read_only
            && !attachment.purge_on_redeploy
    }));
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
