# Linea POS v0.3.0

## Funciones

- Productos con baja lógica y restauración, conservando existencias e historial;
  vistas de lista y tarjetas.
- Sesiones de caja con apertura, ingresos, retiros y cierre con arqueo. Las ventas
  requieren una caja abierta; el historial conserva las ventas de cada sesión.
- Facturas documentales: importación de XML CFDI 4.0 de ingreso, PDF opcional,
  consulta, filtros y exportación de los archivos originales. Registrar una factura
  no modifica las existencias ni la caja.
- Mejoras de Inventario y Compras, colores de movimientos, icono y fuentes locales
  Manrope e Inter.

## Migración y recuperación de datos

La aplicación utiliza el esquema SQLite 16. Admite migrar bases canónicas de los
esquemas 8, 12, 13, 14 y 15; una base nueva se crea en 16 y una base 16 se valida
y abre sin migrar. Las versiones 9, 10 y 11 se rechazan sin escritura. Las ventas
anteriores se vinculan a una sesión histórica cerrada sin inventar apertura ni
arqueo; si no hay ventas, no se crea esa sesión.

Al pasar por la migración 13 se eliminan intencionalmente las columnas financieras
heredadas y los movimientos históricos asociados; se conservan stock, cantidades,
importes comerciales, identificadores, relaciones y secuencias. Antes de ensayar,
cierre la aplicación y prepare una copia SQLite consistente; no pruebe por primera
vez sobre la base activa de producción.

La migración crea un backup del origen y un manifiesto que identifica sus rutas,
versiones y hash. Los marcadores `pos-schema-v13-*` conservan nombres históricos:
consulte las versiones del manifiesto. Para restaurar, cierre la aplicación y use
`restore-migration-backup` del candidato 0.3.0 que realizó la migración, con las
rutas exactas del manifiesto,
siguiendo el [runbook de migración y rollback](releases.md#runbook-de-migracion-al-esquema-16).
No sustituya manualmente la base. Después verifique la integridad y abra la base
restaurada con una versión compatible con su esquema de origen.

El backup previo a migrar no incluye operaciones posteriores, incluidas facturas y
adjuntos creados en 16: conserve antes una copia SQLite v16 consistente para
diagnóstico y preservación de esos datos. La validación en copia real, el rollback
operativo y las pruebas nativas por plataforma deben completarse antes de publicar;
estas notas no acreditan su ejecución ni una publicación.
