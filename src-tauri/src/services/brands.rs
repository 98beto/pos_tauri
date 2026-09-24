use rusqlite::{params, Connection, Row};

use crate::models::brand::{Brand, SaveBrandInput};

use super::{normalize_persisted, search_key, validate_id, ServiceError, ServiceResult};

const NAME_MAX_LENGTH: usize = 100;

fn map_brand(row: &Row<'_>) -> rusqlite::Result<Brand> {
    Ok(Brand {
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
            "BRAND_NAME_REQUIRED",
            "El nombre de la marca es obligatorio",
        ));
    }
    if name.chars().count() > NAME_MAX_LENGTH {
        return Err(ServiceError::validation(
            "BRAND_NAME_TOO_LONG",
            "El nombre de la marca debe tener como maximo 100 caracteres",
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
            "SELECT EXISTS(SELECT 1 FROM brands
             WHERE name_search_key = ?1 AND (?2 IS NULL OR id <> ?2))",
            params![key, excluded_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Brand>> {
    let mut statement = connection.prepare(
        "SELECT id, name, created_at, updated_at FROM brands ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], map_brand)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn create(connection: &Connection, input: SaveBrandInput) -> ServiceResult<Brand> {
    let name = validate_name(&input.name)?;
    if name_exists(connection, &name, None)? {
        return Err(ServiceError::conflict(
            "BRAND_ALREADY_EXISTS",
            "La marca ya existe",
        ));
    }

    let key = search_key(&name);
    connection.execute(
        "INSERT INTO brands (name, name_search_key) VALUES (?1, ?2)",
        params![name, key],
    )?;
    get(connection, connection.last_insert_rowid())
}

pub fn update(connection: &Connection, id: i64, input: SaveBrandInput) -> ServiceResult<Brand> {
    validate_id(id)?;
    let name = validate_name(&input.name)?;
    if name_exists(connection, &name, Some(id))? {
        return Err(ServiceError::conflict(
            "BRAND_ALREADY_EXISTS",
            "La marca ya existe",
        ));
    }

    let changed = connection.execute(
        "UPDATE brands SET name = ?1, name_search_key = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
        params![name, search_key(&name), id],
    )?;
    if changed == 0 {
        return Err(ServiceError::not_found(
            "BRAND_NOT_FOUND",
            "Marca no encontrada",
        ));
    }
    get(connection, id)
}

fn get(connection: &Connection, id: i64) -> ServiceResult<Brand> {
    connection
        .query_row(
            "SELECT id, name, created_at, updated_at FROM brands WHERE id = ?1",
            [id],
            map_brand,
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ServiceError::not_found("BRAND_NOT_FOUND", "Marca no encontrada")
            }
            other => other.into(),
        })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::{db::migrations, models::brand::SaveBrandInput};

    #[test]
    fn rejects_names_over_the_catalog_limit() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let error = super::create(
            &connection,
            SaveBrandInput {
                name: "a".repeat(101),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "BRAND_NAME_TOO_LONG");
    }

    #[test]
    fn normalizes_names_and_rejects_unicode_equivalent_duplicates() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let brand = super::create(
            &connection,
            SaveBrandInput {
                name: "Cafe\u{301}".into(),
            },
        )
        .unwrap();
        let duplicate = super::create(
            &connection,
            SaveBrandInput {
                name: "CAFÉ".into(),
            },
        )
        .unwrap_err();

        assert_eq!(brand.name, "Café");
        assert_eq!(duplicate.code, "BRAND_ALREADY_EXISTS");
    }

    #[test]
    fn rejects_duplicates_using_full_unicode_case_folding() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();

        let brand = super::create(
            &connection,
            SaveBrandInput {
                name: "Straße".into(),
            },
        )
        .unwrap();
        let duplicate = super::create(
            &connection,
            SaveBrandInput {
                name: "STRASSE".into(),
            },
        )
        .unwrap_err();

        assert_eq!(brand.name, "Straße");
        assert_eq!(duplicate.code, "BRAND_ALREADY_EXISTS");
    }
}
