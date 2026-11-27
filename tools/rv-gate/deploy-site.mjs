// Host reason (ARC-03): the release kit imports and the operator CLI executes this path.
import {runDeploymentCli} from "./deploy-site.mts";
export * from "./deploy-site.mts";
if (import.meta.main) await runDeploymentCli();
