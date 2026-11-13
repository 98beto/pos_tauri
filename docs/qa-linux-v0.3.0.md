# QA Linux v0.3.0: candidato rechazado y corrección local

Registro sanitizado del 13 de noviembre de 2026. El procedimiento de release y
sus gates permanecen en el [plan](plans/next-release-v0.3.0.md) y el
[runbook](releases.md). Esta evidencia usa datos sintéticos aislados; no incluye
bases de datos, credenciales, documentos privados ni fixtures.

## Identidad y dictamen

- PR #1 integrado en `627b1ffb12541e0307fc364ae1dbb0a9c503bd1d`; CI PR
  `43322905706` y CI main `43325554134` aprobados para ese estado.
- Validate `43328395267`: candidato firmado de ese SHA, AppImage SHA-256
  `f3ea715774035e15505069ddbe4d2224d6432673d71a8f385aeae5d9ec5973fa`.
  **Rechazado para publicación** por P1, pese a los demás resultados positivos.
- Corrección registrada en `f51658b129f4eb604376a17183ae39e2abdccfec`, pendiente
  de integración, CI y nuevo candidato firmado. El binario de producción local,
  verificado antes de ese commit, tiene SHA-256
  `34151ecdbe186084b16384dee59d4f7e1e745800c2ab28e6f3f1a476cbcf1670`
  no es un candidato firmado ni sustituye los assets rechazados.

## P1 y reproducción

En el AppImage histórico, crear un producto sintético con existencia e historial
y pulsar una vez Eliminar en Productos. Se archiva sin aceptación efectiva. La
función `window.confirm` era asíncrona y su promesa se interpretaba como valor
verdadero; además se observó rechazo ACL de `plugin:dialog|confirm`. Restaurar
conservó stock e historia, pero no subsana la falta de consentimiento.

La corrección utiliza el Modal compartido y exige aceptación explícita; invalida
intenciones al cambiar sesión o desmontar, descarta respuestas antiguas y protege
contra doble envío. En error permite reintentar o cancelar. Las revisiones del
cambio y las regresiones locales pasaron.

Para repetir la comprobación con un build aislado, crear un producto sintético
con stock e historial, abrir Eliminar en lista y tarjetas y verificar que abrir,
Cancelar, Escape y Cerrar no archivan; confirmar con doble clic, verificar una
sola acción y conservación de datos, y restaurar. En la verificación local los
diez casos nativos pasaron; la unicidad exacta de la petición se comprobó además
mediante tests del componente. No extrapolar estos resultados a otro build.

## Evidencia y límites

El payload AppImage real rechazado se ejecutó en Linux con backend, IPC y SQLite
reales, perfil aislado y datos sintéticos: 67 registros UI y 62 comprobaciones de
datos, sin que estos resultados anulen el P1. Se ejercitaron Productos, compras y
conteos de Inventario, efectivo/tarjeta/transferencia y venta medida; caja cerró
con esperado $550, contado $545 y diferencia −$5. Dos documentos y cuatro BLOB
se conservaron; exportaciones XML/PDF coincidieron en bytes y hash. Se probaron
selectores GTK y visor PDF reales, cancelaciones y reinicios con caja abierta y
cerrada. Las operaciones documentales no alteraron stock ni caja. La ventana
nativa tenía mínimo 800 px: no acredita 360 px nativos.

Para la corrección, `pnpm test` pasó 642 pruebas: 398 frontend, 99 guard, 12
versión y 133 release; la suite de Productos comprende 46. Se omitieron la GUI
Windows/PowerShell y la integración opcional Tauri/minisign. `pnpm build` y
`pnpm tauri build --no-bundle -- --locked` pasaron. Este último produjo el
binario local con frontend corregido embebido, probado en perfil sintético aislado.

Las fuentes locales de estos resultados son
`/tmp/opencode/native-qa-v030-e9965vog/RESULTADOS.md` y
`/tmp/opencode/native-qa-v030-7u94ew16/CORRECCION.md`; son temporales y pueden
desaparecer. No compartir los datos, capturas o archivos de esos perfiles sin
revisión específica. Los resultados interactivos acreditan únicamente Linux y
el build identificado en cada caso: no acreditan Windows GUI, copia cerrada de
datos reales, instalación real DEB/RPM ni actualización N−1→N.

## Continuación

Integrar la corrección mediante PR y CI, generar un nuevo candidato firmado del
nuevo SHA de `main`, verificar procedencia, firmas y hashes, y repetir la QA
requerida con esos mismos bytes. Permanecen pendientes la copia real consistente,
rollback y las pruebas de plataforma, instalación y actualización del plan.
La política acordada para el único mantenedor permite PR con CI verde y cero
aprobaciones externas; no elimina la aprobación del propietario para release.
No existe aún tag ni release v0.3.0; no inferir aprobación del candidato nuevo.
