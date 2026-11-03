import { commands } from "@/bindings";
import { callCommand } from "./invoke";

export type LoaderType = "vanilla" | "fabric" | "forge" | "neoforge" | "quilt" | "optifine";

export async function getLoaderVersions(mcVersion: string, loader: LoaderType): Promise<string[]> {
  if (loader === "vanilla") return [];
  return callCommand(() => commands.getLoaderVersions(mcVersion, loader));
}

export async function installLoader(
  instanceId: string,
  loader: LoaderType,
  version: string | null,
): Promise<void> {
  await callCommand(() => commands.installLoader(instanceId, loader, version));
}
