# Planes de implementacion

## Plan vigente

[`measured-presentations-inventory-cost-corrections.md`](./measured-presentations-inventory-cost-corrections.md) es el plan vigente. Propone separar compras y conteos fisicos, retirar el costo acumulado y reemplazar nuevamente la migracion SQLite 13 antes de publicarla.

## Archivo historico

Los documentos siguientes conservan decisiones y resultados anteriores. **No son instrucciones ejecutables ni runbooks vigentes.** Sus comandos, versiones, ramas, tags, migraciones y pasos de publicacion no deben aplicarse al estado actual. Para inventario y SQLite 13 se debe usar el [plan vigente](./measured-presentations-inventory-cost-corrections.md); para operar una release se debe usar [`../releases.md`](../releases.md).

- [`pos-implementation.md`](./pos-implementation.md): implementacion general inicial de la aplicacion POS.
- [`rusqlite-migration.md`](./rusqlite-migration.md): migracion ya realizada del acceso a SQLite hacia Rust y `rusqlite`.
- [`inventory-manual-movements.md`](./inventory-manual-movements.md): modelo anterior de compras y ajustes manuales, reemplazado por compras documentales y conteos fisicos.
- [`product-presentations-measured-inventory.md`](./product-presentations-measured-inventory.md): historia de SQLite 9-11; su modelo de costos fue reemplazado por el plan vigente.
- [`inventory-movement-ux-v0.2.0-rollout.md`](./inventory-movement-ux-v0.2.0-rollout.md): historia de SQLite 12; sus decisiones de costo y su runbook de despliegue fueron reemplazados.
- [`github-actions-updater.md`](./github-actions-updater.md): diseno inicial de CI, releases multiplataforma y actualizaciones firmadas con Tauri.
- [`first-installers-release.md`](./first-installers-release.md): preparacion historica de la primera release `v0.1.0`.
- [`v0.1.1-linux-updaters-dev-fixes.md`](./v0.1.1-linux-updaters-dev-fixes.md): preparacion historica de AppImage y updaters para `v0.1.1`.

Los planes nuevos deben guardarse en esta carpeta con nombres descriptivos en `kebab-case`.
