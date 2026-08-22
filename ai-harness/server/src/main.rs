#[tauri::command]
fn greet(request: shared::GreetRequest) -> shared::GreetResponse {
    shared::GreetResponse {
        message: lib::build_greeting(&request.name),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
