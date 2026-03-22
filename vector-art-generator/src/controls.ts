import { breedNextGeneration } from "./ipc";
import { getSelectedIds, renderGrid } from "./grid";

let currentGeneration = 0;

export function setGeneration(gen: number) {
  currentGeneration = gen;
  document.getElementById("gen-counter")!.textContent = `Gen: ${gen}`;
}

export function initControls() {
  const breedBtn = document.getElementById("btn-breed")!;
  breedBtn.addEventListener("click", breed);

  document.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      breed();
    }
  });
}

async function breed() {
  const ids = getSelectedIds();
  if (ids.length === 0) return;

  const organisms = await breedNextGeneration(ids);
  currentGeneration++;
  setGeneration(currentGeneration);
  renderGrid(organisms);
}
