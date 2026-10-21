# Linea POS

Aplicación de punto de venta de escritorio para registrar ventas, administrar productos y controlar el inventario de un negocio desde una interfaz rápida y sencilla.

Linea POS guarda la información en una base de datos SQLite local y está disponible para Linux y Windows. La aplicación está construida con Tauri 2, React, TypeScript y Rust.

## Características

- Registro de ventas con pagos en efectivo, tarjeta o transferencia.
- Búsqueda de productos por nombre, SKU o código de barras.
- Carrito con cantidades, precios personalizados y descuentos porcentuales o fijos.
- Cálculo del efectivo recibido y el cambio a entregar.
- Catálogo de productos organizado por marcas y categorías.
- Presentaciones por pieza, paquete, peso, volumen o longitud, cada una con su propio precio y código de barras.
- Control de existencias físicas mediante compras y conteos, con soporte para piezas, peso, volumen y longitud.
- Precio e importe propios en cada compra, con corrección local que no modifica ventas, stock ni movimientos posteriores.
- Consulta de movimientos y detalle histórico por producto.
- Historial de tickets con filtros, totales y detalle de cada venta.
- Configuración inicial y autenticación del propietario.
- Actualizaciones firmadas de la aplicación.

## Capturas de pantalla

> Próximamente: capturas del punto de venta, el catálogo de productos y el control de inventario.

## Tecnologías

| Área | Tecnologías |
| --- | --- |
| Aplicación de escritorio | [Tauri 2](https://tauri.app/) |
| Interfaz | [React 19](https://react.dev/), TypeScript y [Tailwind CSS 4](https://tailwindcss.com/) |
| Herramientas frontend | [Vite](https://vite.dev/) y pnpm |
| Backend local | Rust y comandos de Tauri |
| Base de datos | SQLite, `rusqlite` y migraciones versionadas |
| Seguridad | Argon2 para contraseñas y CSP de Tauri |
| Pruebas | Vitest, Testing Library y pruebas de Rust |
| Distribución | Tauri Updater y actualizaciones firmadas |

## Descargas

Las versiones publicadas se encuentran en [GitHub Releases](https://github.com/98beto/pos_tauri/releases). El proyecto genera paquetes para:

- Linux: AppImage, DEB y RPM.
- Windows: instalador NSIS.

## Desarrollo local

### Requisitos

- Node.js 24.15.0, fijado en `.node-version`.
- pnpm 12.4.1, fijado por `packageManager`.
- Rust 1.98.1 con `rustfmt` y `clippy`, fijado en `rust-toolchain.toml`.
- [Dependencias de sistema de Tauri 2](https://tauri.app/start/prerequisites/) para la plataforma de desarrollo.

### Instalación y ejecución

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

El modo de desarrollo no necesita claves de firma ni configuración de publicación. Tampoco registra el actualizador ni consulta el endpoint de producción.

### Comandos útiles

```bash
# Ejecutar la interfaz en el navegador
pnpm dev

# Ejecutar todas las pruebas, incluido el gate de símbolos financieros retirados
pnpm test

# Ejecutar de forma focalizada el gate del modelo sin valoración
pnpm test:retired-costs

# Comprobar tipos y generar el frontend de producción
pnpm build

# Ensayar migración, backup y restauración SQLite desde los esquemas 8 y 12 hasta el 13
pnpm qa:migration-v8-v13

# Comprobar el modo de mantenimiento del ejecutable instalado
/usr/bin/pos_tauri maintenance-info

# Restaurar el backup indicado por el manifiesto, con Linea POS cerrada
/usr/bin/pos_tauri restore-migration-backup --database /ruta/absoluta/pos.db --backup /ruta/absoluta/pos-v12-before-v13-ID.db

# Comprobar formato, lint y pruebas del backend
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Estructura del proyecto

```text
.
├── src/                    # Interfaz React, componentes, vistas y adaptadores de Tauri
├── src-tauri/
│   ├── migrations/         # Migraciones versionadas de SQLite
│   └── src/                # Comandos, servicios, modelos y acceso a datos en Rust
├── scripts/                # Validaciones y herramientas de publicación y QA
└── docs/                   # Documentación técnica y operativa
```

La interfaz invoca comandos de Tauri definidos en Rust. Estos comandos aplican las reglas de negocio y persisten los datos en `pos.db`, dentro del directorio de datos de la aplicación.

La restauración es un modo temprano del mismo ejecutable que incluyen los instaladores; no está expuesta como comando IPC ni en la interfaz y no inicializa Tauri. Las rutas deben ser absolutas, ambos archivos deben existir y ser distintos, el destino debe ser el `pos.db` canónico y el CLI rechaza la operación mientras la aplicación conserva el lock del directorio de datos. Solo acepta el backup identificado por un manifiesto v13 pendiente o completado de ese mismo directorio.

## Calidad y pruebas

Además de las pruebas automatizadas, el proyecto incluye un ensayo reproducible de las rutas SQLite `8 -> 13` y `12 -> 13`. Verifica la eliminación intencional del modelo de valoración y de sus filas, la preservación literal de stock y datos comerciales, los fixtures v8/v12, centímetros, backup, marcadores, reapertura y rollback literal al esquema de origen. Consulta [`docs/quality-assurance.md`](docs/quality-assurance.md).

## Releases

La preparación de versiones, firma, publicación, actualización automática y pruebas de migración se documenta en [`docs/releases.md`](docs/releases.md). No se deben crear tags de release sin completar sus prerrequisitos.
