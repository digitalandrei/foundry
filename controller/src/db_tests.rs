//! MariaDB-backed command and HTTP integration tests. They are ignored by the
//! fast offline suite and run explicitly in CI against a privileged disposable
//! database; `sqlx::test` creates and drops one isolated schema per test.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use foundry_shared::dto::{CreateDeploymentRequest, DeployTarget, TaskPayload, VolumeSpec};
use foundry_shared::{
    GitlabInstanceId, RegistryTagId, ServerId, SlotId, TaskType, UserId, VolumePlacement,
    VolumeVisibility,
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
    project_id: foundry_shared::GitlabProjectId,
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
        project_id: project_id.into(),
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
async fn deployment_command_commits_reservation_task_event_and_audit(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    let created = crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &deployment_request(&fixture),
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.project_id,
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
async fn active_deployment_names_are_unique_per_server(pool: MySqlPool) {
    let fixture = insert_runtime_fixture(&pool).await;
    crate::repos::deployments::create(
        &pool,
        &state(pool.clone()).secrets,
        &deployment_request(&fixture),
        "registry.test/team/model:v1",
        fixture.instance_id,
        fixture.project_id,
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
        fixture.project_id,
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
        fixture.project_id,
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
async fn project_volumes_reuse_across_users_while_private_volumes_do_not(pool: MySqlPool) {
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
        visibility: VolumeVisibility::Project,
        placement: VolumePlacement::Server,
        purge_on_redeploy: false,
    };
    let mut tx = pool.begin().await.unwrap();
    let creator_shared = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.project_id,
        fixture.slot_id,
        &spec,
        fixture.admin,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let collaborator_shared = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.project_id,
        fixture.slot_id,
        &spec,
        collaborator,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(creator_shared, collaborator_shared);
    assert!(creator_shared.1.starts_with("/storage/containers/volumes/"));

    spec.visibility = VolumeVisibility::Private;
    let mut tx = pool.begin().await.unwrap();
    let creator_private = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.project_id,
        fixture.slot_id,
        &spec,
        fixture.admin,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let collaborator_private = crate::repos::volumes::ensure(
        &mut tx,
        fixture.server_id,
        fixture.project_id,
        fixture.slot_id,
        &spec,
        collaborator,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(creator_private.0, collaborator_private.0);

    let visible = crate::repos::volumes::list(
        &pool,
        fixture.server_id,
        fixture.project_id,
        None,
        collaborator,
        false,
    )
    .await
    .unwrap();
    assert!(visible.iter().any(|volume| volume.id == creator_shared.0));
    assert!(!visible.iter().any(|volume| volume.id == creator_private.0));
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
