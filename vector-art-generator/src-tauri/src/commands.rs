use crate::generated::RenderedOrganism;
use crate::ga::population::Population;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub population: Mutex<Population>,
}

#[tauri::command]
pub fn get_current_generation(state: State<AppState>) -> Result<Vec<RenderedOrganism>, String> {
    let pop = state.population.lock().map_err(|e| e.to_string())?;
    Ok(pop.render_all())
}

#[tauri::command]
pub fn breed_next_generation(
    selected_ids: Vec<String>,
    state: State<AppState>,
) -> Result<Vec<RenderedOrganism>, String> {
    let mut pop = state.population.lock().map_err(|e| e.to_string())?;
    pop.breed(&selected_ids)?;
    Ok(pop.render_all())
}
