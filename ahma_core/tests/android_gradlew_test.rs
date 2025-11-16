mod common;

#[cfg(feature = "android")]
mod android_tests {
    use crate::common::{
        get_workspace_dir,
        test_utils::{dir_exists, file_exists},
    };
    use anyhow::Result;
    use serde_json::Value;
    use std::{future::Future, path::Path, pin::Pin};
    use tempfile::TempDir;
    use tokio::task::spawn_blocking;

    /// Copy the Android test project to a unique temporary directory
    async fn copy_android_test_project() -> Result<TempDir> {
        let process_id = std::process::id();
        let temp_dir = spawn_blocking(move || {
            tempfile::Builder::new()
                .prefix(&format!("android_test_{}_", process_id))
                .tempdir()
        })
        .await??;

        // Copy test-data/AndoidTestBasicViews/ to temp directory
        let source_dir = get_workspace_dir().join("test-data/AndoidTestBasicViews");
        copy_dir_recursive(&source_dir, temp_dir.path()).await?;

        Ok(temp_dir)
    }

    /// Validate that a file name is safe (no path traversal)
    fn is_safe_filename(name: &std::ffi::OsStr) -> bool {
        let name_str = name.to_string_lossy();
        // Reject paths containing dangerous sequences
        !name_str.contains("..") && !name_str.contains('/') && !name_str.contains('\\')
    }

    /// Async recursive directory copy using tokio::fs with proper recursion handling and path validation
    fn copy_dir_recursive<'a>(
        src: &'a Path,
        dst: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(dst).await?;
            let mut entries = tokio::fs::read_dir(src).await?;

            while let Some(entry) = entries.next_entry().await? {
                let file_name = entry.file_name();

                // Validate filename for path traversal attacks
                if !is_safe_filename(&file_name) {
                    eprintln!("Skipping unsafe filename: {:?}", file_name);
                    continue;
                }

                let src_path = src.join(&file_name);
                let dst_path = dst.join(&file_name);

                if src_path.is_dir() {
                    copy_dir_recursive(&src_path, &dst_path).await?;
                } else {
                    tokio::fs::copy(&src_path, &dst_path).await?;
                }
            }
            Ok(())
        })
    }

    /// Test that Android test project can be copied successfully
    #[tokio::test]
    async fn test_copy_android_test_project() -> Result<()> {
        let temp_dir = copy_android_test_project().await?;
        let project_path = temp_dir.path();

        // Verify essential Android project files exist
        assert!(
            file_exists(&project_path.join("gradlew")).await,
            "gradlew script should exist"
        );
        assert!(
            file_exists(&project_path.join("build.gradle.kts")).await,
            "Root build.gradle.kts should exist"
        );
        assert!(
            dir_exists(&project_path.join("app")).await,
            "app directory should exist"
        );
        assert!(
            file_exists(&project_path.join("app/build.gradle.kts")).await,
            "App build.gradle.kts should exist"
        );

        // Verify gradlew is executable (on Unix systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(project_path.join("gradlew")).await?;
            let permissions = metadata.permissions();
            assert!(
                permissions.mode() & 0o111 != 0,
                "gradlew should be executable"
            );
        }

        Ok(())
    }

    /// Test that copied projects are unique and don't interfere with each other
    #[tokio::test]
    async fn test_concurrent_android_project_isolation() -> Result<()> {
        // Create multiple temporary projects concurrently
        let tasks: Vec<_> = (0..3)
            .map(|_| tokio::spawn(copy_android_test_project()))
            .collect();

        let temp_dirs = futures::future::try_join_all(tasks).await?;
        let temp_dirs: Result<Vec<_>> = temp_dirs.into_iter().collect();
        let temp_dirs = temp_dirs?;

        // Verify all projects are in different directories
        let paths: Vec<_> = temp_dirs.iter().map(|td| td.path()).collect();
        for (i, path1) in paths.iter().enumerate() {
            for (j, path2) in paths.iter().enumerate() {
                if i != j {
                    assert_ne!(path1, path2, "Projects should be in different directories");
                }
            }
        }

        // Verify each project is complete
        for temp_dir in &temp_dirs {
            let project_path = temp_dir.path();
            assert!(
                file_exists(&project_path.join("gradlew")).await,
                "Each project should have gradlew"
            );
            assert!(
                file_exists(&project_path.join("build.gradle.kts")).await,
                "Each project should have build.gradle.kts"
            );
        }

        Ok(())
    }

    /// Test Android gradlew tool definition loading
    #[tokio::test]
    async fn test_gradlew_tool_definition_loading() -> Result<()> {
        let workspace_dir = get_workspace_dir();
        let tools_dir = workspace_dir.join(".ahma/tools");
        let gradlew_tool_path = tools_dir.join("gradlew.json");

        // Verify tool definition file exists
        assert!(
            file_exists(&gradlew_tool_path).await,
            "gradlew.json tool definition should exist"
        );

        // Load and parse the tool definition
        let tool_content = tokio::fs::read_to_string(&gradlew_tool_path).await?;
        let tool_def: Value = serde_json::from_str(&tool_content)?;

        // Verify basic tool structure
        assert_eq!(tool_def["name"].as_str().unwrap(), "gradlew");
        assert_eq!(tool_def["command"].as_str().unwrap(), "./gradlew");
        assert!(tool_def["enabled"].as_bool().unwrap());

        // Verify timeout is reasonable for Android builds
        let timeout = tool_def["timeout_seconds"].as_u64().unwrap();
        assert!(
            timeout >= 300,
            "Android builds need at least 5 minutes timeout"
        );
        assert!(timeout <= 1800, "Timeout should not exceed 30 minutes");

        // Verify subcommands array exists and is non-empty
        let subcommands = tool_def["subcommand"].as_array().unwrap();
        assert!(
            !subcommands.is_empty(),
            "Should have Android gradle subcommands"
        );

        Ok(())
    }

    /// Test that essential Android gradlew subcommands are defined
    #[tokio::test]
    async fn test_gradlew_essential_subcommands() -> Result<()> {
        let workspace_dir = get_workspace_dir();
        let gradlew_tool_path = workspace_dir.join(".ahma/tools/gradlew.json");
        let tool_content = tokio::fs::read_to_string(&gradlew_tool_path).await?;
        let tool_def: Value = serde_json::from_str(&tool_content)?;

        let subcommands = tool_def["subcommand"].as_array().unwrap();
        let subcommand_names: Vec<_> = subcommands
            .iter()
            .map(|sc| sc["name"].as_str().unwrap())
            .collect();

        // Verify essential Android tasks are present
        let essential_tasks = [
            "tasks",
            "help",
            "clean",
            "build",
            "assemble",
            "assembleDebug",
            "assembleRelease",
            "test",
            "testDebugUnitTest",
            "testReleaseUnitTest",
            "installDebug",
            "lint",
            "dependencies",
            "properties",
            "signingReport",
            "sourceSets",
        ];

        for task in essential_tasks.iter() {
            assert!(
                subcommand_names.contains(task),
                "Essential Android task '{}' should be defined. Available: {:?}",
                task,
                subcommand_names
            );
        }

        Ok(())
    }

    /// Test async vs sync operation classification
    #[tokio::test]
    async fn test_gradlew_async_sync_classification() -> Result<()> {
        let workspace_dir = get_workspace_dir();
        let gradlew_tool_path = workspace_dir.join(".ahma/tools/gradlew.json");
        let tool_content = tokio::fs::read_to_string(&gradlew_tool_path).await?;
        let tool_def: Value = serde_json::from_str(&tool_content)?;

        let subcommands = tool_def["subcommand"].as_array().unwrap();

        // Tasks that should be synchronous (quick info tasks)
        let sync_tasks = [
            "tasks",
            "help",
            "dependencies",
            "properties",
            "signingReport",
            "sourceSets",
            "androidDependencies",
            "clean",
            "checkJetifier",
            "checkKotlinGradlePluginConfigurationErrors",
        ];

        // Tasks that should be async (longer operations)
        let async_tasks = [
            "build",
            "assemble",
            "assembleDebug",
            "assembleRelease",
            "test",
            "testDebugUnitTest",
            "installDebug",
            "lint",
            "compileDebugSources",
        ];

        for subcommand in subcommands {
            let name = subcommand["name"].as_str().unwrap();
            // force_synchronous: true means always sync, false/None means can be async (obeys --async)
            let is_forced_sync = subcommand
                .get("force_synchronous")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let guidance_key = subcommand.get("guidance_key").and_then(|v| v.as_str());

            if sync_tasks.contains(&name) {
                assert!(
                    is_forced_sync,
                    "Task '{}' should be marked as force_synchronous to stay synchronous",
                    name
                );
                assert_eq!(
                    guidance_key,
                    Some("sync_behavior"),
                    "Sync task '{}' should have sync_behavior guidance",
                    name
                );
            } else if async_tasks.contains(&name) {
                assert!(
                    !is_forced_sync,
                    "Task '{}' should NOT be force_synchronous (can run async)",
                    name
                );
                assert_eq!(
                    guidance_key,
                    Some("async_behavior"),
                    "Async task '{}' should have async_behavior guidance",
                    name
                );
            }
        }

        Ok(())
    }

    /// Test gradlew tool validation against MTDF schema
    #[tokio::test]
    async fn test_gradlew_tool_schema_validation() -> Result<()> {
        let workspace_dir = get_workspace_dir();
        let gradlew_tool_path = workspace_dir.join(".ahma/tools/gradlew.json");
        let tool_content = tokio::fs::read_to_string(&gradlew_tool_path).await?;
        let tool_def: Value = serde_json::from_str(&tool_content)?;

        // Verify required top-level fields
        assert!(
            tool_def.get("name").is_some(),
            "Tool must have 'name' field"
        );
        assert!(
            tool_def.get("description").is_some(),
            "Tool must have 'description' field"
        );
        assert!(
            tool_def.get("command").is_some(),
            "Tool must have 'command' field"
        );
        assert!(
            tool_def.get("enabled").is_some(),
            "Tool must have 'enabled' field"
        );
        assert!(
            tool_def.get("subcommand").is_some(),
            "Tool must have 'subcommand' field"
        );

        // Verify subcommands structure
        let subcommands = tool_def["subcommand"].as_array().unwrap();
        for (i, subcommand) in subcommands.iter().enumerate() {
            assert!(
                subcommand.get("name").is_some(),
                "Subcommand {} must have 'name' field",
                i
            );
            assert!(
                subcommand.get("description").is_some(),
                "Subcommand {} must have 'description' field",
                i
            );

            // Check options structure if present
            if let Some(options) = subcommand.get("options") {
                let options_array = options.as_array().unwrap();
                for (j, option) in options_array.iter().enumerate() {
                    assert!(
                        option.get("name").is_some(),
                        "Option {} in subcommand {} must have 'name' field",
                        j,
                        i
                    );
                    assert!(
                        option.get("type").is_some(),
                        "Option {} in subcommand {} must have 'type' field",
                        j,
                        i
                    );
                    assert!(
                        option.get("description").is_some(),
                        "Option {} in subcommand {} must have 'description' field",
                        j,
                        i
                    );
                }
            }
        }

        Ok(())
    }

    /// Test that gradlew commands include working_directory option
    #[tokio::test]
    async fn test_gradlew_working_directory_support() -> Result<()> {
        let workspace_dir = get_workspace_dir();
        let gradlew_tool_path = workspace_dir.join(".ahma/tools/gradlew.json");
        let tool_content = tokio::fs::read_to_string(&gradlew_tool_path).await?;
        let tool_def: Value = serde_json::from_str(&tool_content)?;

        let subcommands = tool_def["subcommand"].as_array().unwrap();

        for subcommand in subcommands {
            let name = subcommand["name"].as_str().unwrap();
            let options = subcommand.get("options").and_then(|v| v.as_array());

            if let Some(options_array) = options {
                let has_working_directory = options_array.iter().any(|opt| {
                    opt.get("name").and_then(|v| v.as_str()) == Some("working_directory")
                });

                assert!(
                    has_working_directory,
                    "Subcommand '{}' should have working_directory option",
                    name
                );

                // Verify working_directory option has correct format
                let working_dir_option = options_array
                    .iter()
                    .find(|opt| {
                        opt.get("name").and_then(|v| v.as_str()) == Some("working_directory")
                    })
                    .unwrap();

                assert_eq!(
                    working_dir_option.get("type").and_then(|v| v.as_str()),
                    Some("string"),
                    "working_directory should be string type"
                );
                assert_eq!(
                    working_dir_option.get("format").and_then(|v| v.as_str()),
                    Some("path"),
                    "working_directory should have path format"
                );
            }
        }

        Ok(())
    }
}
