# Plan de migracion a rusqlite

Este documento describe como mover todo el acceso a SQLite y la logica de negocio al backend Rust de Tauri.

## Versiones

Versiones publicadas en crates.io y verificadas el 17 de septiembre de 2026:

```toml
rusqlite = { version = "0.40.2", features = ["bundled"] }
rusqlite_migration = "2.6.0"
argon2 = "0.6.0"
```

`rusqlite_migration 2.6.0` depende de `rusqlite 0.40`, por lo que son compatibles. Esta version de `rusqlite_migration` requiere Rust 1.95 como minimo; el equipo tiene Rust 1.98.1.

Para consultar las versiones disponibles en el futuro:

```bash
cargo search rusqlite --limit 1
cargo search rusqlite_migration --limit 1
cargo search argon2 --limit 1
```

Para consultar requisitos, caracteristicas y dependencias de una version:

```bash
cargo info rusqlite --verbose
cargo info rusqlite_migration --verbose
cargo info argon2 --verbose
```

No es obligatorio escribir manualmente las versiones. `cargo add` selecciona versiones compatibles y actualiza `Cargo.lock`:

```bash
cargo add rusqlite --features bundled
cargo add rusqlite_migration
cargo add argon2
```

Todos los comandos de Cargo de este documento se ejecutan dentro de `src-tauri`.

## Arquitectura final

```text
React
  -> invoke(...)
Comandos Tauri
  -> services en Rust
rusqlite
  -> pos.db
```

El frontend no ejecutara SQL ni recibira hashes de contrasenas.

Estructura sugerida:

```text
src/
  api/
  types/

src-tauri/src/
  commands/
    mod.rs
    auth.rs
    brands.rs
    categories.rs
    products.rs
    sales.rs
    inventory.rs
  db/
    mod.rs
    migrations.rs
  models/
    mod.rs
    user.rs
    brand.rs
    category.rs
    product.rs
    sale.rs
    inventory_movement.rs
  services/
    mod.rs
    auth.rs
    brands.rs
    categories.rs
    products.rs
    sales.rs
    inventory.rs
  lib.rs
```

## 1. Preparar la base existente

`tauri-plugin-sql` y `rusqlite_migration` no registran el historial de migraciones de la misma manera.

Si `pos.db` solo contiene datos de desarrollo, elimina esa base una vez despues de cambiar a `rusqlite`. La aplicacion la creara nuevamente.

Si `pos.db` ya contiene datos importantes, no la elimines. Antes de continuar se debe preparar una adopcion del esquema existente para evitar que `rusqlite_migration` intente crear tablas que ya existen.

## 2. Cambiar dependencias de Rust

Desde `src-tauri`:

```bash
cargo rm tauri-plugin-sql
cargo add rusqlite --features bundled
cargo add rusqlite_migration
cargo add argon2
```

La caracteristica `bundled` compila SQLite con la aplicacion. Esto evita depender de la version de SQLite instalada en la computadora del negocio.

Despues, `Cargo.toml` debe incluir:

```toml
rusqlite = { version = "0.40.2", features = ["bundled"] }
rusqlite_migration = "2.6.0"
argon2 = "0.6.0"
```

## 3. Retirar el acceso SQL del frontend

Desde la raiz del proyecto:

```bash
pnpm remove @tauri-apps/plugin-sql
```

Eliminar:

```text
src/db/database.ts
```

Quitar `"sql:default"` de `src-tauri/capabilities/default.json`.

Quitar de `src-tauri/src/lib.rs`:

```rust
use tauri_plugin_sql::{Migration, MigrationKind};
```

Tambien quitar el bloque `.plugin(tauri_plugin_sql::Builder...)` y la lista actual de migraciones del plugin. Las migraciones se moveran a `db/migrations.rs`.

## 4. Registrar migraciones con rusqlite_migration

Crear `src-tauri/src/db/migrations.rs`:

```rust
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

const MIGRATIONS: Migrations<'static> = Migrations::from_slice(&[
    M::up(include_str!("../../migrations/001_create_users.sql")),
    M::up(include_str!("../../migrations/002_create_brands.sql")),
    M::up(include_str!("../../migrations/003_create_categories.sql")),
    M::up(include_str!("../../migrations/004_create_products.sql")),
    M::up(include_str!("../../migrations/005_create_sales.sql")),
    M::up(include_str!("../../migrations/006_create_sale_details.sql")),
    M::up(include_str!("../../migrations/007_create_inventory_movements.sql")),
    M::up(include_str!("../../migrations/008_create_indexes.sql")),
]);

pub fn run(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_latest(connection)
}
```

Cada posicion del arreglo representa una version. No se deben reordenar, eliminar ni editar migraciones que ya se hayan aplicado. Los cambios siguientes deben agregarse como `009_...sql`, `010_...sql`, etc.

## 5. Crear y abrir la base de datos

Crear `src-tauri/src/db/mod.rs`. Este modulo debe:

1. Obtener `app_data_dir` mediante Tauri.
2. Crear el directorio si no existe.
3. Abrir `pos.db` con `rusqlite::Connection::open`.
4. Activar las claves foraneas en la conexion.
5. Configurar WAL.
6. Ejecutar las migraciones.
7. Entregar la conexion como estado administrado de Tauri.

Configuraciones necesarias para cada conexion:

```rust
connection.pragma_update(None, "foreign_keys", "ON")?;
connection.pragma_update(None, "journal_mode", "WAL")?;
```

El estado puede comenzar de forma sencilla:

```rust
pub struct Database(pub std::sync::Mutex<rusqlite::Connection>);
```

Una conexion con `Mutex` es suficiente para este POS local de un solo usuario. No se necesita un pool inicialmente.

## 6. Inicializar la base en Tauri

En `lib.rs`, usar `.setup(...)` para abrir la base y ejecutar las migraciones antes de que la interfaz empiece a utilizarla. Despues registrar el estado con:

```rust
app.manage(Database(std::sync::Mutex::new(connection)));
```

Los comandos acceden a la conexion mediante:

```rust
tauri::State<'_, Database>
```

## 7. Crear modelos Rust

Los modelos de salida deben derivar `Serialize` para regresar datos a React:

```rust
#[derive(serde::Serialize)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: i64,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub created_at: String,
    pub updated_at: String,
}
```

Las entradas deben ser estructuras distintas y derivar `Deserialize`:

```rust
#[derive(serde::Deserialize)]
pub struct CreateProductInput {
    pub name: String,
    pub price: i64,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
}
```

No deben recibir `id`, `created_at` ni `updated_at`, porque SQLite genera esos valores.

## 8. Crear services en orden

Implementar primero operaciones pequenas para comprobar la infraestructura:

1. Marcas: listar, crear y actualizar.
2. Categorias: listar, crear y actualizar.
3. Productos: listar, crear, actualizar y buscar por SKU.
4. Inventario: registrar compra o ajuste y modificar stock en una transaccion.
5. Ventas: crear una venta completa en una transaccion.
6. Autenticacion: verificar la contrasena del propietario con Argon2.

Los services contienen SQL y reglas de negocio. Los comandos Tauri solo reciben datos, llaman al service y transforman errores para el frontend.

## 9. Crear comandos Tauri

Ejemplo conceptual:

```rust
#[tauri::command]
pub fn list_products(
    database: tauri::State<'_, Database>,
) -> Result<Vec<Product>, String> {
    let connection = database.0.lock().map_err(|error| error.to_string())?;
    products::list(&connection).map_err(|error| error.to_string())
}
```

Registrar cada comando en `lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    commands::products::list_products,
    commands::products::create_product,
])
```

No conviene devolver errores internos completos en la version final. Inicialmente se pueden convertir a `String`; despues debe crearse un tipo de error controlado.

## 10. Implementar inventario con transacciones

Una compra o ajuste debe cambiar `products.stock` y crear `inventory_movements` dentro de la misma transaccion.

Flujo:

```text
BEGIN
  validar producto y cantidad
  modificar products.stock
  insertar inventory_movements
COMMIT
```

Si una consulta falla antes de `commit()`, `rusqlite` revierte la transaccion cuando esta se descarta.

## 11. Implementar ventas con transacciones

El service de ventas debe:

1. Comenzar una transaccion.
2. Validar que el carrito no este vacio.
3. Consultar cada producto y obtener su precio desde SQLite.
4. Verificar stock suficiente.
5. Calcular el total en Rust.
6. Insertar `sales`.
7. Insertar cada `sale_details`.
8. Restar stock con una condicion que evite valores negativos.
9. Insertar cada movimiento como `reason = 'venta'` y `type = 'salida'`.
10. Confirmar con `transaction.commit()`.

La actualizacion segura del stock debe incluir una condicion:

```sql
UPDATE products
SET stock = stock - ?1,
    updated_at = CURRENT_TIMESTAMP
WHERE id = ?2 AND stock >= ?1;
```

Si la consulta actualiza cero filas, el producto no existe o no tiene stock suficiente y la venta debe fallar completa.

## 12. Implementar autenticacion

La tabla guarda `password_hash`, nunca la contrasena original.

Flujo del login:

1. React envia `email` y `password` al comando `login`.
2. Rust consulta el usuario y su `password_hash`.
3. Argon2 verifica la contrasena recibida contra el hash.
4. Rust devuelve un usuario sin `password_hash`.
5. React descarta la contrasena del formulario.

Como no existira registro publico, se debe definir un mecanismo de inicializacion del propietario. Puede ser una configuracion inicial disponible solo cuando la tabla `users` esta vacia o un usuario preparado antes de entregar la aplicacion.

## 13. Crear una capa API pequena en React

Los archivos de `src/api/` no contienen SQL ni reglas de negocio. Solo encapsulan `invoke`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { Product } from "../types/product";

export function listProducts(): Promise<Product[]> {
  return invoke<Product[]>("list_products");
}
```

Esto evita repetir nombres de comandos y tipos en los componentes.

## 14. Verificacion por etapas

Despues de cada cambio de Rust:

```bash
cargo check
```

Despues de modificar TypeScript:

```bash
pnpm build
```

Para probar la aplicacion completa:

```bash
pnpm tauri dev
```

Pruebas minimas:

- Crear una base vacia y aplicar las ocho migraciones.
- Confirmar `PRAGMA foreign_keys` con resultado `1`.
- Rechazar IDs de marca, categoria, producto o venta inexistentes.
- Crear productos y encontrarlos por SKU.
- Rechazar cantidades y precios invalidos.
- Registrar entradas y salidas de inventario.
- Rechazar ventas sin stock suficiente.
- Confirmar que una venta fallida no modifica stock ni deja detalles parciales.
- Confirmar que una venta correcta crea venta, detalles y movimientos.
- Verificar login correcto e incorrecto.

## Primer bloque recomendado

No implementar todo a la vez. El primer bloque debe incluir solamente:

1. Cambiar dependencias.
2. Retirar `tauri-plugin-sql`.
3. Crear `db/mod.rs` y `db/migrations.rs`.
4. Inicializar `pos.db` desde `lib.rs`.
5. Ejecutar `cargo check`.
6. Abrir la aplicacion con una base vacia y confirmar que se crean las tablas.
7. Implementar `list_products` como primer comando de prueba.

Cuando este bloque funcione, continuar con CRUD, inventario, ventas y autenticacion en ese orden.

## Estado de implementacion

Completado el 17 de septiembre de 2026:

- `tauri-plugin-sql` fue retirado del codigo y de las dependencias activas.
- La conexion usa `rusqlite`, `foreign_keys = ON`, WAL y un timeout de cinco segundos.
- Las ocho migraciones se ejecutan con `rusqlite_migration`.
- Existen modelos, services y comandos para marcas, categorias y productos.
- El stock inicial de un producto crea un movimiento de ajuste.
- Las modificaciones posteriores de stock pasan por el service de inventario.
- Inventario y ventas utilizan transacciones.
- La autenticacion usa Argon2 y una sesion en memoria administrada por Rust.
- Los comandos de negocio requieren una sesion autenticada.
- React dispone de wrappers en `src/api/` y no ejecuta SQL.
- Las migraciones y los flujos principales tienen pruebas sobre SQLite en memoria.

Pendiente para la interfaz:

- Crear las pantallas de configuracion inicial, login, catalogos, productos, inventario y ventas.
- Conectar esas pantallas a las funciones de `src/api/`.
- Decidir el flujo de anulacion o devolucion de ventas antes de implementarlo.
