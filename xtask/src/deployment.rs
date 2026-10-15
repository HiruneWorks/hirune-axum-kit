use anyhow::{Context, Result, ensure};
use std::{fs, process::Command};

pub fn verify() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    fs::create_dir_all(root.join("deploy"))?;
    fs::create_dir_all(root.join(".envs/.production"))?;
    for (source, destination) in [
        ("deploy/compose.yml", "deploy/compose.yml"),
        ("deploy/production.env.example", "deploy/production.env"),
        (
            ".envs/.example/production/app.env",
            ".envs/.production/app.env",
        ),
        (
            ".envs/.example/production/migration.env",
            ".envs/.production/migration.env",
        ),
        (
            ".envs/.example/production/maintenance.env",
            ".envs/.production/maintenance.env",
        ),
    ] {
        fs::copy(source, root.join(destination))?;
    }
    let output = Command::new("docker")
        .current_dir(root)
        .env("DEPLOY_ENV", "production")
        .env("COMPOSE_PROJECT_NAME", "kit-deployment-check")
        .env("APP_IMAGE", "hirune-kit:verification")
        .env("API_PORT", "3000")
        .args([
            "compose",
            "--profile",
            "*",
            "--env-file",
            "deploy/production.env",
            "-f",
            "deploy/compose.yml",
            "config",
            "--format",
            "json",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "deployment Compose validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let services = &rendered["services"];
    for name in ["api", "worker", "manage"] {
        ensure!(
            services[name]["environment"]
                .get("MIGRATION_DATABASE_URL")
                .is_none(),
            "runtime received migration credentials"
        );
        ensure!(
            services[name]["environment"]["DATABASE_URL"]
                .as_str()
                .context("runtime database missing")?
                .contains("runtime:"),
            "runtime must use its own role"
        );
        ensure!(
            services[name]["read_only"] == true,
            "runtime filesystem must be read-only"
        );
    }
    ensure!(
        services["maintenance"]["environment"]["DATABASE_URL"]
            .as_str()
            .context("maintenance database missing")?
            .contains("maintenance:"),
        "maintenance must use its own role"
    );
    ensure!(
        services["maintenance"]["environment"]
            .get("MIGRATION_DATABASE_URL")
            .is_none(),
        "maintenance received schema-owner credentials"
    );
    ensure!(
        services["migration"]["environment"]
            .get("DATABASE_URL")
            .is_none(),
        "migration environment must be separate"
    );
    ensure!(
        services["api"]["ports"][0]["host_ip"] == "127.0.0.1",
        "API port must be private by default"
    );
    let local = Command::new("docker")
        .args([
            "compose",
            "-f",
            "compose/compose.local.yml",
            "config",
            "--quiet",
        ])
        .status()?;
    ensure!(local.success(), "local Compose validation failed");
    println!("Local and deployment Compose contracts passed.");
    Ok(())
}
