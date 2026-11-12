# Linea POS

Aplicación de punto de venta de escritorio para registrar ventas, administrar productos y controlar el inventario de un negocio desde una interfaz rápida y sencilla.

Linea POS guarda la información en una base de datos SQLite local y está disponible para Linux y Windows. La aplicación está construida con Tauri 2, React, TypeScript y Rust.

## Características

- Registro de ventas con pagos en efectivo, tarjeta o transferencia.
- Búsqueda de productos por nombre, SKU o código de barras.
- Carrito con cantidades, precios personalizados y descuentos porcentuales o fijos.
- Cálculo del efectivo recibido y el cambio a entregar.
- Catálogo de productos organizado por marcas y categorías.
- Baja lógica y restauración de productos, conservando ventas, movimientos, existencias y códigos.
- Presentaciones por pieza, paquete, peso, volumen o longitud, cada una con su propio precio y código de barras.
- Control de existencias físicas mediante compras y conteos, con soporte para piezas, peso, volumen y longitud.
- Precio e importe propios en cada compra, con corrección local que no modifica ventas, stock ni movimientos posteriores.
- Consulta de movimientos y detalle histórico por producto.
- Registro documental de facturas de proveedores desde XML CFDI, con PDF opcional y consulta en Compras.
- Historial de tickets con filtros, totales y detalle de cada venta.
- Vistas de lista y tarjetas en productos, movimientos y ventas, con preferencia local.
- Apertura de caja con fondo, ingresos y retiros de efectivo, y cierre con arqueo y diferencia.
- Ventas asociadas a una sesión de caja; el cobro requiere una caja abierta.
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

# Ensayar migración, backup y restauración SQLite desde 8, 12, 13, 14 y 15 hasta 16
pnpm qa:migration-v16
# Entrada histórica compatible
pnpm qa:migration-v8-v13
node scripts/run-v15-migration-qa.mjs

# Comprobar el modo de mantenimiento del ejecutable instalado
/usr/bin/pos_tauri maintenance-info

# Restaurar el backup indicado por el manifiesto, con Linea POS cerrada
/usr/bin/pos_tauri restore-migration-backup --database /ruta/absoluta/pos.db --backup /ruta/absoluta/backup-indicado-en-manifiesto.db

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

La restauración es un modo temprano del mismo ejecutable que incluyen los instaladores; no está expuesta como comando IPC ni en la interfaz y no inicializa Tauri. Las rutas deben ser absolutas, ambos archivos deben existir y ser distintos, el destino debe ser el `pos.db` canónico y el CLI rechaza la operación mientras la aplicación conserva el lock del directorio de datos. Solo acepta el backup identificado por un manifiesto pendiente o completado de ese mismo directorio. Los nombres `pos-schema-v13-*` se conservan por compatibilidad; el manifiesto declara las versiones reales de origen y destino.

## Uso de caja y productos eliminados

Antes de cobrar, abre **Caja** y registra el fondo inicial. La sesión es única para esa base de datos y permanece abierta al reiniciar o cerrar sesión de usuario. Los ingresos y retiros requieren motivo; al cerrar se captura el efectivo contado y se conserva el esperado, el contado y su diferencia. Tarjeta y transferencia se muestran por separado y no incrementan el efectivo del cajón.

Las ventas anteriores al control de caja se agrupan en una sesión histórica cerrada. Sus importes se conservan, pero apertura y arqueo se muestran como **No registrado**. No se necesita recrear la base de datos.

**Eliminar** oculta un producto de las operaciones nuevas sin borrar su historia ni ajustar stock. En Productos, el filtro **Eliminados** permite restaurarlo; sus códigos permanecen reservados y las presentaciones conservan su estado. La unidad base sigue sin poder modificarse después del alta.

## Facturas de proveedores

En **Compras**, selecciona **Importar factura**, elige el XML y, si lo deseas, adjunta un PDF. Revisa emisor, receptor, importes y la tabla de conceptos originales antes de registrar; cancelar no guarda el documento. En el listado puedes combinar proveedor, número y fechas, alternar lista/tarjetas y abrir **Ver detalle**. Desde el detalle puedes adjuntar un PDF si falta, abrirlo con el visor del sistema y exportar los archivos originales. La preferencia de vista se guarda localmente.

Se admite XML UTF-8 CFDI 4.0 timbrado de ingreso (`I`) de hasta 5 MiB, con namespaces y prefijos variables; el PDF opcional admite hasta 15 MiB y lo aporta el usuario. Los decimales se conservan como texto exacto. No hay extracción de PDF, OCR, IA ni servidor externo, ni certificación SAT. Registrar es únicamente documental: no crea productos, entradas de stock ni pagos o movimientos de caja. El XML y PDF se guardan en SQLite; un respaldo consistente de esa base conserva sus bytes y hashes. No existe una nueva interfaz de respaldo para esta función.

## Calidad y pruebas

Además de las pruebas automatizadas, el proyecto incluye un ensayo reproducible de las rutas SQLite `8/12/13/14/15 -> 16`. Cubre datos anteriores, backup, marcadores, reapertura, rollback y conservación de adjuntos en un respaldo v16. La verificación final y los smokes de plataforma están pendientes; consulta [`docs/quality-assurance.md`](docs/quality-assurance.md).

## Releases

La preparación de versiones, firma, publicación, actualización automática y pruebas de migración se documenta en [`docs/releases.md`](docs/releases.md). No se deben crear tags de release sin completar sus prerrequisitos.
