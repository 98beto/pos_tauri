# Catálogo, vistas y sesiones de caja

## Alcance aprobado

- Baja lógica y restauración de productos. Conservar ventas, movimientos, stock,
  códigos y estados de presentaciones. Los códigos permanecen reservados.
- Lista/tarjetas en catálogo, movimientos y ventas, con preferencia local por vista.
- Una caja abierta por base de datos. Apertura con fondo, ingresos y retiros con
  motivo, cierre final con contado y diferencia; operaciones cerradas inmutables.
- Bloquear todo nuevo checkout sin caja abierta. Asociar ventas atómicamente y
  conservar la idempotencia y la sesión original en reintentos.
- Ventas previas en sesión 1 histórica, cerrada, sin inventar apertura ni arqueo.
- Fuera de alcance: Clip, correcciones de unidad, roles, cortes parciales,
  devoluciones y cambios de versión/publicación.

## Reglas

- Eliminar no significa ajustar existencias ni borrar filas históricas. Los
  productos eliminados no admiten nuevas ventas, compras o conteos. Sus carritos
  deben seguir siendo recuperables para retirar líneas no disponibles.
- Restaurar conserva qué presentaciones estaban activas. Archivo/restauración
  idempotentes; una operación ya guardada puede recuperarse aun tras archivar.
- Todas las cantidades monetarias se guardan en centavos enteros.
- Esperado = apertura + totales de ventas en efectivo + ingresos - retiros.
  Tarjeta/transferencia no afectan el cajón; recibido no se suma sin restar cambio.
- Diferencia = contado - esperado. No se permiten retiros superiores al esperado.
- Aperturas, movimientos y cierres deben soportar reintentos sin duplicación y
  concurrencia con checkout; no se escriben ventas contra sesiones cerradas.
- Una sesión abierta persiste tras cerrar la aplicación o la sesión de usuario.
- Migraciones aditivas posteriores a v13; preservar migraciones y snapshots
  anteriores, ampliar validación de esquemas, backups y QA de actualización.

## Bloques y dependencias

1. **Backend catálogo** (complejo): migración de archivo, servicios, guards,
   contratos IPC/TypeScript y pruebas. Posee Rust y tipos/API de productos.
2. **Presentación** (rutinario, paralelo a 1): control y preferencia local,
   representaciones alternativas y pruebas de productos/inventario/ventas.
3. **Backend caja** (complejo, después de 1): migración, histórico, servicios,
   IPC/contratos, checkout y pruebas transaccionales/migración.
4. **UI catálogo** (rutinario, después de 1 y 2, paralelo a 3): filtro,
   confirmación de eliminación y restauración, recuperación de carrito.
5. **UI caja** (complejo, después de 3 y 4): navegación, formularios, histórico,
   estado/bloqueo de POS, sesión en ventas e integración de pruebas.
6. Auditoría integrada, correcciones con nueva revisión y verificación final.

Cada bloque tiene revisión independiente antes de cerrarse. No modificar el
trabajo previo de iconos o sus referencias.

## Verificación

- Pruebas de archivo, lookup, carrito, stock, recuperación e historia preservada.
- Selector accesible, acciones equivalentes, filtros/paginación conservados y
  preferencia tolerante a almacenamiento ausente o inválido.
- Caja única, fondos, retiros, cierre, idempotencia, carreras de checkout/cierre,
  históricos y recuperación; preservar los recibos ya confirmados.
- `pnpm test`
- `pnpm build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`
- `cargo test --locked --release --manifest-path src-tauri/Cargo.toml`
- `pnpm qa:migration-v8-v13` y QA de actualización a las nuevas versiones.
- Smoke de arranque controlado y comprobación final del diff.

## Resultado de implementación

Bloques implementados y revisados independientemente, incluidas las correcciones
de concurrencia de interfaz, recuperación de intenciones y referencias de QA.

- SQLite 014: estado activo de producto; archivo/restauración conserva stock,
  presentaciones, códigos e historia. SQLite 015: caja y asociación de ventas.
- Contrato de caja: `src-tauri/CASH_CONTRACT.md`.
- `pnpm test`: 327 pruebas de interfaz, 99 del guard transversal, 12 de versión y
  124 de release aprobadas. Dos pruebas de release se omiten por plataforma o por
  herramientas opcionales (GUI Windows y firmas con minisign).
- Rust debug: 187 pruebas unitarias y 5 de mantenimiento aprobadas. Release: 188
  unitarias y 5 de mantenimiento aprobadas. El QA ignorado ordinariamente se
  ejecutó explícitamente mediante ambos entrypoints de migración.
- Compilación frontend, formato, Clippy y QA de las cuatro fuentes a v15 aprobados.
- Arranque Tauri controlado con datos XDG aislados: binario ejecutado, esquema 15,
  `quick_check=ok`, sin ventas ni sesiones ficticias. Se detuvo por timeout previsto.
- Smoke Chromium con IPC simulado: Caja responsive (360/1280), confirmación de
  cierre sin enviarla, archivo en lista/tarjetas, movimientos y filtro de sesión.
  No sustituye una prueba end-to-end de operaciones dentro de la WebView nativa.

La primera operación real requiere abrir caja. No hay que recrear la base de datos;
las ventas previas quedan en la sesión histórica sin apertura ni arqueo fabricados.
