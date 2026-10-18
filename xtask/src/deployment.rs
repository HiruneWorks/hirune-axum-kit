use crate::kubernetes_environment::{Deployment, Environment, copy_directory};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

pub fn verify() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    copy_directory(Path::new("deploy/k3s/base"), &root.join("deploy/k3s/base"))?;
    for environment in [Environment::Dev, Environment::Prod] {
        copy_directory(
            &Path::new(".envs/.example")
                .join(environment.name())
                .join("k3s"),
            &root.join(format!(".envs/.{}/k3s", environment.name())),
        )?;
    }
    let dev = Deployment::load(root, Environment::Dev)?;
    let prod = Deployment::load(root, Environment::Prod)?;
    for (deployment, name) in [(&dev, "dev"), (&prod, "prod")] {
        ensure!(
            deployment.target.context == format!("k3s-{name}"),
            "wrong environment context"
        );
        ensure!(
            deployment.target.namespace == format!("hirune-{name}"),
            "wrong environment namespace"
        );
        verify_roles(deployment)?;
        ensure!(
            resource(deployment, "hirune-api")["spec"]["replicas"] == 2,
            "two API replicas required"
        );
        for service in deployment
            .resources
            .iter()
            .filter(|r| r["kind"] == "Service")
        {
            ensure!(
                service["spec"]["type"] == "ClusterIP",
                "ingress must stay private"
            );
        }
        for policy in deployment
            .resources
            .iter()
            .filter(|r| r["kind"] == "NetworkPolicy")
        {
            ensure!(
                !policy.to_string().contains("namespaceSelector"),
                "ingress peers must stay within the selected namespace"
            );
        }
        let secrets = generated_secret(deployment, "hirune-app-secrets-");
        let keys = base64::engine::general_purpose::STANDARD.decode(
            secrets["data"]["SECRET_KEYS"]
                .as_str()
                .context("keys missing")?,
        )?;
        let _: Value = serde_json::from_slice(&keys)
            .context("Kubernetes JSON must use raw env-file values")?;
    }
    let secrets = root.join(".envs/.prod/k3s/secrets.env");
    fs::write(
        &secrets,
        fs::read_to_string(&secrets)?
            .replace("SMTP_PASSWORD=", "SMTP_PASSWORD=changed-for-rollout"),
    )?;
    let changed = Deployment::load(root, Environment::Prod)?;
    for name in crate::kubernetes::APPLICATIONS {
        ensure!(
            resource(&prod, name)["spec"]["template"]
                != resource(&changed, name)["spec"]["template"],
            "Secret changes must replace affected application Pods"
        );
    }
    ensure!(
        resource(&prod, "hirune-cloudflared")["spec"]["template"]
            == resource(&changed, "hirune-cloudflared")["spec"]["template"],
        "application Secret changes must not replace tunnel Pods"
    );
    ensure!(
        Deployment::load(root, Environment::Dev)?.resources == dev.resources,
        "changing prod settings changed dev resources"
    );
    let before_owner_change = changed;
    let migration = root.join(".envs/.prod/k3s/migration.env");
    fs::write(
        &migration,
        fs::read_to_string(&migration)?.replace("REPLACE_ME", "new-owner-password"),
    )?;
    let changed = Deployment::load(root, Environment::Prod)?;
    for name in crate::kubernetes::APPLICATIONS {
        ensure!(
            resource(&before_owner_change, name) == resource(&changed, name),
            "schema-owner Secret leaked into runtime"
        );
    }
    ensure!(
        resource(&before_owner_change, "hirune-migration")
            != resource(&changed, "hirune-migration"),
        "migration Job must use the new owner Secret"
    );
    fs::write(
        root.join(".envs/.prod/k3s/target.env"),
        "KUBE_CONTEXT=k3s-prod\nKUBE_NAMESPACE=hirune-dev\n",
    )?;
    ensure!(
        Deployment::load(root, Environment::Prod).is_err(),
        "namespace mismatch must fail before mutation"
    );
    ensure!(
        Environment::parse("production").is_err(),
        "unknown deployment environment accepted"
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
    println!(
        "Kubernetes dev/prod isolation, role separation, Secret-triggered rollouts, namespace validation, and local Compose contracts passed."
    );
    Ok(())
}

fn resource<'a>(deployment: &'a Deployment, name: &str) -> &'a Value {
    deployment
        .resources
        .iter()
        .find(|r| {
            matches!(r["kind"].as_str(), Some("Deployment" | "Job"))
                && r["metadata"]["name"] == name
        })
        .expect("required resource missing")
}

fn generated_secret<'a>(deployment: &'a Deployment, prefix: &str) -> &'a Value {
    deployment
        .resources
        .iter()
        .find(|r| {
            r["kind"] == "Secret"
                && r["metadata"]["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(prefix))
        })
        .expect("generated Secret missing")
}

fn verify_roles(deployment: &Deployment) -> Result<()> {
    let runtime = generated_secret(deployment, "hirune-runtime-database-");
    let maintenance = generated_secret(deployment, "hirune-maintenance-database-");
    let owner = generated_secret(deployment, "hirune-migration-database-");
    for (secret, key, role) in [
        (runtime, "DATABASE_URL", "runtime:"),
        (maintenance, "DATABASE_URL", "maintenance:"),
        (owner, "MIGRATION_DATABASE_URL", "schema_owner:"),
    ] {
        let values = secret["data"].as_object().context("Secret data missing")?;
        ensure!(
            values.len() == 1,
            "database roles must use separate Secrets"
        );
        let value = base64::engine::general_purpose::STANDARD.decode(
            values[key]
                .as_str()
                .context("database URL missing")?
                .split_whitespace()
                .collect::<String>(),
        )?;
        ensure!(
            String::from_utf8(value)?.contains(role),
            "incorrect example database role"
        );
    }
    for (name, database) in [
        ("hirune-api", runtime),
        ("hirune-worker", runtime),
        ("hirune-maintenance", maintenance),
        ("hirune-migration", owner),
    ] {
        let workload = resource(deployment, name);
        let container = &workload["spec"]["template"]["spec"]["containers"][0];
        ensure!(
            container["securityContext"]["readOnlyRootFilesystem"] == true,
            "application filesystem must be read-only"
        );
        let reference = &container["env"][0]["valueFrom"]["secretKeyRef"]["name"];
        ensure!(
            reference == &database["metadata"]["name"],
            "incorrect database Secret reference"
        );
        if name != "hirune-migration" {
            ensure!(
                !container.to_string().contains("hirune-migration-database"),
                "runtime received owner credentials"
            );
        }
    }
    Ok(())
}
