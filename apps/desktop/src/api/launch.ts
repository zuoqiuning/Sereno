import { commands } from "@/bindings";
import { callCommand } from "./invoke";

export async function launchGame(instanceId: string): Promise<void> {
  await callCommand(() => commands.launchGame(instanceId));
}
