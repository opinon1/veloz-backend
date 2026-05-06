# Veloz Infra (CDK / TypeScript)

Provisions the AWS resources Veloz runs on:

- **VPC** — 2 AZ, public + isolated subnets, **no NAT** (saves ~$32/mo). Fargate task gets a public IP and pulls from ECR / Secrets Manager directly.
- **ECR** repo `veloz` (image lifecycle: keep last 10).
- **ECS Fargate Spot** service — 1 task on 0.25 vCPU / 0.5 GiB ARM64. Auto-scales 1→3 on CPU 70%. Circuit-breaker rolls back failed deploys.
- **ALB** (HTTP, port 80). Health-checks `/health`.
- **RDS Postgres 17** `db.t4g.micro`, Single-AZ, isolated subnets, 3-day backups.
- **ElastiCache Redis 7** `cache.t4g.micro`, single node, transit + at-rest encryption, AUTH token from Secrets Manager.
- **Secrets Manager** entries: `veloz/db`, `veloz/redis`.
- **GitHub OIDC** trust + `GithubActionsDeployRole` (in a separate stack so the app stack can be torn down without losing it).

Self-heal: ECS auto-restarts crashed tasks and replaces Spot evictions. ALB drops unhealthy targets.

## Cost estimate (us-west-1)

| Item | $/mo |
|---|---|
| Fargate Spot (1× 0.25 vCPU / 0.5 GiB, 24/7) | ~$3 |
| ALB | ~$16 |
| RDS db.t4g.micro Single-AZ | ~$13 |
| ElastiCache cache.t4g.micro | ~$11 |
| ECR + Secrets + logs + light egress | ~$2 |
| **Total** | **~$45** |

## First-time deploy

Run from your laptop. Needs AWS creds (`aws sts get-caller-identity` should already work — current account `346481751619`).

```bash
cd infra
npm install

# Bootstrap CDK in the target account/region (one-time per account/region):
npx cdk bootstrap aws://346481751619/us-west-1

# Deploy the OIDC stack first (creates the role GH Actions will assume):
npx cdk deploy VelozGithubOidc

# Note the DeployRoleArn output. Add it as a GitHub secret:
#   gh secret set AWS_DEPLOY_ROLE_ARN -b "<arn>"

# Deploy the app stack:
npx cdk deploy VelozStack
```

Outputs after `VelozStack`:

- `AlbDns` — public hostname. App reachable at `http://<AlbDns>/`.
- `RepoUri` — ECR URI for image pushes.
- `ClusterName` / `ServiceName` — for `aws ecs update-service`.
- `DbEndpoint` / `RedisEndpoint` — internal-only.

> The first `cdk deploy VelozStack` will fail to start the ECS task because the
> `veloz:latest` image doesn't exist in ECR yet. Push a first image (the GH
> Actions workflow will do this on the next master push), then ECS will pull
> and start.

## CI/CD

`.github/workflows/ci.yml`:

1. **test** — every push/PR. Spins up `docker-compose`, runs the 213 pytest integ tests.
2. **build-and-deploy** — on master push only.
   1. Assumes `GithubActionsDeployRole` via OIDC.
   2. Builds ARM64 image, pushes to ECR (`:latest` and `:<sha>`).
   3. `aws ecs update-service --force-new-deployment` to roll out.
   4. Waits for service stable.

Required GitHub secret:

- `AWS_DEPLOY_ROLE_ARN` — ARN of `GithubActionsDeployRole` (output of `VelozGithubOidc`).

## Override account / region / repo

```bash
# Different region:
CDK_DEFAULT_REGION=us-east-1 npx cdk deploy VelozStack

# Different GitHub repo for OIDC trust:
npx cdk deploy VelozGithubOidc -c githubOwner=myorg -c githubRepo=myrepo
```

## Tearing down

```bash
npx cdk destroy VelozStack          # leaves ECR repo (RETAIN policy)
npx cdk destroy VelozGithubOidc     # only if you want to revoke CI access
```
