use rusqlite::{params, Connection, Row};

use crate::models::category::{Category, SaveCategoryInput};

use super::{normalize_persisted, search_key, validate_id, ServiceError, ServiceResult};

const NAME_MAX_LENGTH: usize = 100;

fn map_category(row: &Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn validate_name(name: &str) -> ServiceResult<String> {
    let name = normalize_persisted(name);
    if name.is_empty() {
        return Err(ServiceError::validation(
            "CATEGORY_NAME_REQUIRED",
            "El nombre de la categoria es obligatorio",
        ));
    }
    if name.chars().count() > NAME_MAX_LENGTH {
        return Err(ServiceError::validation(
            "CATEGORY_NAME_TOO_LONG",
            "El nombre de la categoria debe tener como maximo 100 caracteres",
        ));
    }
    Ok(name)
}

fn name_exists(
    connection: &Connection,
    name: &str,
    excluded_id: Option<i64>,
) -> ServiceResult<bool> {
    let key = search_key(name);
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM categories
             WHERE name_search_key = ?1 AND (?2 IS NULL OR id <> ?2))",
            params![key, excluded_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Category>> {
    let mut statement = connection.prepare(
        "SELECT id, name, created_at, updated_at FROM categories ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], map_category)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn create(connection: &Connection, input: SaveCategoryInput) -> ServiceResult<Category> {
    let name = validate_name(&input.name)?;
    if name_exists(connection, &name, None)? {
        return Err(ServiceError::conflict(
            "CATEGORY_ALREADY_EXISTS",
            "La categoria ya existe",
        ));
    }

    let key = search_key(&name);
    connection.execute(
        "INSERT INTO categories (name, name_search_key) VALUES (?1, ?2)",
        params![name, key],
    )?;
    get(connection, connection.last_insert_rowid())
}

pub fn update(
    connection: &Connection,
    id: i64,
    input: SaveCategoryInput,
) -> ServiceResult<Category> {
    validate_id(id)?;
    let name = validate_name(&input.name)?;
    if name_exists(connection, &name, Some(id))? {
        return Err(ServiceError::conflict(
            "CATEGORY_ALREADY_EXISTS",
            "La categoria ya existe",
        ));
    }

    let changed = connection.execute(
        "UPDATE categories SET name = ?1, name_search_key = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
        params![name, search_key(&name), id],
    )?;
    if changed == 0 {
        return Err(ServiceError::not_found(
            "CATEGORY_NOT_FOUND",
            "Categoria no encontrada",
        ));
    }
    get(connection, id)
}

fn get(connection: &Connection, id: i64) -> ServiceResult<Category> {
    connection
        .query_row(
            "SELECT id, name, created_at, updated_at FROM categories WHERE id = ?1",
            [id],
            map_category,
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ServiceError::not_found("CATEGORY_NOT_FOUND", "Categoria no encontrada")
            }
            other => other.into(),
        })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::{db::migrations, models::category::SaveCategoryInput};

    #[test]
    fn rejects_names_over_the_catalog_limit() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let error = super::create(
            &connection,
            SaveCategoryInput {
                name: "a".repeat(101),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "CATEGORY_NAME_TOO_LONG");
    }

    #[test]
    fn normalizes_names_and_rejects_unicode_equivalent_duplicates() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let category = super::create(
            &connection,
            SaveCategoryInput {
                name: "Cafe\u{301}".into(),
            },
        )
        .unwrap();
        let duplicate = super::create(
            &connection,
            SaveCategoryInput {
                name: "CAFÉ".into(),
            },
        )
        .unwrap_err();

        assert_eq!(category.name, "Café");
        assert_eq!(duplicate.code, "CATEGORY_ALREADY_EXISTS");
    }
}
