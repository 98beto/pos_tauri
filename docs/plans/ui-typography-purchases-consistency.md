# Tipografía y consistencia visual de Inventario y Compras

**Estado:** implementado, revisado independientemente y verificado en Linux.

## Objetivo y decisiones

El usuario aprobó **Manrope para títulos e Inter para el resto de la interfaz**.
También solicitó acciones de productos más a la derecha en Inventario y una vista
de Compras visualmente consistente con Productos y Ventas.

Este plan reúne esos tres ajustes. La base funcional es el esquema 16 y la rama
actual `feat/catalog-cash-sessions`, con cambios previos de facturas y caja.

## 1. Tipografía local

- Incluir Manrope variable mediante Fontsource, siguiendo el patrón de fuentes
  empaquetadas del proyecto.
- Sustituir Fraunces en `--font-heading` por `Manrope Variable`, con fallback sans.
- Conservar Inter para tablas, formularios, botones, descripciones y cifras.
- Mantener los tamaños actuales al principio; usar los pesos existentes 600/700
  de encabezados para no introducir simultáneamente una escala visual distinta.
- No modificar el contenido de las cantidades ni el formato de importes.
- Retirar importación y dependencia de Fraunces si no quedan consumidores activos.
- Instalar las dependencias y actualizar deliberadamente `pnpm-lock.yaml`.
- Las fuentes se sirven desde assets locales de la app: no llamadas a una API,
  CDN, cuentas, claves ni instalación de fuentes en el equipo del usuario.

**Archivos previstos:** `package.json`, `pnpm-lock.yaml`, `src/main.tsx`, `src/index.css`.

## 2. Acciones de productos en Inventario

Se refiere a **Comprar**, **Contar** y **Ver historial**, en la pestaña Productos
de Inventario.

- En tabla: encabezado y contenido de Acciones alineados al extremo derecho.
- Agrupar botones con separación uniforme y sin acercarlos a los datos del producto.
- En tarjetas: acciones al pie, alineadas a la derecha.
- Conservar orden, textos, handlers, controles de operaciones pendientes y foco
  al regresar del historial.
- Permitir distribución responsive y desplazamiento propio de la tabla sin
  desbordamiento horizontal del documento.
- Conservar los indicadores de movimientos verdes/rojos y su vista lista/tarjetas.

**Archivo previsto:** `src/features/inventory/InventoryView.tsx`.

## 3. Consistencia de Compras

### Encabezado

- Tarjeta de sección con borde y separación interna iguales a Productos/Ventas.
- Título y descripción a la izquierda.
- Grupo de controles a la derecha con **Lista / Tarjetas** y **Importar factura**.
- Importar factura con variante primaria azul y un icono de carga decorativo si
  aporta claridad, sin dependencias adicionales.
- Mantener las condiciones actuales que deshabilitan importar durante una operación
  o mientras esté abierta una revisión/detalle.

### Filtros

- Franja separada con fondo suave, borde inferior y espaciados consistentes.
- Mantener los tres conceptos principales: proveedor, número y rango de emisión.
- Agrupar visualmente la búsqueda y selección de proveedor, sin perder ninguna.
- Conservar nombres accesibles, límites, validaciones, debounce y comportamiento
  de selección por RFC.
- Limpiar filtros como acción secundaria con tamaño normal y ubicación consistente.
- No añadir métricas que confundan sugerencias de proveedores con totales reales,
  ni sumar importes de monedas diferentes.

### Resultados y estados

- Tabla con encabezados, bordes de fila, padding y hover consistentes con las otras vistas.
- Total y acciones alineados a la derecha.
- Tarjetas con jerarquía clara y Ver detalle al pie a la derecha.
- Paginación como pie separado, con Anterior, información de página y Siguiente.
- Mensajes de carga, error y vacío usando los componentes/estilos de la aplicación.
- Conservar tabla/tarjetas, preferencia local, filtros y página al consultar detalles.
- Mantener la tabla de conceptos originales, adjuntos, decimales exactos y flujo
  de importación ya implementados; el ajuste es de presentación y distribución.

**Archivo previsto:** `src/features/purchases/PurchasesView.tsx`.

## 4. Ejecución y propiedad de archivos

| Bloque | Agente | Propiedad | Dependencias |
| --- | --- | --- | --- |
| A. Fuentes | implementer | package, lock, entrada y tokens CSS | Context7 Fontsource y estructura existente |
| B. Acciones | implementer | InventoryView | Patrones actuales de tabla/tarjeta |
| C. Compras | implementer | PurchasesView | Contratos y componentes existentes |
| D. Revisión | reviewer independiente | Sólo lectura de cada bloque | A/B/C implementados |
| E. Verificación | verifier y smoke visual | Sólo comprobaciones | Revisiones sin bloqueantes |

A, B y C pueden ejecutarse en paralelo porque no comparten propiedad de archivos.
Cada bloque recibe revisión independiente. Si hay correcciones, se revisan después
por un agente nuevo. El índice y el resultado del plan quedan a cargo del coordinador.

## 5. Verificación y criterios de aceptación

- `pnpm test` y `pnpm build` deben terminar correctamente.
- `git diff --check`, incluido el plan nuevo, debe quedar limpio.
- Las fuentes Manrope e Inter deben cargar realmente; comprobar familia calculada,
  archivos WOFF2 en el build y ausencia de solicitudes a dominios externos.
- Verificar en Chromium con la app real y datos sintéticos mediante IPC simulado,
  a 360 y 1280 px, distinguiendo esa evidencia de una prueba nativa end-to-end.
- En Inventario: botones visibles al extremo derecho de su columna y al pie de las
  tarjetas; Comprar, Contar e historial siguen siendo accesibles y operativos.
- En Compras: Importar factura es la acción primaria junto al selector de vista;
  filtros, lectura de tabla/tarjetas, detalle y paginación conservan su funcionamiento.
- Revisar que cambiar de modo no hace consultas nuevas ni borra filtros/página y
  que los modales siguen teniendo controles y foco correctos.
- Usar las pruebas existentes. No añadir tests que sólo comparen clases CSS para
  estos ajustes reversibles; añadir regresiones si se descubre un defecto funcional.
- No repetir Rust, migraciones o empaquetado por estos cambios exclusivamente
  frontend, salvo que un fallo concreto revele una dependencia no contemplada.

## 6. Conservación del trabajo previo

- Mantener todos los cambios previos de facturas, caja y estilos de movimientos.
- No cambiar backend, esquema, importes, unidades, inventario ni reglas de caja.
- No crear commits, publicar ni integrar en `main` como parte de este pedido.
- Los documentos de proveedores añadidos al workspace por el usuario no se
  requieren para este ajuste y no se incorporan a fixtures ni se modifican.

## Referencias

- [Fontsource: uso con Vite](https://fontsource.org/docs/guides/vite).
- [Fontsource: instalación local](https://fontsource.org/docs/getting-started/install).
- [Importación documental ya implementada](./purchase-invoice-import.md).

## Resultado de ejecución

- Manrope Variable 5.3.0 sustituye a Fraunces; Inter Variable 5.3.0 conserva el
  resto de la interfaz. El cambio de lock sólo sustituye esa familia.
- Las acciones de Inventario quedan al extremo derecho de la tabla y al pie
  derecho de las tarjetas. Comprar, Contar e historial conservaron sus handlers.
- Compras usa encabezado agrupado, importación primaria azul, filtros separados,
  tabla y tarjetas consistentes y paginación al pie.
- Las revisiones independientes y la revisión nueva de correcciones no dejaron
  bloqueantes. Se corrigieron la etiqueta de Proveedor y el inlineado de un WOFF2
  pequeño: Vite emite fuentes como archivos sin ampliar la CSP.
- `pnpm test`: 384 pruebas frontend, 99 del guard transversal, 12 de versión y
  124 de release aprobadas; dos pruebas de release omitidas por plataforma o
  herramientas opcionales. `pnpm build` y comprobaciones de formato aprobados.
- Chromium sobre el build real con la CSP de Tauri: familias calculadas correctas,
  carga de WOFF2 de Manrope/Inter y del subconjunto cirílico extendido; todos los
  recursos desde el servidor local, sin infracciones CSP.
- Smoke a 360/1280 px: alineación calculada de acciones, formularios de compra/conteo,
  retorno de foco del historial, importación cancelada, filtros combinados y detalle
  documental conservados; alternar tarjetas no vuelve a consultar facturas.
- El smoke visual usa IPC simulado y fixtures sintéticos, no documentos del usuario
  ni operaciones reales sobre su base. La evidencia no equivale a un recorrido
  nativo de instaladores o a una prueba en Windows.
