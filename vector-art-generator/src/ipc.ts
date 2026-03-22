import { invoke } from "@tauri-apps/api/core";
import type { RenderedOrganism } from "./generated/types";

export async function getCurrentGeneration(): Promise<RenderedOrganism[]> {
  return invoke("get_current_generation");
}

export async function breedNextGeneration(
  selectedIds: string[]
): Promise<RenderedOrganism[]> {
  return invoke("breed_next_generation", { selectedIds });
}
