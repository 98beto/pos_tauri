# Planes de implementacion

Plan vigente: [`measured-presentations-inventory-cost-corrections.md`](./measured-presentations-inventory-cost-corrections.md). Su implementacion esta integrada; queda pendiente la validacion operativa sobre copia cerrada de produccion y candidato de release.

- [`pos-implementation.md`](./pos-implementation.md): plan general de la aplicacion POS.
- [`rusqlite-migration.md`](./rusqlite-migration.md): migracion del acceso a SQLite hacia Rust y `rusqlite`.
- [`inventory-manual-movements.md`](./inventory-manual-movements.md): registro manual de compras y ajustes de inventario.
- [`product-presentations-measured-inventory.md`](./product-presentations-measured-inventory.md): historia de SQLite 9-11; su modelo de costos sigue vigente y solo la captura inicial de stock/costo fue reemplazada por el plan vigente.
- [`inventory-movement-ux-v0.2.0-rollout.md`](./inventory-movement-ux-v0.2.0-rollout.md): historia de SQLite 12 y base del modelo de costos; su runbook de despliegue fue reemplazado por SQLite 13.
- [`measured-presentations-inventory-cost-corrections.md`](./measured-presentations-inventory-cost-corrections.md): plan vigente para `measured` en gramos, mililitros y centimetros, alta con inventario cero, preservacion literal de costos y correccion de precios de compra.
- [`github-actions-updater.md`](./github-actions-updater.md): CI, releases multiplataforma y actualizaciones firmadas con Tauri.
- [`first-installers-release.md`](./first-installers-release.md): correccion de Actions, firma y publicacion de los primeros instaladores.
- [`v0.1.1-linux-updaters-dev-fixes.md`](./v0.1.1-linux-updaters-dev-fixes.md): AppImage portable, updater para DEB/RPM y correccion de `tauri dev` en `v0.1.1`.

Los planes nuevos deben guardarse en esta carpeta con nombres descriptivos en `kebab-case`.
