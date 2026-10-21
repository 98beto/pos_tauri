# Control de calidad

Esta guía reúne las comprobaciones especializadas que complementan las pruebas automatizadas del proyecto.

## Ensayo de migración SQLite a esquema 13

El ensayo obligatorio cubre las rutas admitidas `8 -> 13` y `12 -> 13`. Crea datos sintéticos, copia cada base a un runtime aislado y la abre mediante la misma función `open_database_path` usada por producción. La fuente v8 usa las migraciones `001`-`008` verificadas por hash contra `main` en `e2ab2d2`; también se fijan los hashes de las migraciones inmutables `009`-`012`.

```bash
pnpm qa:migration-v8-v13
```

El comando no acepta argumentos ni rutas de datos de la aplicación y no abre la interfaz gráfica. Crea un directorio único `pos-tauri-v8-v13-qa-*` bajo el directorio temporal del sistema y al terminar imprime la ruta conservada. Es un gate requerido en CI y antes de generar un candidato de release.

Los artefactos actuales son:

- `before-v8.db` y `before-v8-report.json`.
- `runtime-v8-v13/pos.db`, su backup `pos-v8-before-v13-*.db` y su manifiesto `pos-schema-v13-migration-completed-*.json`.
- `runtime-v8-restore/pos.db`, restaurado mediante la ruta de produccion al esquema 8, con snapshot completo, reapertura, manifiesto de restauracion y cuarentena diagnostica verificados.
- `before-v12.db`.
- `runtime-v12-v13/pos.db`, su backup `pos-v12-before-v13-*.db` y su manifiesto `pos-schema-v13-migration-completed-*.json`.
- `runtime-v12-restore/pos.db`, restaurado mediante la ruta de produccion al esquema 12, con snapshot completo, reapertura, manifiesto de restauracion y cuarentena diagnostica verificados.
- `result-v8-v13.json`, `result-v12-v13.json`, `result.json` y `README.txt`.

Credenciales ficticias: `admin@pos.local` / `PosDemo123!`.

Los fixtures v8 y v12 contienen historia representativa. El fixture v12 incluye costos conocidos, desconocidos e incorrectos, una compra cuyo precio por kilo fue capturado como precio por gramo, ajustes, venta, una valoración histórica y huecos de secuencia. Esos datos financieros solo existen para demostrar su eliminación intencional al migrar.

SQLite 13 no mantiene valoración de inventario. La migración elimina las columnas financieras antiguas, no copia precios históricos a los campos de compra v3 y elimina las filas históricas de valoración, conservando el high-water mark de la secuencia. Debe preservar literalmente:

- IDs, relaciones, secuencias, timestamps, tokens y fingerprints.
- Stock, cantidades capturadas y cantidades base.
- Precio de catálogo, precio aplicado, subtotal, descuento y total cobrado.
- Presentaciones, conversiones, proveedor, documento y nota.

El ensayo comprueba la ausencia de todo campo financiero retirado y de filas de valoración, confirma que los campos de compra quedan `NULL` para historia v1/v2 y admite `centimeter` en `products`, `sale_details` e `inventory_movements`. También rechaza unidades desconocidas y versiones intermedias `9`-`11`; compara el snapshot retenido al reabrir, comprueba que una segunda apertura no duplica backup ni manifiesto y valida el manifiesto completado. La restauración usa la ruta de producción, valida manifiesto y cuarentena, conserva intacto el backup original y reabre el esquema 8 o 12 restaurado para compararlo literalmente con su fuente, incluidos los datos financieros que contenía antes de migrar.

El gate transversal se ejecuta con `pnpm test:retired-costs` y también forma parte de `pnpm test`. Rechaza nombres financieros retirados fuera de la allowlist por símbolo y ruta; el precio específico de compra permanece permitido.

### Smoke funcional y rollback

El gate de release sanea y reempaqueta primero el AppImage final; despues extrae ese AppImage y los DEB/RPM y ejecuta `maintenance-info` desde cada payload, antes de firmar o publicar. En Windows ejecuta `pos_tauri.exe maintenance-info`, inspecciona NSIS/MSI y corre Rust en debug y release; los tests de integracion mantienen por separado `pos-application.lock` y `pos-schema-v13-migration.lock` en un proceso y comprueban que otro proceso rechaza la restauracion con codigo 4.

Antes de publicar, ejecutar sobre un perfil QA desechable migrado desde una copia cerrada:

1. Crear un producto por longitud con presentación `Metro`; confirmar stock y ausencia de movimiento inicial.
2. Registrar una compra en centímetros con precio, proveedor y factura; comprobar stock, importe propio e historial.
3. Registrar un conteo mayor y uno menor; comprobar diferencia, dirección, motivo y stock resultante, sin campos comerciales.
4. Vender centímetros con precio personalizado y descuento; comprobar total comercial, stock y detalle de venta sin datos de costo.
5. Cotizar y confirmar la corrección de una compra en otra copia desechable; comprobar que solo cambian el precio y el importe de esa compra, sin replay de ventas, stock ni movimientos posteriores.
6. Inyectar o reproducir un fallo controlado y confirmar rollback transaccional sin escrituras parciales.
7. Cerrar el candidato, restaurar el backup indicado por el manifiesto y abrirlo con la versión compatible con el esquema de origen.

Estos artefactos son deliberadamente temporales y no deben utilizarse como datos reales.

### Copia cerrada de producción

El ensayo sintético no sustituye la prueba obligatoria sobre una copia externa, cerrada, consistente, anonimizada cuando corresponda y protegida de la base de producción. Esa validación operativa sigue pendiente hasta que un operador la ejecute conforme al runbook de releases. No adjuntar la copia, backups, datos financieros, datos personales ni manifiestos con rutas sensibles a CI, issues o artefactos compartidos.

## Comprobación manual de modales

Cuando una vista use `Modal`, comprobar antes de entregarla:

1. Abrir dos modales anidados o simultáneos y pulsar Escape: solo se cierra el superior.
2. Recorrer el modal superior con Tab y Shift+Tab: el foco no sale de su panel.
3. Cerrar el superior: el foco vuelve a su disparador dentro del modal anterior, o al primer control de este si el disparador ya no existe.
4. Cerrar el último modal: se recuperan el foco original, el scroll y los valores previos de `inert` y `aria-hidden`.
5. Con un modal superior abierto, confirmar en las herramientas de accesibilidad que el contenido de fondo y los modales inferiores están aislados, y que solo el superior tiene `aria-modal=true`.
6. Ejecutar la aplicación Tauri y confirmar que no aparecen infracciones de `style-src` en la consola.
