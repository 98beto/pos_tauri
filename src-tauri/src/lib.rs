mod commands;
mod db;
mod error;
pub mod maintenance;
mod models;
mod process_lock;
mod services;

use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg(desktop)]
fn updater_is_configured(plugins: &tauri::utils::config::PluginConfig) -> bool {
    matches!(plugins.0.get("updater"), Some(serde_json::Value::Object(_)))
}

#[cfg(not(debug_assertions))]
const GENERIC_ERROR_MESSAGE: &str = "The application encountered an unexpected error.";

#[cfg(debug_assertions)]
fn run_error_message(
    error: &tauri::Error,
    _recovery: Option<&db::RecoveryStartupMessage>,
) -> String {
    format!("Tauri application failed: {error:#}")
}

#[cfg(not(debug_assertions))]
fn run_error_message<T>(_error: &T, recovery: Option<&db::RecoveryStartupMessage>) -> String {
    recovery
        .map(|message| message.as_str())
        .unwrap_or(GENERIC_ERROR_MESSAGE)
        .to_owned()
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

    let recovery_message = Arc::new(Mutex::new(None));
    let setup_recovery_message = Arc::clone(&recovery_message);
    let result = builder
        .setup(move |app| {
            #[cfg(desktop)]
            if updater_is_configured(&app.config().plugins) {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            let application_lock =
                db::acquire_application_lock(app).map_err(Box::<dyn std::error::Error>::from)?;
            let connection = db::open_database(app).map_err(|error| {
                if let Some(message) = error.recovery_startup_message() {
                    *setup_recovery_message.lock().unwrap() = Some(message);
                }
                Box::<dyn std::error::Error>::from(error)
            })?;
            #[cfg(debug_assertions)]
            services::auth::seed_debug_user(&connection)?;
            app.manage(application_lock);
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
            commands::cart::set_cart_item_quantity,
            commands::cart::set_cart_item_unit_price,
            commands::cart::reset_cart_item_unit_price,
            commands::cart::set_cart_discount,
            commands::cart::quote_cart_discount,
            commands::cart::remove_cart_discount,
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
            commands::products::find_product_by_code,
            commands::products::create_product_with_presentations,
            commands::products::update_product_with_presentations,
            commands::inventory::list_inventory_movements,
            commands::inventory::lookup_inventory_movement,
            commands::inventory::get_inventory_view,
            commands::inventory::get_product_inventory_detail,
            commands::inventory::quote_purchase,
            commands::inventory::create_purchase,
            commands::inventory::quote_stock_count,
            commands::inventory::create_stock_count,
            commands::inventory::search_inventory_products,
            commands::inventory::quote_purchase_price_correction,
            commands::inventory::confirm_purchase_price_correction,
            commands::sales::list_sales,
            commands::sales::get_sales_view,
            commands::sales::get_sale_history_detail,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        let recovery_message = recovery_message.lock().unwrap();
        eprintln!("{}", run_error_message(&error, recovery_message.as_ref()));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(desktop)]
    fn plugin_config(value: serde_json::Value) -> tauri::utils::config::PluginConfig {
        serde_json::from_value(value).expect("plugin config should deserialize")
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_not_configured_when_absent() {
        assert!(!updater_is_configured(&plugin_config(
            serde_json::json!({})
        )));
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_not_configured_when_null() {
        assert!(!updater_is_configured(&plugin_config(
            serde_json::json!({ "updater": null })
        )));
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_not_configured_when_string() {
        assert!(!updater_is_configured(&plugin_config(
            serde_json::json!({ "updater": "configured" })
        )));
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_not_configured_when_array() {
        assert!(!updater_is_configured(&plugin_config(
            serde_json::json!({ "updater": [] })
        )));
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_not_configured_when_boolean() {
        assert!(!updater_is_configured(&plugin_config(
            serde_json::json!({ "updater": true })
        )));
    }

    #[cfg(desktop)]
    #[test]
    fn updater_is_configured_when_object() {
        assert!(updater_is_configured(&plugin_config(serde_json::json!({
            "updater": {}
        }))));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_error_message_includes_error_details() {
        let error = tauri::Error::AssetNotFound("database initialization failed".into());

        assert!(run_error_message(&error, None).contains("database initialization failed"));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_error_message_does_not_require_error_formatting() {
        struct SensitiveError;

        assert_eq!(
            run_error_message(&SensitiveError, None),
            GENERIC_ERROR_MESSAGE
        );
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_error_message_only_exposes_typed_recovery_details() {
        let backup = std::path::PathBuf::from("/exact/recovery/backup.db");
        let recovery = db::DatabaseOpenError::Recovery {
            message: "Restaure el backup".into(),
            backup_path: backup.clone(),
        }
        .recovery_startup_message()
        .unwrap();

        assert_eq!(
            run_error_message(&"arbitrary secret", Some(&recovery)),
            format!("Restaure el backup: {}", backup.display())
        );
        assert_eq!(
            run_error_message(&"arbitrary secret", None),
            GENERIC_ERROR_MESSAGE
        );
    }
}
