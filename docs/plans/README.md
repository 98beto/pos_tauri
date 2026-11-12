# Planes de implementacion

## Plan de próxima release

[`next-release-v0.3.0.md`](./next-release-v0.3.0.md) propone la preparación y ejecución de v0.3.0: pruebas de usuario, integración en main, candidato firmado, QA por plataforma y publicación de los mismos bytes. Es un plan pendiente, no una release ejecutada.

## Último ajuste visual implementado

[`ui-typography-purchases-consistency.md`](./ui-typography-purchases-consistency.md) documenta Manrope/Inter, acciones de Inventario alineadas a la derecha y Compras consistente con las otras vistas, con revisiones y verificación automatizada/visual aprobadas en Linux.

## Última implementación

[`purchase-invoice-import.md`](./purchase-invoice-import.md) describe el registro y consulta de facturas desde XML CFDI, PDF opcional y filtros por proveedor, número y fecha. Implementado en esquema 16, con revisiones y gates automatizados aprobados en Linux y smoke visual con IPC simulado. El plan distingue la evidencia realizada del recorrido nativo interactivo y Windows pendientes. Es un archivo documental de compras: no crea productos ni movimientos de inventario o caja.

## Funcionalidad ya implementada

[`catalog-layout-cash-sessions.md`](./catalog-layout-cash-sessions.md) documenta la baja lógica, vistas lista/tarjetas y sesiones de caja ya implementadas en SQLite 14 y 15. El contrato de caja está en [`../../src-tauri/CASH_CONTRACT.md`](../../src-tauri/CASH_CONTRACT.md). El plan de facturas toma ese esquema como base y añade la migración 016 sin modificar las anteriores.

## Archivo historico

Los documentos siguientes conservan decisiones y resultados anteriores. **No son instrucciones ejecutables ni runbooks vigentes.** Sus comandos, versiones, ramas, tags, migraciones y pasos de publicacion no deben aplicarse al estado actual. El diseño de inventario de SQLite 13 se conserva como referencia; sus migraciones son ahora inmutables. Para operar una release se debe usar [`../releases.md`](../releases.md).

- [`pos-implementation.md`](./pos-implementation.md): implementacion general inicial de la aplicacion POS.
- [`measured-presentations-inventory-cost-corrections.md`](./measured-presentations-inventory-cost-corrections.md): diseño ya implementado del esquema 13, compras y conteos; no volver a reemplazar la migracion 013.
- [`rusqlite-migration.md`](./rusqlite-migration.md): migracion ya realizada del acceso a SQLite hacia Rust y `rusqlite`.
- [`inventory-manual-movements.md`](./inventory-manual-movements.md): modelo anterior de compras y ajustes manuales, reemplazado por compras documentales y conteos fisicos.
- [`product-presentations-measured-inventory.md`](./product-presentations-measured-inventory.md): historia de SQLite 9-11; su modelo de costos fue reemplazado por `measured-presentations-inventory-cost-corrections.md`.
- [`inventory-movement-ux-v0.2.0-rollout.md`](./inventory-movement-ux-v0.2.0-rollout.md): historia de SQLite 12; sus decisiones de costo fueron reemplazadas por `measured-presentations-inventory-cost-corrections.md`; el runbook vigente está en `../releases.md`.
- [`github-actions-updater.md`](./github-actions-updater.md): diseno inicial de CI, releases multiplataforma y actualizaciones firmadas con Tauri.
- [`first-installers-release.md`](./first-installers-release.md): preparacion historica de la primera release `v0.1.0`.
- [`v0.1.1-linux-updaters-dev-fixes.md`](./v0.1.1-linux-updaters-dev-fixes.md): preparacion historica de AppImage y updaters para `v0.1.1`.

Los planes nuevos deben guardarse en esta carpeta con nombres descriptivos en `kebab-case`.
