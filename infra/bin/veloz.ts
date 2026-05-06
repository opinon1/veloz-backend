#!/usr/bin/env node
import * as cdk from "aws-cdk-lib";
import { VelozStack } from "../lib/veloz-stack";
import { GithubOidcStack } from "../lib/github-oidc-stack";

const app = new cdk.App();

const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION ?? "us-west-1",
};

const githubOwner = app.node.tryGetContext("githubOwner") ?? "opinon1";
const githubRepo = app.node.tryGetContext("githubRepo") ?? "veloz-backend";

new GithubOidcStack(app, "VelozGithubOidc", {
  env,
  githubOwner,
  githubRepo,
});

new VelozStack(app, "VelozStack", {
  env,
});
