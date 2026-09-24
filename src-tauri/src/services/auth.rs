use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

use crate::models::user::{InitializeOwnerInput, LoginInput, User};

use super::{normalize_persisted, ServiceError, ServiceResult};

const NAME_MAX_LENGTH: usize = 100;
const EMAIL_MAX_LENGTH: usize = 254;
const PASSWORD_MAX_LENGTH: usize = 128;

fn map_user(row: &Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        first_name: row.get(1)?,
        last_name: row.get(2)?,
        email: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn owner_exists(connection: &Connection) -> ServiceResult<bool> {
    connection
        .query_row("SELECT EXISTS(SELECT 1 FROM users)", [], |row| row.get(0))
        .map_err(Into::into)
}

pub fn initialize_owner(
    connection: &mut Connection,
    input: InitializeOwnerInput,
) -> ServiceResult<User> {
    if owner_exists(connection)? {
        return Err(ServiceError::conflict(
            "OWNER_ALREADY_CONFIGURED",
            "El propietario ya fue configurado",
        ));
    }

    let first_name = normalize_persisted(&input.first_name);
    let last_name = normalize_persisted(&input.last_name);
    let email = normalize_email(&input.email)?;
    if first_name.is_empty() || last_name.is_empty() || email.is_empty() {
        return Err(ServiceError::validation(
            "REQUIRED_OWNER_FIELDS",
            "Nombre, apellido y correo son obligatorios",
        ));
    }
    if first_name.chars().count() > NAME_MAX_LENGTH || last_name.chars().count() > NAME_MAX_LENGTH {
        return Err(ServiceError::validation(
            "OWNER_NAME_TOO_LONG",
            "El nombre y el apellido deben tener como maximo 100 caracteres",
        ));
    }
    if input.password.len() < 8 {
        return Err(ServiceError::validation(
            "PASSWORD_TOO_SHORT",
            "La contrasena debe tener al menos 8 caracteres",
        ));
    }
    if input.password.len() > PASSWORD_MAX_LENGTH {
        return Err(ServiceError::validation(
            "PASSWORD_TOO_LONG",
            "La contrasena debe tener como maximo 128 bytes",
        ));
    }
    if input.password != input.confirm_password {
        return Err(ServiceError::validation_field(
            "PASSWORD_CONFIRMATION_MISMATCH",
            "La confirmacion no coincide con la contrasena",
            "confirm_password",
        ));
    }

    let password_hash = hash_password(&input.password)?;

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if owner_exists(&transaction)? {
        return Err(ServiceError::conflict(
            "OWNER_ALREADY_CONFIGURED",
            "El propietario ya fue configurado",
        ));
    }

    transaction.execute(
        "INSERT INTO users (first_name, last_name, email, password_hash)
         VALUES (?1, ?2, ?3, ?4)",
        params![first_name, last_name, email, password_hash],
    )?;
    let owner = get_user(&transaction, transaction.last_insert_rowid())?;
    transaction.commit()?;
    Ok(owner)
}

pub fn login(connection: &Connection, input: LoginInput) -> ServiceResult<User> {
    let email = normalize_email(&input.email).map_err(|_| invalid_credentials())?;
    if input.password.len() > PASSWORD_MAX_LENGTH {
        return Err(invalid_credentials());
    }
    if !owner_exists(connection)? {
        return Err(ServiceError::not_found(
            "OWNER_NOT_CONFIGURED",
            "La base de datos no tiene un propietario configurado",
        ));
    }

    let credentials = connection
        .query_row(
            "SELECT id, password_hash FROM users WHERE email = ?1",
            [email],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    let Some((user_id, password_hash)) = credentials else {
        return Err(invalid_credentials());
    };
    let parsed_hash = PasswordHash::new(&password_hash)
        .map_err(|_| ServiceError::internal("PASSWORD_HASH_ERROR", "No se pudo iniciar sesion"))?;
    Argon2::default()
        .verify_password(input.password.as_bytes(), &parsed_hash)
        .map_err(|_| invalid_credentials())?;

    get_user(connection, user_id)
}

fn normalize_email(value: &str) -> ServiceResult<String> {
    let email = normalize_persisted(&normalize_persisted(value).to_lowercase());
    let length = email.chars().count();
    if length > EMAIL_MAX_LENGTH {
        return Err(ServiceError::validation_field(
            "EMAIL_TOO_LONG",
            "El correo debe tener como maximo 254 caracteres",
            "email",
        ));
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if length < 3
        || local.is_empty()
        || domain.is_empty()
        || parts.next().is_some()
        || email
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(ServiceError::validation_field(
            "INVALID_EMAIL",
            "El correo no tiene un formato valido",
            "email",
        ));
    }
    Ok(email)
}

fn hash_password(password: &str) -> ServiceResult<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| {
            ServiceError::internal("PASSWORD_HASH_ERROR", "No se pudo proteger la contrasena")
        })
}

fn invalid_credentials() -> ServiceError {
    ServiceError::authentication("INVALID_CREDENTIALS", "Correo o contrasena incorrectos")
}

#[cfg(any(debug_assertions, test))]
pub fn seed_debug_user(connection: &Connection) -> ServiceResult<()> {
    if owner_exists(connection)? {
        return Ok(());
    }

    let password_hash = hash_password("PosDemo123!")?;
    connection.execute(
        "INSERT INTO users (first_name, last_name, email, password_hash)
         VALUES ('Admin', 'POS', 'admin@pos.local', ?1)",
        [password_hash],
    )?;
    eprintln!("Development user: admin@pos.local / PosDemo123!");
    Ok(())
}

fn get_user(connection: &Connection, id: i64) -> ServiceResult<User> {
    connection
        .query_row(
            "SELECT id, first_name, last_name, email, created_at, updated_at
             FROM users WHERE id = ?1",
            [id],
            map_user,
        )
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use std::{
        fs,
        sync::{Arc, Barrier},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crate::{
        db::migrations,
        models::user::{InitializeOwnerInput, LoginInput},
    };

    #[test]
    fn initializes_and_authenticates_owner_without_exposing_hash() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let owner = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Albert".into(),
                last_name: "Owner".into(),
                email: "OWNER@EXAMPLE.COM".into(),
                password: "password-seguro".into(),
                confirm_password: "password-seguro".into(),
            },
        )
        .unwrap();
        let logged_in = super::login(
            &connection,
            LoginInput {
                email: "owner@example.com".into(),
                password: "password-seguro".into(),
            },
        )
        .unwrap();
        let stored_hash: String = connection
            .query_row("SELECT password_hash FROM users", [], |row| row.get(0))
            .unwrap();

        assert_eq!(owner.id, logged_in.id);
        assert_ne!(stored_hash, "password-seguro");
        assert!(stored_hash.starts_with("$argon2"));
    }

    #[test]
    fn seeds_debug_user_only_when_users_is_empty() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        super::seed_debug_user(&connection).unwrap();
        super::seed_debug_user(&connection).unwrap();

        let user_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        let user = super::login(
            &connection,
            LoginInput {
                email: "admin@pos.local".into(),
                password: "PosDemo123!".into(),
            },
        )
        .unwrap();

        assert_eq!(user_count, 1);
        assert_eq!(user.email, "admin@pos.local");
    }

    #[test]
    fn rejects_oversized_owner_fields_before_hashing() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let name_error = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "a".repeat(101),
                last_name: "Owner".into(),
                email: "owner@example.com".into(),
                password: "password-seguro".into(),
                confirm_password: "password-seguro".into(),
            },
        )
        .unwrap_err();
        let password_error = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Albert".into(),
                last_name: "Owner".into(),
                email: "owner@example.com".into(),
                password: "a".repeat(129),
                confirm_password: "a".repeat(129),
            },
        )
        .unwrap_err();
        let email_error = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Albert".into(),
                last_name: "Owner".into(),
                email: format!("{}@example.com", "a".repeat(243)),
                password: "password-seguro".into(),
                confirm_password: "password-seguro".into(),
            },
        )
        .unwrap_err();

        assert_eq!(name_error.code, "OWNER_NAME_TOO_LONG");
        assert_eq!(password_error.code, "PASSWORD_TOO_LONG");
        assert_eq!(email_error.code, "EMAIL_TOO_LONG");
    }

    #[test]
    fn rejects_mismatched_password_confirmation_with_safe_field() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let error = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Albert".into(),
                last_name: "Owner".into(),
                email: "owner@example.com".into(),
                password: "password-seguro".into(),
                confirm_password: "password-distinto".into(),
            },
        )
        .unwrap_err();
        let serialized = serde_json::to_value(&error).unwrap();

        assert_eq!(error.code, "PASSWORD_CONFIRMATION_MISMATCH");
        assert_eq!(serialized["category"], "validation");
        assert_eq!(serialized["field"], "confirm_password");
        assert!(!super::owner_exists(&connection).unwrap());
    }

    #[test]
    fn validates_email_limit_after_lowercase_expansion() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let error = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Albert".into(),
                last_name: "Owner".into(),
                email: format!("{}İ", "a".repeat(253)),
                password: "password-seguro".into(),
                confirm_password: "password-seguro".into(),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "EMAIL_TOO_LONG");
    }

    #[test]
    fn validates_and_normalizes_owner_email_without_requiring_a_public_domain() {
        for invalid in ["a", "@local", "a@", "a@@local", "a b@local", "a\n@local"] {
            let error = super::normalize_email(invalid).unwrap_err();
            assert_eq!(error.code, "INVALID_EMAIL", "accepted {invalid:?}");
            assert_eq!(error.field, Some("email"));
        }

        assert_eq!(
            super::normalize_email("  OWNER@LOCAL  ").unwrap(),
            "owner@local"
        );
        assert_eq!(
            super::normalize_email("USUARIO@CAFE\u{301}").unwrap(),
            "usuario@café"
        );
    }

    #[test]
    fn normalizes_persisted_owner_fields_before_lowercasing_email() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let owner = super::initialize_owner(
            &mut connection,
            InitializeOwnerInput {
                first_name: "Jose\u{301}".into(),
                last_name: "Mun\u{303}oz".into(),
                email: "USUARIO@CAFE\u{301}.TEST".into(),
                password: "password-seguro".into(),
                confirm_password: "password-seguro".into(),
            },
        )
        .unwrap();
        let logged_in = super::login(
            &connection,
            LoginInput {
                email: "usuario@café.test".into(),
                password: "password-seguro".into(),
            },
        )
        .unwrap();

        assert_eq!(owner.first_name, "José");
        assert_eq!(owner.last_name, "Muñoz");
        assert_eq!(owner.email, "usuario@café.test");
        assert_eq!(logged_in.id, owner.id);
    }

    #[test]
    fn initializes_owner_atomically_across_file_connections() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pos-tauri-owner-race-{}-{unique}.db",
            std::process::id()
        ));
        let mut first_connection = Connection::open(&path).unwrap();
        first_connection
            .busy_timeout(Duration::from_secs(10))
            .unwrap();
        migrations::run(&mut first_connection).unwrap();
        let mut second_connection = Connection::open(&path).unwrap();
        second_connection
            .busy_timeout(Duration::from_secs(10))
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            first_barrier.wait();
            super::initialize_owner(
                &mut first_connection,
                InitializeOwnerInput {
                    first_name: "Primer".into(),
                    last_name: "Owner".into(),
                    email: "first@example.com".into(),
                    password: "password-seguro".into(),
                    confirm_password: "password-seguro".into(),
                },
            )
        });
        let second = thread::spawn(move || {
            barrier.wait();
            super::initialize_owner(
                &mut second_connection,
                InitializeOwnerInput {
                    first_name: "Segundo".into(),
                    last_name: "Owner".into(),
                    email: "second@example.com".into(),
                    password: "password-seguro".into(),
                    confirm_password: "password-seguro".into(),
                },
            )
        });

        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let conflict = results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap();
        assert_eq!(conflict.code, "OWNER_ALREADY_CONFIGURED");

        let verification = Connection::open(&path).unwrap();
        let user_count: i64 = verification
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_count, 1);
        drop(verification);
        fs::remove_file(path).unwrap();
    }
}
