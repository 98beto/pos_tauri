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

El fixture v12 incluye costos conocidos y desconocidos y una cronología apta para corregir una compra: compra, ajuste de entrada, venta, ajuste de salida y valoración posterior. La migración debe preservar literalmente:

- IDs, relaciones, secuencias, timestamps, tokens y fingerprints.
- Stock y `inventory_cost_cents`, incluidos los valores `NULL`.
- Precio y costo de cada movimiento, además de los saldos de costo antes y después.
- `movement_cost_cents` de los detalles de venta.
- Precio de catálogo, precio aplicado, subtotal, descuento y total cobrado.
- Presentaciones, conversiones, proveedor, documento y nota.

El único cambio estructural permitido en SQLite 13 es admitir `centimeter` en `products`, `sale_details` e `inventory_movements`. El ensayo confirma ese dominio, rechaza unidades desconocidas y versiones intermedias `9`-`11` y, para ambas rutas, compara el snapshot completo al reabrir la base migrada, comprueba que una segunda apertura no altera ni duplica el backup o el manifiesto, y valida el contenido del manifiesto completado. Tambien restaura por la ruta de produccion, valida el manifiesto y la cuarentena, confirma la ausencia de marcadores pendientes, conserva intacto el backup original y reabre en solo lectura el esquema restaurado para comparar nuevamente el snapshot completo.

### Smoke funcional y rollback

Antes de publicar, ejecutar sobre un perfil QA desechable migrado desde una copia cerrada:

1. Crear un producto por longitud con presentación `Metro`, confirmar stock y costo acumulado cero y ausencia de movimiento inicial.
2. Registrar una compra en centímetros con precio de compra, proveedor y documento; comprobar stock, costo acumulado e historial.
3. Registrar un ajuste de entrada, uno de salida y una valoración; comprobar costo promedio y valor total a costo.
4. Vender centímetros con precio personalizado y descuento; comprobar total comercial, stock, costo retirado y detalle de venta.
5. Cotizar y confirmar la corrección de una compra en otra copia desechable; comprobar el replay posterior y que cantidades, precios cobrados, descuentos y totales comerciales no cambian.
6. Inyectar o reproducir un fallo controlado y confirmar rollback transaccional sin escrituras parciales.
7. Cerrar el candidato, restaurar el backup indicado por el manifiesto y abrirlo con la versión compatible con el esquema de origen.

Estos artefactos son deliberadamente temporales y no deben utilizarse como datos reales.

### Copia cerrada de producción

El ensayo sintético no sustituye la prueba obligatoria sobre una copia externa, cerrada, consistente, anonimizada cuando corresponda y protegida de la base de producción. Esa validación operativa sigue pendiente hasta que un operador la ejecute conforme al runbook de releases. No adjuntar la copia, backups, costos, datos personales ni manifiestos con rutas sensibles a CI, issues o artefactos compartidos.

## Comprobación manual de modales

Cuando una vista use `Modal`, comprobar antes de entregarla:

1. Abrir dos modales anidados o simultáneos y pulsar Escape: solo se cierra el superior.
2. Recorrer el modal superior con Tab y Shift+Tab: el foco no sale de su panel.
3. Cerrar el superior: el foco vuelve a su disparador dentro del modal anterior, o al primer control de este si el disparador ya no existe.
4. Cerrar el último modal: se recuperan el foco original, el scroll y los valores previos de `inert` y `aria-hidden`.
5. Con un modal superior abierto, confirmar en las herramientas de accesibilidad que el contenido de fondo y los modales inferiores están aislados, y que solo el superior tiene `aria-modal=true`.
6. Ejecutar la aplicación Tauri y confirmar que no aparecen infracciones de `style-src` en la consola.
