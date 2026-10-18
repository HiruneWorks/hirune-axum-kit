use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{path::Path, process::Command};

pub const NAMESPACE: &str = "hirune-kit";
pub const APPLICATIONS: &[&str] = &["hirune-api", "hirune-worker", "hirune-maintenance"];

pub fn deploy_from_environment() -> Result<()> {
    let mode = std::env::args().nth(2).context(
        "choose deploy-k3s rolling for compatible changes, or deploy-k3s drain for an offline upgrade",
    )?;
    ensure!(
        matches!(mode.as_str(), "rolling" | "drain"),
        "unknown deployment mode"
    );
    let directory = std::env::var("K3S_DIRECTORY").unwrap_or_else(|_| "deploy/k3s".into());
    deploy(Path::new(&directory), mode == "drain")
}

pub fn kubectl(arguments: &[&str]) -> Command {
    let mut command = Command::new("kubectl");
    command.args(["--namespace", NAMESPACE]).args(arguments);
    command
}

pub fn json(arguments: &[&str]) -> Result<Value> {
    let output = kubectl(arguments).arg("--output=json").output()?;
    ensure!(output.status.success(), "kubectl read failed");
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn deploy(directory: &Path, drain: bool) -> Result<()> {
    let directory = directory.to_str().context("deployment path is not UTF-8")?;
    // Refuse an unfinished Job, including one that is still pending scheduling.
    let output = kubectl(&[
        "get",
        "job",
        "hirune-migration",
        "--ignore-not-found",
        "-o",
        "json",
    ])
    .output()?;
    ensure!(output.status.success(), "cannot inspect migration Job");
    let previous_job = if output.stdout.is_empty() {
        None
    } else {
        let job: Value = serde_json::from_slice(&output.stdout)?;
        let finished = job["status"]["conditions"]
            .as_array()
            .is_some_and(|conditions| {
                conditions.iter().any(|condition| {
                    condition["status"] == "True"
                        && matches!(condition["type"].as_str(), Some("Complete" | "Failed"))
                })
            });
        ensure!(
            finished,
            "migration Job is unfinished; inspect it before another deployment"
        );
        Some(job)
    };
    if drain {
        let autoscalers = json(&["get", "hpa"])?;
        ensure!(
            autoscalers["items"].as_array().is_some_and(Vec::is_empty),
            "remove autoscalers before a drained upgrade so they cannot restore old replicas"
        );
        for name in APPLICATIONS {
            let existing = kubectl(&[
                "get",
                "deployment",
                name,
                "--ignore-not-found",
                "-o",
                "name",
            ])
            .output()?;
            ensure!(
                existing.status.success(),
                "cannot inspect application Deployment"
            );
            if !existing.stdout.is_empty() {
                crate::run(&mut kubectl(&["scale", "deployment", name, "--replicas=0"]))?;
            }
        }
        crate::run(&mut kubectl(&[
            "wait",
            "--for=delete",
            "pod",
            "--selector",
            "app.kubernetes.io/component in (api,worker,maintenance)",
            "--timeout=300s",
        ]))?;
    }
    if previous_job.is_some() {
        crate::run(&mut kubectl(&[
            "delete",
            "job",
            "hirune-migration",
            "--wait=true",
        ]))?;
    }
    crate::run(&mut kubectl(&[
        "apply",
        "-k",
        directory,
        "--selector",
        "app.kubernetes.io/component=migration",
    ]))?;
    crate::run(&mut kubectl(&[
        "wait",
        "--for=condition=complete",
        "job/hirune-migration",
        "--timeout=600s",
    ]))?;
    crate::run(&mut kubectl(&[
        "apply",
        "-k",
        directory,
        "--selector",
        "app.kubernetes.io/component!=migration",
    ]))?;
    for name in APPLICATIONS
        .iter()
        .copied()
        .chain(["hirune-apisix", "hirune-cloudflared"])
    {
        crate::run(&mut kubectl(&[
            "rollout",
            "status",
            &format!("deployment/{name}"),
            "--timeout=300s",
        ]))?;
    }
    Ok(())
}
