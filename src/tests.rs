#[cfg(test)]
mod tests {
    use crate::scanner;
    use crate::validator;
    use crate::generator;
    use crate::models::{AgentsManifest, Capability, Endpoint, Parameter, Authentication};
    use std::fs;
    use std::path::Path;

    fn mock_manifest() -> AgentsManifest {
        AgentsManifest {
            name: "test-repo".to_string(),
            description: "A test repository for agents.".to_string(),
            version: Some("1.0.0".to_string()),
            capabilities: vec![
                Capability {
                    name: "do_something".to_string(),
                    description: "Does something useful.".to_string(),
                    input_schema: None,
                    output_schema: None,
                    examples: vec!["example usage".to_string()],
                }
            ],
            endpoints: vec![
                Endpoint {
                    path: "/api/test".to_string(),
                    method: "GET".to_string(),
                    description: "Test endpoint".to_string(),
                    parameters: vec![
                        Parameter {
                            name: "id".to_string(),
                            r#type: "string".to_string(),
                            required: true,
                            description: "The ID".to_string(),
                        }
                    ],
                }
            ],
            authentication: Some(Authentication {
                r#type: "bearer".to_string(),
                description: Some("Pass token in Authorization header".to_string()),
            }),
            rate_limits: None,
            contact: None,
        }
    }

    #[test]
    fn test_scanner_rejects_nonexistent_path() {
        let result = scanner::scan_local("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_scanner_scans_current_dir() {
        let result = scanner::scan_local("./");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert!(!ctx.name.is_empty());
        assert!(ctx.source_files.len() > 0);
        assert!(ctx.package_manifest.is_some());
    }

    #[test]
    fn test_scanner_finds_cargo_toml() {
        let result = scanner::scan_local("./").unwrap();
        assert!(result.package_manifest.is_some());
        let manifest = result.package_manifest.unwrap();
        assert!(manifest.contains("[package]"));
    }

    #[test]
    fn test_generator_creates_file() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_output.md";
        let result = generator::generate_agents_md(&manifest, path);
        assert!(result.is_ok());
        assert!(Path::new(path).exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_name() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_name.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("test-repo"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_capabilities() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_caps.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("## Capabilities"));
        assert!(content.contains("do_something"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_generator_output_contains_endpoints() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_ep.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("## Endpoints"));
        assert!(content.contains("GET"));
        assert!(content.contains("/api/test"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_validator_passes_valid_content() {
        let manifest = mock_manifest();
        let path = "/tmp/beacon_test_valid.md";
        generator::generate_agents_md(&manifest, path).unwrap();
        let content = fs::read_to_string(path).unwrap();
        let result = validator::validate_content(&content).unwrap();
        assert!(result.valid);
        assert!(result.errors.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_validator_fails_missing_capabilities() {
        let content = "# AGENTS.md — test\n\n> A description\n\n## Endpoints\n\n";
        let result = validator::validate_content(content).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Capabilities")));
    }

    #[test]
    fn test_validator_fails_missing_heading() {
        let content = "just some random text with no heading";
        let result = validator::validate_content(content).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("heading")));
    }

    #[test]
    fn test_validator_warns_missing_description() {
        let content = "# AGENTS.md — test\n\n## Capabilities\n\n### `do_thing`\n\nDoes a thing.\n\n";
        let result = validator::validate_content(content).unwrap();
        assert!(result.warnings.iter().any(|w| w.contains("description")));
    }
}

#[cfg(test)]
mod db_tests {
    use crate::db;

    #[tokio::test]
    async fn test_payment_already_used_returns_false_for_unknown_hash() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let result = db::payment_already_used("nonexistent_hash_xyz_123").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_create_run_returns_uuid() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let result = db::create_run("test-repo").await;
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn test_full_run_lifecycle() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let run_id = db::create_run("test-lifecycle-repo").await.unwrap();
        
        let paid = db::mark_run_paid(&run_id, "0xtest_txn_hash_123", "base").await;
        assert!(paid.is_ok());

        let complete = db::mark_run_complete(&run_id, "# AGENTS.md\n\n> test").await;
        assert!(complete.is_ok());
    }

    #[tokio::test]
    async fn test_mark_run_failed() {
        dotenvy::dotenv().ok();
        if std::env::var("SUPABASE_URL").is_err() {
            return;
        }
        let run_id = db::create_run("test-fail-repo").await.unwrap();
        let result = db::mark_run_failed(&run_id, "inference failed").await;
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod api_tests {
    use crate::{AppState, rate_limit_middleware};
    use axum::{routing::get, Router, middleware, http::{HeaderName, HeaderValue}};
    use axum_test::TestServer;
    use std::sync::Arc;

    async fn mock_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    #[ignore]
    async fn test_rate_limiter_enforces_limit() {
        dotenvy::dotenv().ok();
        let redis_url = match std::env::var("REDIS_URL") {
            Ok(url) => url,
            Err(_) => return,
        };

        let state = AppState {
            redis_client: Arc::new(redis::Client::open(redis_url).unwrap()),
        };

        let app = Router::new()
            .route("/generate", get(mock_handler))
            .layer(middleware::from_fn_with_state(state.clone(), rate_limit_middleware))
            .with_state(state);

        let server = TestServer::new(app).unwrap();

        let header_name = HeaderName::from_static("x-forwarded-for");
        let header_value = HeaderValue::from_static("1.2.3.4");



        for _ in 0..20 {
            let response = server.get("/generate")
                .add_header(header_name.clone(), header_value.clone())
                .await;
            assert_ne!(response.status_code(), 429);
        }

        
        
        let response = server.get("/generate")
            .add_header(header_name, header_value)
            .await;
        assert_eq!(response.status_code(), 429);
    }
}