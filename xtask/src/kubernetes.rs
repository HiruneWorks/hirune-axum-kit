use crate::kubernetes_environment::{Deployment, Environment, Target};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{io::Write, path::Path, process::Stdio};

pub const APPLICATIONS: &[&str] = &["hirune-api", "hirune-worker", "hirune-maintenance"];
pub const DEPLOYMENTS: &[&str] = &[
    "hirune-api",
    "hirune-worker",
    "hirune-maintenance",
    "hirune-apisix",
    "hirune-cloudflared",
    "hirune-edge-valkey",
];

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Rolling,
    Drain,
}

impl Mode {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "rolling" => Ok(Self::Rolling),
            "drain" => Ok(Self::Drain),
            _ => bail!("deployment mode must be rolling or drain"),
        }
    }
}

pub fn from_arguments() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(2).collect();
    let action = arguments
        .first()
        .context("select check, up, down, status, logs, manage, or cleanup")?;
    let environment =
        Environment::parse(arguments.get(1).context("select dev or prod explicitly")?)?;
    let extra: Vec<_> = arguments.iter().skip(2).map(String::as_str).collect();
    if matches!(action.as_str(), "check" | "up") {
        let deployment = Deployment::load(Path::new("."), environment)?;
        println!(
            "Target: context={}, namespace={}",
            deployment.target.context, deployment.target.namespace
        );
        match action.as_str() {
            "check" => {
                ensure!(extra.is_empty(), "check takes only an environment");
                println!(
                    "Kubernetes environment rendered and namespace validation passed. Secret values are not displayed."
                );
                Ok(())
            }
            _ => {
                ensure!(
                    extra.len() <= 1,
                    "up takes an environment and optional rolling or drain mode"
                );
                deploy(
                    &deployment,
                    Mode::parse(extra.first().copied().unwrap_or("rolling"))?,
                )
            }
        }
    } else {
        let deployment = Deployment::load(Path::new("."), environment)?;
        let target = deployment.target;
        println!(
            "Target: context={}, namespace={}",
            target.context, target.namespace
        );
        match action.as_str() {
            "down" => {
                ensure!(extra.is_empty(), "down takes only an environment");
                stop(&target, DEPLOYMENTS)
            }
            "status" => {
                ensure!(extra.is_empty(), "status takes only an environment");
                crate::run(&mut target.kubectl(&["get", "deployment,pod,service,job,hpa,pdb"]))
            }
            "logs" => {
                ensure!(
                    extra.len() == 1 && DEPLOYMENTS.contains(&extra[0]),
                    "logs requires a hirune Deployment name"
                );
                crate::run(&mut target.kubectl(&[
                    "logs",
                    "--follow",
                    &format!("deployment/{}", extra[0]),
                    "--all-pods=true",
                    "--all-containers=true",
                ]))
            }
            "manage" => {
                ensure!(!extra.is_empty(), "manage requires a management command");
                crate::run(
                    target
                        .kubectl(&["exec", "deployment/hirune-api", "--", "manage"])
                        .args(extra),
                )
            }
            "cleanup" => {
                ensure!(extra.is_empty(), "cleanup takes only an environment");
                crate::run(&mut target.kubectl(&[
                    "exec",
                    "deployment/hirune-maintenance",
                    "--",
                    "manage",
                    "cleanup",
                ]))
            }
            _ => bail!("unknown Kubernetes deployment action"),
        }
    }
}

pub fn apply(target: &Target, resources: Vec<Value>) -> Result<()> {
    if resources.is_empty() {
        return Ok(());
    }
    // Render once. Send Secret values only on stdin, without printing YAML.
    let mut child = target
        .kubectl(&["apply", "-f", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("kubectl stdin missing")?
        .write_all(&serde_json::to_vec(
            &json!({"apiVersion":"v1","kind":"List","items":resources}),
        )?)?;
    ensure!(
        child.wait_with_output()?.status.success(),
        "resource apply failed for selected target"
    );
    Ok(())
}

pub fn deploy(deployment: &Deployment, mode: Mode) -> Result<()> {
    deployment.validate()?;
    let target = &deployment.target;
    let select = |kind: &str| {
        deployment
            .resources
            .iter()
            .filter(|r| r["kind"] == kind)
            .cloned()
            .collect()
    };
    apply(target, select("Namespace"))?;
    let previous = target
        .kubectl(&[
            "get",
            "job",
            "hirune-migration",
            "--ignore-not-found",
            "-o",
            "json",
        ])
        .output()?;
    ensure!(previous.status.success(), "cannot inspect migration Job");
    if !previous.stdout.is_empty() {
        let job: Value = serde_json::from_slice(&previous.stdout)?;
        ensure!(
            job["status"]["conditions"]
                .as_array()
                .is_some_and(|conditions| conditions.iter().any(|c| c["status"] == "True"
                    && matches!(c["type"].as_str(), Some("Complete" | "Failed")))),
            "migration Job is unfinished; inspect it before another deployment"
        );
    }
    if mode == Mode::Drain {
        ensure!(
            !deployment
                .resources
                .iter()
                .any(|r| r["kind"] == "HorizontalPodAutoscaler"),
            "remove autoscalers from the overlay before a drained upgrade"
        );
        stop(target, APPLICATIONS)?;
    }
    apply(
        target,
        deployment
            .resources
            .iter()
            .filter(|r| !matches!(r["kind"].as_str(), Some("Namespace" | "Job" | "Deployment")))
            .cloned()
            .collect(),
    )?;
    if !previous.stdout.is_empty() {
        crate::run(&mut target.kubectl(&["delete", "job", "hirune-migration", "--wait=true"]))?;
    }
    apply(target, select("Job"))?;
    crate::run(&mut target.kubectl(&[
        "wait",
        "--for=condition=complete",
        "job/hirune-migration",
        "--timeout=600s",
    ]))?;
    apply(
        target,
        deployment
            .resources
            .iter()
            .filter(|r| r["kind"] == "Deployment" && r["metadata"]["name"] == "hirune-edge-valkey")
            .cloned()
            .collect(),
    )?;
    crate::run(&mut target.kubectl(&[
        "rollout",
        "status",
        "deployment/hirune-edge-valkey",
        "--timeout=300s",
    ]))?;
    apply(
        target,
        deployment
            .resources
            .iter()
            .filter(|r| r["kind"] == "Deployment" && r["metadata"]["name"] != "hirune-edge-valkey")
            .cloned()
            .collect(),
    )?;
    for name in DEPLOYMENTS {
        crate::run(&mut target.kubectl(&[
            "rollout",
            "status",
            &format!("deployment/{name}"),
            "--timeout=300s",
        ]))?;
    }
    Ok(())
}

fn stop(target: &Target, deployments: &[&str]) -> Result<()> {
    let autoscalers = target.json(&["get", "hpa"])?;
    ensure!(
        autoscalers["items"].as_array().is_some_and(Vec::is_empty),
        "remove autoscalers from the selected namespace before stopping replicas"
    );
    for name in deployments {
        let existing = target
            .kubectl(&[
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
            crate::run(&mut target.kubectl(&["scale", "deployment", name, "--replicas=0"]))?;
        }
    }
    let selector = if deployments == APPLICATIONS {
        "app.kubernetes.io/part-of=hirune-kit,app.kubernetes.io/component in (api,worker,maintenance)"
    } else {
        "app.kubernetes.io/part-of=hirune-kit,app.kubernetes.io/component in (api,worker,maintenance,gateway,tunnel,edge-counter)"
    };
    crate::run(&mut target.kubectl(&[
        "wait",
        "--for=delete",
        "pod",
        "--selector",
        selector,
        "--timeout=300s",
    ]))
}
