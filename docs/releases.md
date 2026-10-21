# Operacion de releases

Esta guia corresponde al repositorio publico `98beto/pos_tauri`, la rama `main` y los workflows `.github/workflows/release-request.yml` y `.github/workflows/release.yml`. El canal estable del updater es:

```text
https://github.com/98beto/pos_tauri/releases/latest/download/latest.json
```

El workflow admite `validate` y `publish`. `validate` compila el SHA inmutable del dispatch desde `main` (`github.sha`), firma y verifica todos los bundles y conserva durante siete dias un artifact candidato ligado a tag, commit y run ID, sin crear ni modificar releases. `publish` nunca recompila: descarga ese artifact ya ensamblado, verifica procedencia, hashes y firmas, crea o reutiliza una release en borrador y promueve exactamente los diez archivos que fueron aprobados.

## Prerrequisitos locales

- Acceso de escritura al repositorio y permiso para crear tags.
- Rama local `main` actualizada, limpia y basada en `origin/main`.
- Node.js 24.15.0, pnpm 12.4.1 y Rust 1.98.1 con `rustfmt` y `clippy`.
- Dependencias de sistema de Tauri 2 si se ejecutan builds nativos locales.
- Cambios de la release ya integrados en `main` y CI verde.
- Variable, environments protegidos y secrets de firma configurados en GitHub antes de crear el tag.
- Ruleset inmutable para tags `v*` y GitHub immutable releases habilitados. Una release no debe comenzar sin ambas protecciones.

Las versiones se fijan en `.node-version`, `package.json`, `rust-toolchain.toml` y los workflows. Una actualizacion de toolchain es un cambio independiente y debe pasar CI antes de preparar una release.

El identificador Tauri `com.beto98.pos-tauri` es un invariante de release. No cambiarlo: determina la identidad instalada y la ubicacion de los datos; modificarlo puede hacer que SQLite parezca perdido o crear una segunda instalacion.

## Clave del updater

### Generacion y respaldo inicial

Generar la pareja una sola vez, fuera del repositorio y desde una terminal privada:

```bash
pnpm tauri signer generate -w ~/.tauri/linea-pos-updater.key
```

Usar el prompt interactivo para una contrasena robusta. No pasar la contrasena como argumento, no usar `--ci` para produccion y no pegar claves en comandos, historial, tickets o logs. El comando crea la clave privada en la ruta indicada y una clave publica asociada; ninguno de esos archivos debe copiarse dentro del repositorio.

Conservar al menos dos respaldos cifrados e independientes de la clave privada y su contrasena, con acceso restringido y una restauracion probada. Perder la clave impide actualizar los clientes que confian en ella. Nunca almacenar la clave privada ni la contrasena en Git, `.env`, artefactos de Actions o notas de release.

### Configuracion en GitHub

En `Settings > Secrets and variables > Actions`:

- Crear la variable de repositorio `TAURI_UPDATER_PUBLIC_KEY` con el contenido completo de la clave publica.

En `Settings > Environments`:

- Crear el environment `release`, configurar `Deployment branches and tags` como `Selected branches and tags` con el patron `main`, exigir required reviewers y activar `Prevent self-review` cuando el plan de GitHub lo permita.
- Crear preferiblemente como environment secrets de `release` el secret `TAURI_SIGNING_PRIVATE_KEY` con el contenido completo de la clave privada y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` con su contrasena. Si temporalmente son repository secrets, el environment protegido sigue siendo obligatorio.
- Crear un environment distinto `release-publication`, configurar el mismo patron de deployment `main`, exigir required reviewers y activar `Prevent self-review` cuando este disponible. No agregarle variables ni secrets de firma: solo aprueba la promocion de bytes ya firmados. Los revisores deben cotejar el run ID solicitado con la evidencia de migracion, smoke y rollback antes de aprobar.

Introducir los valores solamente en los campos protegidos de la interfaz. Evitar comandos CLI que incluyan el valor en argumentos, porque pueden quedar en el historial o la lista de procesos. La variable publica se inyecta en un overlay efimero durante la release; `src-tauri/tauri.conf.json` no contiene una clave de produccion ni un valor sustituible que pueda publicarse por accidente.

Para probar bundles antes del tag, ejecutar manualmente `Release` desde `main` con `mode=validate`. El modo `publish` requiere un tag existente y puede publicar la release; comprobar siempre el modo seleccionado.

## Preparar una version

1. Elegir una version SemVer sin prefijo, por ejemplo `1.2.3`.
2. Cambiarla de forma identica en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
3. Regenerar los locks con las herramientas fijadas cuando sea necesario; no editar dependencias bloqueadas a mano.
4. Revisar y confirmar cualquier cambio esperado en `pnpm-lock.yaml` y `src-tauri/Cargo.lock`. El paquete raiz de Cargo debe reflejar la nueva version.
5. Ejecutar las validaciones usando el tag que se creara.

Comandos para el ejemplo:

```bash
pnpm install
cargo check --manifest-path src-tauri/Cargo.toml
pnpm install --frozen-lockfile
pnpm validate:version v1.2.3
pnpm test
pnpm build
pnpm qa:migration-v8-v13
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock
```

El ultimo comando se ejecuta despues de confirmar los cambios de version y locks; comprueba que instalaciones y pruebas no los modificaron de nuevo. `pnpm validate:version` rechaza formatos distintos de `vX.Y.Z` y discrepancias entre los tres archivos.

## Runbook de migracion al esquema 13

SQLite 13 adopta un modelo sin valoracion de inventario. Elimina intencionalmente las columnas financieras antiguas y los movimientos historicos de valoracion, no traslada precios historicos a los campos de compra v3 y amplia a `centimeter` el dominio `base_unit`. Conserva literalmente stock, cantidades, importes comerciales, IDs, relaciones y secuencias; la secuencia de movimientos no retrocede aunque se elimine su fila de ID mas alto.

Se admiten solamente `8 -> 13` y `12 -> 13`. Una base nueva se crea directamente en 13; una base 13 se valida y abre sin migrar; las versiones 9, 10 y 11 se rechazan sin escritura. Nunca ejecutar el primer ensayo sobre el archivo activo de produccion.

La prueba automatizada requerida por CI y release es:

```bash
pnpm qa:migration-v8-v13
```

El comando valida fixtures v8 y v12 con datos financieros conocidos, desconocidos e incorrectos, el caso de precio por kilo capturado como precio por gramo, compras, ajustes, ventas y valoraciones historicas. Comprueba su eliminacion intencional, preservacion literal de los campos retenidos, centimetros, rechazo de unidades desconocidas y versiones intermedias, backup, marcadores, reapertura y restauracion. No sustituye la prueba externa sobre una copia cerrada de produccion, que sigue pendiente hasta su ejecucion por un operador.

### Candidato y copia cerrada

1. Registrar el commit exacto y ejecutar `Release` desde `main` con `mode=validate`.
2. Descargar el artifact `release-candidate-<sha>` y registrar el run ID del `validate`; comprobar que `candidate-provenance.json` contiene el tag, SHA y run ID esperados y ejecutar `sha256sum --check SHA256SUMS`.
3. Cerrar Linea POS en el equipo origen y confirmar que no quedan procesos ni operaciones activas.
4. Confirmar `PRAGMA user_version` igual a 8 o 12, `quick_check=ok` y cero filas de `foreign_key_check`.
5. Crear una copia consistente mediante `.backup`; no usar `cp` si puede existir WAL pendiente.
6. Mantener copia, backups y manifiestos en almacenamiento restringido y anonimizar antes de sacarlos del entorno autorizado.

Antes de migrar una fuente 12, registrar en evidencia privada snapshots literales de productos, presentaciones, ventas, detalles y movimientos, junto con `sqlite_sequence`. Separar los campos y filas financieros que deben desaparecer de los campos retenidos que deben permanecer identicos: stock, cantidades, precio de catalogo, precio aplicado, subtotales, descuentos y totales. No publicar esos datos.

### Migrar solamente la copia

Usar una maquina, VM o perfil QA sin acceso al directorio de datos real. Mantener el identificador Tauri `com.beto98.pos-tauri`; aislar el directorio de datos en vez de cambiar la identidad de la aplicacion.

La aplicacion crea:

- `pos-v8-before-v13-<attempt-id>.db` o `pos-v12-before-v13-<attempt-id>.db`.
- `pos-schema-v13-migration-pending.json` mientras la migracion no esta cerrada durablemente.
- `pos-schema-v13-migration-completed-<attempt-id>.json` al completar.
- `pos-schema-v13-migration.lock` para excluir aperturas concurrentes.
- `pos-schema-v13-restore-pending.json` durante una restauracion.
- `pos-schema-v13-restore-completed-<restore-id>.json` al completar la restauracion.

Tras el primer inicio, comprobar `user_version=13`, `quick_check=ok`, cero violaciones de FK, exactamente un backup y un manifiesto completado, y ausencia del marcador pendiente. Leer siempre `database_path`, `backup_path`, `source_version`, `target_version`, `backup_size_bytes`, `backup_sha256` y `backup_validation_completed` desde el manifiesto. Reabrir y confirmar que no se crean otro backup ni otro manifiesto.

Para una fuente 12, comparar antes y despues todos los campos retenidos. Confirmar por separado que no quedan columnas financieras antiguas, precios historicos copiados ni filas de valoracion. Las demas diferencias de esquema se limitan a los campos de compra v3 y a `centimeter` en los tres dominios de unidad.

### Smoke funcional

1. Productos: crear una longitud con presentacion `Metro`; confirmar stock y ausencia de movimiento inicial.
2. Compras: registrar una compra en centimetros con precio, proveedor y factura; comprobar stock, precio e importe propios.
3. Conteos: capturar existencias reales mayores y menores; comprobar diferencia, motivo y stock resultante sin datos comerciales.
4. POS: vender centimetros con precio personalizado y descuento; comprobar total comercial y stock.
5. Ventas: comprobar snapshots, precio aplicado, subtotal, descuento y total historico, sin datos de costo.
6. Correccion: sobre otra copia desechable, cotizar y corregir una compra con movimientos posteriores; comprobar que solo cambian el precio y el importe de esa compra y que cantidades, stock, ventas y movimientos posteriores permanecen iguales.
7. Rollback transaccional: provocar un rechazo seguro, por ejemplo fingerprint obsoleto, y confirmar que no hay escrituras parciales.

### Restauracion y rollback

Ensayar el rollback antes de publicar. Cerrar completamente el candidato, conservar una copia diagnostica de SQLite 13 y tomar el backup exacto desde el manifiesto completado o el marcador pendiente. Validar que conserva el `source_version` y el snapshot de origen.

Leer `database_path` y `backup_path` del manifiesto `pos-schema-v13-migration-completed-<attempt-id>.json` o, si la migracion quedo pendiente, de `pos-schema-v13-migration-pending.json`. Cerrar Linea POS por completo. El comando real depende del artefacto instalado; sustituir las rutas por los dos valores exactos del manifiesto:

```bash
# AppImage descargado
./Linea.POS_0.2.1_amd64.AppImage restore-migration-backup --database /ruta/absoluta/pos.db --backup /ruta/absoluta/pos-v12-before-v13-ID.db

# DEB o RPM instalado
/usr/bin/pos_tauri restore-migration-backup --database /ruta/absoluta/pos.db --backup /ruta/absoluta/pos-v12-before-v13-ID.db
```

```powershell
# NSIS instalado por usuario (Start-Process espera al ejecutable GUI y conserva ExitCode)
# ArgumentList recibe una sola cadena con comillas dobles literales para las rutas con espacios.
$restoreArgs = 'restore-migration-backup --database "C:\ruta con espacios\pos.db" --backup "C:\ruta con espacios\pos-v12-before-v13-ID.db"'
$result = Start-Process -FilePath "$env:LOCALAPPDATA\Linea POS\pos_tauri.exe" -ArgumentList $restoreArgs -Wait -PassThru
$result.ExitCode

# MSI instalado para el equipo
$result = Start-Process -FilePath "$env:ProgramFiles\Linea POS\pos_tauri.exe" -ArgumentList $restoreArgs -Wait -PassThru
$result.ExitCode
```

El CLI adquiere `pos-application.lock` antes de resolver los archivos y el lock de migracion antes de validarlos. Rechaza symlinks/reparse points, exige que backup, manifiesto y `pos.db` pertenezcan al mismo directorio canonico, comprueba esquema, atestaciones, tamaño y SHA-256, y repite identidad/hash bajo lock inmediatamente antes del primer movimiento. Si Linea POS esta ejecutandose, un lock no puede adquirirse o su archivo es sustituido, falla con codigo 4 sin restaurar; los fallos de rutas, manifiestos o datos conservan su categoria propia. No borrar los locks manualmente: son locks del sistema operativo y sus archivos pueden permanecer normalmente en disco.

El comando usa el mecanismo durable del backend: solo acepta el esquema canónico completo v8 o v12 declarado por ese manifiesto, crea el marcador de restauracion, pone en cuarentena la base y los sidecars de SQLite 13, promociona un temporal validado, persiste el manifiesto completado y cierra SQLite. Debe terminar con `Restauracion durable completada`; cualquier otro resultado se considera fallo. Códigos estables: `0` éxito, `2` sintaxis, `3` ruta/archivo, `4` lock o aplicación activa, `5` restauración rechazada/fallida y `6` cierre SQLite fallido. No reemplazar manualmente `pos.db`.

Despues, confirmar `PRAGMA user_version` igual al `source_version`, `quick_check=ok`, cero filas de `foreign_key_check` y la presencia de `pos-schema-v13-restore-completed-<restore-id>.json`. Abrir y reabrir el resultado con la version compatible con el esquema 8 o 12 restaurado. Comparar literalmente productos, stock, historia financiera de origen, ventas, detalles, movimientos y secuencias. Toda operacion creada despues de migrar a 13 se pierde al restaurar el backup.

Este procedimiento corresponde a `v0.2.1`. Su implementacion local no acredita por si sola la prueba sobre una copia cerrada real, `mode=validate`, smoke de artefactos, rollback operativo, tag ni publicacion.

### Checklist

No publicar hasta que migracion, reapertura, smoke y rollback pasen sobre los archivos del mismo run ID de `validate`. La evidencia debe registrar `SHA256SUMS`; el tag se crea despues de aprobar esos bytes y `publish` debe recibir ese run ID, no generar otro candidato.

- [ ] `pnpm qa:migration-v8-v13` termina correctamente en CI y en el candidato.
- [ ] `pnpm test:retired-costs` confirma que no reaparecen simbolos financieros retirados.
- [ ] Copia externa cerrada y consistente de produccion creada; las pruebas del repositorio no completan este paso.
- [ ] Fuente 8 o 12 integra, preflight aprobado y snapshot privado registrado.
- [ ] Commit, validate run ID, plataforma, instalador y `SHA256SUMS` registrados.
- [ ] Esquema 13 integro, reapertura idempotente y manifiesto apuntando al backup exacto.
- [ ] Campos y filas financieros antiguos eliminados intencionalmente; stock, campos comerciales, IDs, relaciones y secuencias retenidos literalmente.
- [ ] Compras historicas v1/v2 no reciben precio ni importe de compra v3.
- [ ] Smoke de productos, compras, conteos, checkout, ventas y correccion local completado con centimetros.
- [ ] Rechazo o fallo controlado revierte sin escrituras parciales.
- [ ] Restauracion durable devuelve el esquema 8 o 12 y la version compatible lo abre.
- [ ] `maintenance-info` y el rollback se ejecutan desde el AppImage o instalación DEB/RPM/NSIS/MSI del candidato, sin checkout ni toolchain.
- [ ] No hay bases, backups, datos financieros, rutas sensibles, datos personales, tokens ni secretos en evidencia compartida.
- [ ] Tag y `publish` cuentan con aprobacion explicita.

Crear un commit normal mediante pull request. Cuando este en `main`, crear el tag anotado sobre el commit exacto y publicarlo:

```bash
git switch main
git pull --ff-only origin main
git tag -a v1.2.3 -m "Linea POS v1.2.3"
git push origin v1.2.3
# Si la ruta automatica encuentra mas de un candidato, publicar explicitamente el aprobado:
gh workflow run release.yml --ref main -f mode=publish -f tag=v1.2.3 -f candidate_run_id=VALIDATE_RUN_ID
```

No mover ni reutilizar un tag enviado. El workflow exige que su commit pertenezca a la historia de `origin/main` y que no exista una release ya publicada para ese tag.

## Ejecucion del workflow

Un push de `v*` ejecuta primero `Release request`, que no tiene secrets ni permisos de escritura. Al terminar correctamente, `Release` se carga desde la rama default mediante `workflow_run`, obtiene exactamente un tag SemVer que apunte a `workflow_run.head_sha`, exige que el SHA pertenezca a `origin/main` y busca el candidato previo de ese SHA. No compila en esta ruta. La publicacion automatica continua solamente si existe un unico artifact valido; si hay varios, falla de forma segura y se debe usar `workflow_dispatch` indicando el run ID aprobado.

`workflow_dispatch` solo puede lanzarse seleccionando `main`:

- `mode=validate` fija `github.sha` del evento/run y comprueba que pertenece a la historia de `origin/main`. Usa ese mismo SHA para los checkouts, el tag derivado de `package.json`, los metadatos y el artifact candidato, aunque `main` avance despues del dispatch o antes de un reintento. El `head_sha` del run coincide asi con el candidato seleccionable mediante `candidate_run_id`. Deja omitido el job con `contents: write`.
- `mode=publish` requiere un tag `vX.Y.Z` existente y `candidate_run_id` con el run ID de `validate` aprobado. Sirve para la publicacion normal, recuperacion o reintento y no ejecuta tests, Vite, Cargo ni Tauri.

Cada job de compilacion genera `dist` con `pnpm build` antes de cualquier acceso a secrets. Linux compila sin la clave privada, sanea el AppImage y expone la clave solamente al paso que firma los tres instaladores finales. NSIS tambien compila sin la clave y la expone solo al firmador posterior. MSI no recibe secretos ni configuracion del updater. El overlay efimero anula `build.beforeBuildCommand`, por lo que Vite o sus plugins no heredan credenciales de firma.

Windows MSI, que no recibe secretos, puede compilar mientras espera la aprobacion. Tras la aprobacion de los required reviewers, se ejecutan Linux y Windows NSIS. El job `assemble`, con `contents: read`, se ejecuta solamente en `validate` y:

1. Canonicaliza los nombres de assets.
2. Verifica criptograficamente las cuatro firmas locales.
3. Genera `latest.json` y valida version, nombres, URLs, firmas y conjunto exacto.
4. Calcula `SHA256SUMS`, escribe `candidate-provenance.json` con tag, SHA y run ID, y sube `release-candidate-<sha>` con esos dos registros y los nueve assets nativos mas `latest.json`.

Solo en `publish`, un job separado con `contents: write` y environment `release-publication` descarga el artifact del run aprobado, confirma procedencia, tag, commit, conjunto, `SHA256SUMS` y firmas, crea o reutiliza el borrador, sube exclusivamente los diez archivos registrados, vuelve a descargarlos, compara sus SHA-256 y publica la release como `latest`. No instala dependencias ni ejecuta ningun comando de build.

## Artefactos esperados

La config base mantiene `createUpdaterArtifacts=false`, por lo que builds normales no requieren firma. Para `1.2.3`, el workflow firma los cuatro canales updater y exige estos nueve assets nativos antes de agregar `latest.json`:

```text
Linea.POS_1.2.3_amd64.AppImage
Linea.POS_1.2.3_amd64.AppImage.sig
Linea.POS_1.2.3_amd64.deb
Linea.POS_1.2.3_amd64.deb.sig
Linea.POS-1.2.3-1.x86_64.rpm
Linea.POS-1.2.3-1.x86_64.rpm.sig
Linea.POS_1.2.3_x64-setup.exe
Linea.POS_1.2.3_x64-setup.exe.sig
Linea.POS_1.2.3_x64_en-US.msi
latest.json
```

Una version futura cambia solo el segmento de version. Los basenames, producto, arquitectura y locale deben coincidir exactamente. No son validos los formatos heredados `*.AppImage.tar.gz`, `*.AppImage.tar.gz.sig`, `*.nsis.zip` o `*.nsis.zip.sig`.

El artifact interno de Actions contiene ademas `SHA256SUMS` y `candidate-provenance.json`. Son registros de handoff y no se publican como assets de GitHub Release; los diez archivos publicados deben coincidir byte por byte con las entradas de `SHA256SUMS`.

| Plataforma | Instalacion manual | Updater |
| --- | --- | --- |
| Linux | AppImage, `.deb`, `.rpm` | AppImage, DEB y RPM, cada uno con su `.sig` |
| Windows | NSIS `setup.exe`, MSI | NSIS `setup.exe` y su `.sig` |

`latest.json` contiene solamente `linux-x86_64-appimage`, `linux-x86_64-deb`, `linux-x86_64-rpm` y `windows-x86_64-nsis`. Cada entrada apunta a su formato nativo y usa la firma del mismo archivo. MSI permanece manual y nunca aparece como URL del updater.

AppImage se actualiza reemplazando el ejecutable. DEB usa `dpkg -i` y RPM usa `rpm -U`; ambos pueden requerir elevacion administrativa. El usuario confirma antes de descargar e instalar. Actualizar o desinstalar el paquete no elimina `pos.db`, almacenada en el directorio de datos de `com.beto98.pos-tauri`.

El frontend del updater se compila con `VITE_UPDATER_ENABLED=true` solamente para AppImage, DEB, RPM y NSIS. MSI y builds de produccion sin overlay no registran ni ofrecen el updater.

## Revision, publicacion y cancelacion

El borrador solo existe durante la fase final y puede durar pocos segundos. Si se necesita observarlo, abrir `Releases` mientras corre `Atomically publish verified candidate` y comprobar:

- Tag, titulo y notas correctos.
- Los nueve assets nativos y `latest.json`, sin residuos de otro intento.
- Version igual al tag, fecha valida, notas no vacias, cuatro plataformas y URLs de esa release.
- Firmas no vacias y `.sig` emparejados exactamente con AppImage, DEB, RPM y NSIS.

La inspeccion visual no reemplaza los gates: el workflow verifica firmas con `minisign`, compara el conjunto remoto con el local y valida el manifiesto descargado. No pulsar `Publish release` manualmente; puede saltarse comprobaciones pendientes. El workflow publica por si mismo.

Para cancelar antes de publicar:

1. Cancelar la ejecucion desde `Actions > Release` cuanto antes.
2. Comprobar en `Releases` si el job final creo un borrador.
3. Eliminar el borrador desde la interfaz si se abandona, o conservarlo para que un rerun lo reconcilie.
4. Mantener el tag si solo se corregira una causa externa. Si el codigo era incorrecto, preparar otra version; no mover el tag.

Cancelar no revierte una release que ya alcanzo el paso final. Si fue publicada, no reemplazar sus assets: tratarla como inmutable y publicar una version correctiva.

## Recuperacion de fallos

- Antes del borrador: corregir variables, secrets, runner o incidencia transitoria y usar `Re-run failed jobs`, o `workflow_dispatch` con el mismo tag y el `candidate_run_id` aprobado.
- Despues del borrador: conservarlo y reintentar. El workflow elimina assets remotos residuales, sube el conjunto local exacto y revalida.
- Por version, tests, lockfiles o compilacion: corregir en `main`, incrementar otra vez la version y crear un tag nuevo. No retaggear.
- Por intervencion manual que publico algo parcial: no confiar en ese canal. Retirarlo de `latest` si es imprescindible, investigar y publicar una version correctiva; clientes pueden haber descargado ya el manifiesto.
- Por secret o variable ausente: configurarlo en GitHub y reintentar; nunca agregarlo al repositorio.

Una release incompleta que sigue en draft no es visible mediante `/releases/latest` y no sustituye el `latest.json` estable anterior.

## Primera release con updater

Una aplicacion anterior al plugin updater no puede descubrir la primera release habilitada. Esa version se instala manualmente y sirve como N-1. La prueba real requiere publicar despues una segunda version habilitada N:

1. Publicar la primera release habilitada y comprobar que `latest.json` es publico.
2. Instalar AppImage, DEB, RPM y NSIS en sus sistemas de prueba.
3. Crear datos representativos y cerrar/reabrir la aplicacion.
4. Preparar y publicar la siguiente version con el workflow.
5. Ejecutar la lista N-1 a N en los cuatro canales.

## Prueba real N-1 a N

1. Instalar N-1 desde sus assets publicados en cuatro entornos: AppImage, DEB, RPM y NSIS.
2. Probar rutas con espacios para AppImage o descarga cuando la plataforma lo permita.
3. Crear propietario, productos, inventario y una venta; anotar totales y conteos.
4. Dejar un carrito no cobrado y comprobar su advertencia.
5. Confirmar que N-1 detecta N al iniciar, muestra versiones/notas y no descarga antes de `Actualizar ahora`.
6. Elegir `Despues`, seguir usando el POS y recuperar el flujo mediante la consulta manual.
7. Reiniciar N-1 y confirmar que vuelve a ofrecer N.
8. Actualizar sin checkout ni mutacion del carrito en curso.
9. En Windows aceptar que NSIS puede cerrar la aplicacion; `downloadAndInstall` no necesariamente regresa a la UI.
10. En Linux confirmar reemplazo de AppImage o elevacion para `dpkg -i`/`rpm -U`, y comprobar el relanzamiento cuando la llamada regrese.
11. Abrir N, verificar su version y confirmar que SQLite, propietario, productos, inventario y ventas sobrevivieron.
12. Repetir con usuario estandar, confirmar la elevacion esperada para DEB/RPM y registrar cancelaciones o fallos.

Probar por separado en un entorno controlado: `latest.json` malformado, firma alterada o de otro archivo, interrupcion de red y cierre durante la descarga. La instalacion vigente debe permanecer utilizable y un reintento posterior debe ser posible.

No alterar la release estable para estas pruebas. Un draft no es accesible anonimamente por `/releases/latest/download/latest.json` y produccion tiene el endpoint estable embebido. Probar un draft requiere una build QA separada con otro endpoint publico/controlado y artefactos firmados, o infraestructura dinamica de staging. Con el repositorio actual, los gates validan el borrador y la prueba N-1 a N estable se completa inmediatamente despues de publicar N. Los casos destructivos solo se ejecutan en QA.

Registrar SO, arquitectura, versiones, instalador, ruta, privilegios, resultado y logs sanitizados; nunca claves ni tokens.

## Rotacion de claves

La clave publica queda embebida en cada aplicacion. Un cliente que solo confia en la clave anterior rechazara una release firmada solo con la nueva. `latest.json` tiene una firma por plataforma y el endpoint estatico de GitHub no sirve firmas diferentes por cliente.

- No rotar periodicamente una clave protegida; respaldarla y limitar acceso favorece la continuidad.
- Una rotacion planificada necesita primero una release puente firmada con la clave anterior que embeba la nueva publica.
- No cambiar el canal a releases firmadas con la nueva hasta que la poblacion objetivo instale el puente.
- Clientes offline durante la ventana no pueden saltar directamente a la clave nueva con el endpoint actual. Conservar una ruta manual al puente o implementar antes un servicio/canal para ambas cohortes.
- Conservar la clave anterior en respaldo restringido mientras haya clientes dependientes; no usarla para operacion ordinaria tras migrar.
- Si la privada fue comprometida, dejar de publicar con ella. Los clientes antiguos pueden requerir instalacion manual confiable; un puente firmado con una clave comprometida no recupera por si solo la confianza.

Toda rotacion requiere plan y prueba N-1 a N propios. No reemplazar silenciosamente la variable y los secrets esperando compatibilidad automatica.

## Firma Tauri y Authenticode

La `.sig` de Tauri autentica el artefacto para clientes que confian en la clave publica embebida. No identifica al editor ante Windows, no elimina SmartScreen y no sustituye Authenticode. Si se incorpora Authenticode, su certificado, timestamping y proteccion son un proceso adicional; las firmas Tauri siguen siendo necesarias.

## Proteccion obligatoria

- Proteger `main` con pull request, al menos una aprobacion y conversaciones resueltas.
- Exigir `Frontend and Rust (Linux)` y `Rust cfg(not(unix)) (Windows)`.
- Bloquear force-push y eliminacion de `main`.
- Aplicar una ruleset a `v*` que permita crear tags solo al rol autorizado y bloquee su actualizacion y eliminacion; nunca mover uno existente.
- Habilitar immutable releases de GitHub para impedir cambios en tags y assets despues de publicar.
- Proteger workflows y scripts de release mediante `CODEOWNERS` y revision obligatoria.
- Mantener el environment `release` con required reviewers, deployments limitados a `main` y los secrets de firma como environment secrets siempre que sea posible.
- Mantener `release-publication` separado, con required reviewers y deployments limitados a `main`, sin secrets de firma. Aprobar solamente el run ID cuya evidencia cerrada registra los mismos hashes.

## Diagnostico

| Sintoma | Accion |
| --- | --- |
| `Invalid release tag` | Usar exactamente `vX.Y.Z`, sin prerelease ni espacios. |
| Versiones no coinciden | Igualar los tres manifests, regenerar `Cargo.lock` si corresponde y ejecutar `pnpm validate:version vX.Y.Z`. |
| Tag fuera de `main` | Integrar normalmente y crear un tag nuevo sobre `main`. |
| Falta la publica | Revisar `TAURI_UPDATER_PUBLIC_KEY`; no agregarla literal a la config base. |
| Falla la firma | Revisar ambos secrets, contrasena y pareja publica; no imprimir valores. |
| Se esperan archivos comprimidos | Son heredados; se exigen AppImage, DEB, RPM y NSIS nativos con sus `.sig`. |
| Falta una firma DEB/RPM | Cancelar; los cuatro canales updater deben estar firmados y verificados. |
| `latest.json` usa una clave generica | Cancelar; debe usar las cuatro claves especificas por formato. |
| `latest.json` apunta a MSI | Cancelar; MSI es exclusivamente manual. |
| El draft no aparece en `/latest` | Es normal; usar el procedimiento QA para staging. |
| No ofrece la primera version habilitada | Instalar manualmente la primera base con updater. |
| Windows cierra la app | Es esperado durante NSIS; despues verificar N y SQLite. |
| Linux queda en `Actualizacion instalada` | Confirmar el relanzamiento; no es automatico. |
| Rechazo tras rotar clave | Restaurar ruta al puente o instalar manualmente; no desactivar verificacion. |
| Falla despues de crear draft | Reintentar mismo tag/commit; eliminar el draft solo si se abandona. |

## Referencias

- [Workflow de release](../.github/workflows/release.yml)
- [Validador de version](../scripts/validate-version.mjs)
- [Generador y validador de `latest.json`](../scripts/latest-json.mjs)
- [Verificador de firmas](../scripts/verify-updater-signatures.mjs)
- [Plan de implementacion](plans/github-actions-updater.md)
