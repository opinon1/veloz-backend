import * as cdk from "aws-cdk-lib";
import { Construct } from "constructs";
import * as ecr from "aws-cdk-lib/aws-ecr";

/**
 * Standalone ECR stack.
 *
 * Lives in its own stack so the image registry can be created and populated
 * with a first image *before* the app stack tries to start an ECS service.
 * Without this split, ECS attempts to pull `veloz:latest` during initial
 * stack creation, the pull fails, the deployment circuit breaker fires, and
 * the rollback gets stuck on the cluster.
 */
export class VelozEcrStack extends cdk.Stack {
  public readonly repositoryName = "veloz";

  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const repo = new ecr.Repository(this, "Repo", {
      repositoryName: this.repositoryName,
      imageScanOnPush: true,
      lifecycleRules: [
        { maxImageCount: 10, description: "Keep last 10 images" },
      ],
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    new cdk.CfnOutput(this, "RepoUri", { value: repo.repositoryUri });
    new cdk.CfnOutput(this, "RepoArn", { value: repo.repositoryArn });
  }
}
