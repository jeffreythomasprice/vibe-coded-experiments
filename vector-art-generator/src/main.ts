import { initGrid } from "./grid";
import { initControls } from "./controls";
import { getCurrentGeneration } from "./ipc";

async function init() {
  const organisms = await getCurrentGeneration();
  initGrid(organisms);
  initControls();
}

init();
