set dotenv-load := true
set dotenv-path := ".envs/.local/app.env"
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Generate local secrets without replacing existing settings.
init:
    cargo xtask init

infra-up:
    docker compose -f compose/compose.local.yml up -d --wait

infra-down:
    docker compose -f compose/compose.local.yml --profile files --profile search down

infra-up-files:
    docker compose -f compose/compose.local.yml --profile files --profile search up -d --wait

migrate:
    cargo run --locked --package migration

api:
    cargo run --locked --bin api

worker:
    cargo run --locked --bin worker

maintenance:
    cargo run --locked --bin maintenance

manage *arguments:
    cargo run --locked --bin manage -- {{arguments}}

fmt:
    cargo fmt --all

check:
    cargo xtask check

test-db:
    cargo xtask test-db

test-api:
    cargo xtask test-api

test-process:
    cargo xtask test-process

test-files:
    cargo xtask test-files

test-content:
    cargo xtask test-content

test-deployment:
    cargo xtask test-deployment

test-production:
    cargo xtask test-production

test-data-services:
    cargo xtask test-data-services

# Requires a release image in K3S_TEST_IMAGE, Docker, k3d, and kubectl.
test-k3s:
    cargo xtask test-k3s

deploy-k3s environment mode="rolling":
    just --justfile deploy/justfile up {{quote(environment)}} {{quote(mode)}}

load-test:
    cargo xtask load-test

openapi:
    cargo xtask openapi

template-check:
    cargo xtask template-check
