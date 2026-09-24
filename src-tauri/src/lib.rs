mod commands;
mod db;
mod error;
mod models;
mod services;

use std::sync::Mutex;
use tauri::Manager;

#[cfg(not(debug_assertions))]
const GENERIC_ERROR_MESSAGE: &str = "The application encountered an unexpected error.";

#[cfg(debug_assertions)]
fn run_error_message(error: &tauri::Error) -> String {
    format!("Tauri application failed: {error:#}")
}

#[cfg(not(debug_assertions))]
fn run_error_message<T>(_error: &T) -> &'static str {
    GENERIC_ERROR_MESSAGE
}

#[cfg(all(not(debug_assertions), not(test)))]
fn install_release_panic_hook() {
    std::panic::set_hook(Box::new(|_| eprintln!("{GENERIC_ERROR_MESSAGE}")));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(all(not(debug_assertions), not(test)))]
    install_release_panic_hook();

    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_process::init());

    let result = builder
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            let connection = db::open_database(app)?;
            #[cfg(debug_assertions)]
            services::auth::seed_debug_user(&connection)?;
            app.manage(db::Database(Mutex::new(connection)));
            app.manage(commands::AuthSession(Mutex::new(
                commands::SessionState::default(),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth::owner_exists,
            commands::auth::initialize_owner,
            commands::auth::login,
            commands::auth::logout,
            commands::auth::is_authenticated,
            commands::auth::current_user,
            commands::cart::get_cart,
            commands::cart::add_cart_item,
            commands::cart::increment_cart_item,
            commands::cart::decrement_cart_item,
            commands::cart::remove_cart_item,
            commands::cart::clear_cart,
            commands::cart::quote_cash_change,
            commands::cart::checkout_cart,
            commands::brands::list_brands,
            commands::brands::create_brand,
            commands::brands::update_brand,
            commands::categories::list_categories,
            commands::categories::create_category,
            commands::categories::update_category,
            commands::products::list_products,
            commands::products::get_products_view,
            commands::products::list_product_selector,
            commands::products::find_product_by_sku,
            commands::products::create_product,
            commands::products::update_product,
            commands::inventory::list_inventory_movements,
            commands::inventory::get_inventory_view,
            commands::inventory::create_inventory_movement,
            commands::sales::list_sales,
            commands::sales::get_sales_view,
            commands::sales::get_sale_history_detail,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("{}", run_error_message(&error));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(debug_assertions)]
    #[test]
    fn debug_error_message_includes_error_details() {
        let error = tauri::Error::AssetNotFound("database initialization failed".into());

        assert!(run_error_message(&error).contains("database initialization failed"));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_error_message_does_not_require_error_formatting() {
        struct SensitiveError;

        assert_eq!(run_error_message(&SensitiveError), GENERIC_ERROR_MESSAGE);
    }
}
