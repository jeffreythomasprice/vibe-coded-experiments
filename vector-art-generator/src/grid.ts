import type { RenderedOrganism } from "./generated/types";

const selectedIds = new Set<string>();

export function getSelectedIds(): string[] {
  return [...selectedIds];
}

export function getSelectionCount(): number {
  return selectedIds.size;
}

export function initGrid(organisms: RenderedOrganism[]) {
  renderGrid(organisms);
}

export function renderGrid(organisms: RenderedOrganism[]) {
  const grid = document.getElementById("grid")!;
  grid.innerHTML = "";
  selectedIds.clear();

  for (const org of organisms) {
    const cell = document.createElement("div");
    cell.className = "cell";
    cell.dataset.id = org.id;
    cell.innerHTML = org.svg;

    cell.addEventListener("click", () => {
      if (selectedIds.has(org.id)) {
        selectedIds.delete(org.id);
        cell.classList.remove("selected");
      } else {
        selectedIds.add(org.id);
        cell.classList.add("selected");
      }
      updateSelectionCount();
    });

    grid.appendChild(cell);
  }

  updateSelectionCount();
}

function updateSelectionCount() {
  const counter = document.getElementById("sel-counter")!;
  counter.textContent = `Selected: ${selectedIds.size}`;

  const breedBtn = document.getElementById(
    "btn-breed"
  ) as HTMLButtonElement;
  breedBtn.disabled = selectedIds.size === 0;
}
