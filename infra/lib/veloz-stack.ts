import * as cdk from "aws-cdk-lib";
import { Construct } from "constructs";
import * as ec2 from "aws-cdk-lib/aws-ec2";
import * as ecs from "aws-cdk-lib/aws-ecs";
import * as ecr from "aws-cdk-lib/aws-ecr";
import * as elbv2 from "aws-cdk-lib/aws-elasticloadbalancingv2";
import * as logs from "aws-cdk-lib/aws-logs";
import * as rds from "aws-cdk-lib/aws-rds";
import * as elasticache from "aws-cdk-lib/aws-elasticache";
import * as secrets from "aws-cdk-lib/aws-secretsmanager";
import * as iam from "aws-cdk-lib/aws-iam";

interface VelozStackProps extends cdk.StackProps {
  repositoryName: string;
}

export class VelozStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: VelozStackProps) {
    super(scope, id, props);

    // ──────────────────────────────────────────────────────────
    // VPC: 2 AZ, public subnets only (no NAT — saves ~$32/mo).
    // Fargate task lives in public subnet with public IP so it can
    // pull from ECR + reach Secrets Manager without NAT.
    // RDS + ElastiCache live in isolated subnets, reached only from
    // task SG inside the VPC.
    // ──────────────────────────────────────────────────────────
    const vpc = new ec2.Vpc(this, "Vpc", {
      maxAzs: 2,
      natGateways: 0,
      subnetConfiguration: [
        {
          name: "public",
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 24,
        },
        {
          name: "isolated",
          subnetType: ec2.SubnetType.PRIVATE_ISOLATED,
          cidrMask: 24,
        },
      ],
    });

    // ──────────────────────────────────────────────────────────
    // ECR: imported from VelozEcrStack so the registry exists (and
    // the first image has been pushed) before this stack tries to
    // start the ECS service.
    // ──────────────────────────────────────────────────────────
    const repo = ecr.Repository.fromRepositoryName(
      this,
      "Repo",
      props.repositoryName
    );

    // ──────────────────────────────────────────────────────────
    // Secrets: DB password (auto-generated), Redis auth token.
    // ──────────────────────────────────────────────────────────
    const dbSecret = new secrets.Secret(this, "DbSecret", {
      secretName: "veloz/db",
      generateSecretString: {
        secretStringTemplate: JSON.stringify({ username: "veloz" }),
        generateStringKey: "password",
        excludePunctuation: true,
        passwordLength: 24,
      },
    });

    const redisAuthToken = new secrets.Secret(this, "RedisAuth", {
      secretName: "veloz/redis",
      generateSecretString: {
        passwordLength: 32,
        excludePunctuation: true,
      },
    });

    // Etomin credentials. Imported by name so an operator can populate the
    // secret out-of-band (CDK won't overwrite it on subsequent deploys).
    // Schema: { email: string, password: string }.
    const etominSecret = secrets.Secret.fromSecretNameV2(
      this,
      "EtominSecret",
      "veloz/etomin"
    );

    // ──────────────────────────────────────────────────────────
    // Security groups.
    // ──────────────────────────────────────────────────────────
    const taskSg = new ec2.SecurityGroup(this, "TaskSg", {
      vpc,
      description: "Fargate task SG",
      allowAllOutbound: true,
    });

    const dbSg = new ec2.SecurityGroup(this, "DbSg", {
      vpc,
      description: "RDS Postgres SG",
      allowAllOutbound: false,
    });
    dbSg.addIngressRule(taskSg, ec2.Port.tcp(5432), "task to pg");

    const redisSg = new ec2.SecurityGroup(this, "RedisSg", {
      vpc,
      description: "ElastiCache Redis SG",
      allowAllOutbound: false,
    });
    redisSg.addIngressRule(taskSg, ec2.Port.tcp(6379), "task to redis");

    // ──────────────────────────────────────────────────────────
    // RDS Postgres 17, db.t4g.micro, Single-AZ, isolated subnets.
    // ──────────────────────────────────────────────────────────
    const db = new rds.DatabaseInstance(this, "Db", {
      engine: rds.DatabaseInstanceEngine.postgres({
        version: rds.PostgresEngineVersion.VER_17,
      }),
      vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_ISOLATED },
      instanceType: ec2.InstanceType.of(
        ec2.InstanceClass.BURSTABLE4_GRAVITON,
        ec2.InstanceSize.MICRO
      ),
      credentials: rds.Credentials.fromSecret(dbSecret),
      databaseName: "veloz_db",
      allocatedStorage: 20,
      storageType: rds.StorageType.GP3,
      multiAz: false,
      publiclyAccessible: false,
      securityGroups: [dbSg],
      backupRetention: cdk.Duration.days(3),
      deleteAutomatedBackups: true,
      removalPolicy: cdk.RemovalPolicy.SNAPSHOT,
      autoMinorVersionUpgrade: true,
    });

    // ──────────────────────────────────────────────────────────
    // ElastiCache Redis 7, cache.t4g.micro, single node.
    // ──────────────────────────────────────────────────────────
    const redisSubnetGroup = new elasticache.CfnSubnetGroup(
      this,
      "RedisSubnetGroup",
      {
        description: "Veloz Redis subnets",
        subnetIds: vpc.selectSubnets({
          subnetType: ec2.SubnetType.PRIVATE_ISOLATED,
        }).subnetIds,
      }
    );

    const redis = new elasticache.CfnReplicationGroup(this, "Redis", {
      replicationGroupDescription: "Veloz sessions",
      engine: "redis",
      engineVersion: "7.1",
      cacheNodeType: "cache.t4g.micro",
      numCacheClusters: 1,
      automaticFailoverEnabled: false,
      cacheSubnetGroupName: redisSubnetGroup.ref,
      securityGroupIds: [redisSg.securityGroupId],
      transitEncryptionEnabled: true,
      authToken: redisAuthToken.secretValue.unsafeUnwrap(),
      atRestEncryptionEnabled: true,
    });
    redis.addDependency(redisSubnetGroup);

    // ──────────────────────────────────────────────────────────
    // ECS cluster + Fargate Spot service.
    // ──────────────────────────────────────────────────────────
    const cluster = new ecs.Cluster(this, "Cluster", {
      vpc,
      clusterName: "veloz",
      enableFargateCapacityProviders: true,
    });

    const logGroup = new logs.LogGroup(this, "AppLogs", {
      logGroupName: "/ecs/veloz",
      retention: logs.RetentionDays.ONE_WEEK,
      removalPolicy: cdk.RemovalPolicy.DESTROY,
    });

    const taskDef = new ecs.FargateTaskDefinition(this, "TaskDef", {
      cpu: 256,
      memoryLimitMiB: 512,
      runtimePlatform: {
        cpuArchitecture: ecs.CpuArchitecture.ARM64,
        operatingSystemFamily: ecs.OperatingSystemFamily.LINUX,
      },
    });

    dbSecret.grantRead(taskDef.taskRole);
    redisAuthToken.grantRead(taskDef.taskRole);
    etominSecret.grantRead(taskDef.taskRole);

    const container = taskDef.addContainer("app", {
      image: ecs.ContainerImage.fromEcrRepository(repo, "latest"),
      logging: ecs.LogDrivers.awsLogs({
        logGroup,
        streamPrefix: "veloz",
      }),
      environment: {
        APP_PORT: "81",
        DB_HOST: db.dbInstanceEndpointAddress,
        DB_PORT: db.dbInstanceEndpointPort,
        DB_NAME: "veloz_db",
        REDIS_HOST: redis.attrPrimaryEndPointAddress,
        REDIS_PORT: redis.attrPrimaryEndPointPort,
        REDIS_TLS: "true",
        RATE_LIMIT_ENABLED: "true",
        ETOMIN_BASE_URL: "https://pagos.etomin.com",
      },
      secrets: {
        DB_USER: ecs.Secret.fromSecretsManager(dbSecret, "username"),
        DB_PASSWORD: ecs.Secret.fromSecretsManager(dbSecret, "password"),
        REDIS_PASSWORD: ecs.Secret.fromSecretsManager(redisAuthToken),
        ETOMIN_EMAIL: ecs.Secret.fromSecretsManager(etominSecret, "email"),
        ETOMIN_PASSWORD: ecs.Secret.fromSecretsManager(etominSecret, "password"),
      },
      healthCheck: {
        command: [
          "CMD-SHELL",
          "wget -qO- http://127.0.0.1:81/health || exit 1",
        ],
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        retries: 3,
        startPeriod: cdk.Duration.seconds(30),
      },
    });

    container.addPortMappings({
      containerPort: 81,
      protocol: ecs.Protocol.TCP,
    });

    const service = new ecs.FargateService(this, "Service", {
      cluster,
      taskDefinition: taskDef,
      desiredCount: 1,
      assignPublicIp: true,
      vpcSubnets: { subnetType: ec2.SubnetType.PUBLIC },
      securityGroups: [taskSg],
      capacityProviderStrategies: [
        { capacityProvider: "FARGATE_SPOT", weight: 1 },
      ],
      circuitBreaker: { rollback: true },
      enableExecuteCommand: true,
      // 100/200: rolling deploy starts new task before stopping old to zero
      // downtime even at desiredCount=1.
      minHealthyPercent: 100,
      maxHealthyPercent: 200,
    });

    // Self-heal: ECS auto-restarts crashed tasks (built-in).
    // Spot eviction: ECS launches replacement task automatically.
    // Rolling deploy with circuit breaker rolls back on failure.

    // ──────────────────────────────────────────────────────────
    // Autoscaling: 1–3 tasks on CPU 70%.
    // ──────────────────────────────────────────────────────────
    const scaling = service.autoScaleTaskCount({
      minCapacity: 1,
      maxCapacity: 3,
    });
    scaling.scaleOnCpuUtilization("CpuScaling", {
      targetUtilizationPercent: 70,
      scaleInCooldown: cdk.Duration.seconds(120),
      scaleOutCooldown: cdk.Duration.seconds(60),
    });

    // ──────────────────────────────────────────────────────────
    // ALB (HTTP only, port 80 to task port 81).
    // ──────────────────────────────────────────────────────────
    const alb = new elbv2.ApplicationLoadBalancer(this, "Alb", {
      vpc,
      internetFacing: true,
      vpcSubnets: { subnetType: ec2.SubnetType.PUBLIC },
    });

    const listener = alb.addListener("HttpListener", {
      port: 80,
      protocol: elbv2.ApplicationProtocol.HTTP,
      open: true,
    });

    listener.addTargets("AppTargets", {
      port: 81,
      protocol: elbv2.ApplicationProtocol.HTTP,
      targets: [service],
      healthCheck: {
        path: "/health",
        interval: cdk.Duration.seconds(30),
        timeout: cdk.Duration.seconds(5),
        healthyThresholdCount: 2,
        unhealthyThresholdCount: 3,
        healthyHttpCodes: "200",
      },
      deregistrationDelay: cdk.Duration.seconds(15),
    });

    // ALB SG to task SG on port 81.
    taskSg.addIngressRule(
      ec2.Peer.securityGroupId(alb.connections.securityGroups[0].securityGroupId),
      ec2.Port.tcp(81),
      "ALB to task"
    );

    // ──────────────────────────────────────────────────────────
    // Outputs.
    // ──────────────────────────────────────────────────────────
    new cdk.CfnOutput(this, "AlbDns", { value: alb.loadBalancerDnsName });
    new cdk.CfnOutput(this, "RepoUri", { value: repo.repositoryUri });
    new cdk.CfnOutput(this, "ClusterName", { value: cluster.clusterName });
    new cdk.CfnOutput(this, "ServiceName", { value: service.serviceName });
    new cdk.CfnOutput(this, "DbEndpoint", { value: db.dbInstanceEndpointAddress });
    new cdk.CfnOutput(this, "RedisEndpoint", {
      value: redis.attrPrimaryEndPointAddress,
    });
  }
}
