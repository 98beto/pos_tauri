# Operacion de releases

Esta guia corresponde al repositorio publico `98beto/pos_tauri`, la rama `main` y los workflows `.github/workflows/release-request.yml` y `.github/workflows/release.yml`. El canal estable del updater es:

```text
https://github.com/98beto/pos_tauri/releases/latest/download/latest.json
```

El workflow compila el commit inmutable de un tag `vX.Y.Z`, valida todos los artefactos, crea o reutiliza una release en borrador y la publica automaticamente solo despues de superar todos los gates. No hay un paso manual de publicacion entre la validacion y `gh release edit --draft=false --latest`.

## Prerrequisitos locales

- Acceso de escritura al repositorio y permiso para crear tags.
- Rama local `main` actualizada, limpia y basada en `origin/main`.
- Node.js 24.15.0, pnpm 12.4.1 y Rust 1.98.1 con `rustfmt` y `clippy`.
- Dependencias de sistema de Tauri 2 si se ejecutan builds nativos locales.
- Cambios de la release ya integrados en `main` y CI verde.
- Variable, environment protegido y secrets de firma configurados en GitHub antes de crear el tag.
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

- Crear el environment `release`, exigir required reviewers y limitar deployments a `main` (la rama default).
- Crear preferiblemente como environment secrets de `release` el secret `TAURI_SIGNING_PRIVATE_KEY` con el contenido completo de la clave privada y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` con su contrasena. Si temporalmente son repository secrets, el environment protegido sigue siendo obligatorio.

Introducir los valores solamente en los campos protegidos de la interfaz. Evitar comandos CLI que incluyan el valor en argumentos, porque pueden quedar en el historial o la lista de procesos. La variable publica se inyecta en un overlay efimero durante la release; `src-tauri/tauri.conf.json` no contiene una clave de produccion ni un valor sustituible que pueda publicarse por accidente.

Ejecutar manualmente `Release` no es una prueba inocua: si todos los gates pasan, publicara la release.

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
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
git diff --exit-code -- pnpm-lock.yaml src-tauri/Cargo.lock
```

El ultimo comando se ejecuta despues de confirmar los cambios de version y locks; comprueba que instalaciones y pruebas no los modificaron de nuevo. `pnpm validate:version` rechaza formatos distintos de `vX.Y.Z` y discrepancias entre los tres archivos.

Crear un commit normal mediante pull request. Cuando este en `main`, crear el tag anotado sobre el commit exacto y publicarlo:

```bash
git switch main
git pull --ff-only origin main
git tag -a v1.2.3 -m "Linea POS v1.2.3"
git push origin v1.2.3
```

No mover ni reutilizar un tag enviado. El workflow exige que su commit pertenezca a la historia de `origin/main` y que no exista una release ya publicada para ese tag.

## Ejecucion del workflow

Un push de `v*` ejecuta primero `Release request`, que no tiene secrets ni permisos de escritura. Al terminar correctamente, `Release` se carga desde la rama default mediante `workflow_run`, obtiene exactamente un tag SemVer que apunte a `workflow_run.head_sha`, exige que el SHA pertenezca a `origin/main` y compila ese SHA. `workflow_dispatch` solo puede lanzarse seleccionando `main`; recibe un tag `vX.Y.Z` ya existente y aplica las mismas validaciones. Se usa para recuperar o reintentar, no para publicar codigo sin tag.

Cada job firmado genera `dist` con `pnpm build` antes de exponer los secrets. El overlay efimero anula `build.beforeBuildCommand`, por lo que `tauri-action` reutiliza ese `frontendDist` y Vite o sus plugins no heredan las credenciales de firma. La compilacion Rust firmada si recibe necesariamente esos secrets.

Windows MSI, que no recibe secretos, puede compilar mientras espera la aprobacion. Tras la aprobacion de los required reviewers, se ejecutan los jobs firmados de Linux y Windows NSIS. Sus resultados temporales duran siete dias. El job `publish` tambien usa el environment protegido y:

1. Verifica criptograficamente las firmas locales.
2. Confirma que el tag remoto sigue apuntando al commit validado.
3. Crea o reutiliza el borrador.
4. Genera `latest.json` y valida version, URLs, firmas y el conjunto exacto de assets.
5. Sube los assets, vuelve a descargarlos y repite la verificacion remota.
6. Publica atomica y automaticamente la release como `latest`.

## Artefactos esperados

La config base mantiene `createUpdaterArtifacts=false`, por lo que builds normales no requieren firma. Para `1.2.3`, el overlay efimero de release establece `createUpdaterArtifacts=true` en Linux/NSIS y `false` en MSI; produce estos siete assets nativos y el workflow agrega `latest.json`:

```text
Linea POS_1.2.3_amd64.AppImage
Linea POS_1.2.3_amd64.AppImage.sig
Linea POS_1.2.3_amd64.deb
Linea POS-1.2.3-1.x86_64.rpm
Linea POS_1.2.3_x64-setup.exe
Linea POS_1.2.3_x64-setup.exe.sig
Linea POS_1.2.3_x64_en-US.msi
latest.json
```

Una version futura cambia solo el segmento de version. Cada sufijo debe existir exactamente una vez. No son validos los formatos heredados `*.AppImage.tar.gz`, `*.AppImage.tar.gz.sig`, `*.nsis.zip` o `*.nsis.zip.sig`.

| Plataforma | Instalacion manual | Updater |
| --- | --- | --- |
| Linux | AppImage, `.deb`, `.rpm` | AppImage y su `.sig` |
| Windows | NSIS `setup.exe`, MSI | NSIS `setup.exe` y su `.sig` |

`latest.json` contiene solamente `linux-x86_64` apuntando al AppImage y `windows-x86_64` apuntando al `setup.exe`. `.deb`, `.rpm` y MSI son manuales y nunca deben aparecer como URL del updater.

## Revision, publicacion y cancelacion

El borrador solo existe durante la fase final y puede durar pocos segundos. Si se necesita observarlo, abrir `Releases` mientras corre `Assemble and atomically publish` y comprobar:

- Tag, titulo y notas correctos.
- Los siete assets nativos y `latest.json`, sin residuos de otro intento.
- Version igual al tag, fecha valida, notas no vacias, dos plataformas y URLs de esa release.
- Firmas no vacias y `.sig` emparejados exactamente con AppImage y NSIS.

La inspeccion visual no reemplaza los gates: el workflow verifica firmas con `minisign`, compara el conjunto remoto con el local y valida el manifiesto descargado. No pulsar `Publish release` manualmente; puede saltarse comprobaciones pendientes. El workflow publica por si mismo.

Para cancelar antes de publicar:

1. Cancelar la ejecucion desde `Actions > Release` cuanto antes.
2. Comprobar en `Releases` si el job final creo un borrador.
3. Eliminar el borrador desde la interfaz si se abandona, o conservarlo para que un rerun lo reconcilie.
4. Mantener el tag si solo se corregira una causa externa. Si el codigo era incorrecto, preparar otra version; no mover el tag.

Cancelar no revierte una release que ya alcanzo el paso final. Si fue publicada, no reemplazar sus assets: tratarla como inmutable y publicar una version correctiva.

## Recuperacion de fallos

- Antes del borrador: corregir variables, secrets, runner o incidencia transitoria y usar `Re-run failed jobs`, o `workflow_dispatch` con el mismo tag.
- Despues del borrador: conservarlo y reintentar. El workflow elimina assets remotos residuales, sube el conjunto local exacto y revalida.
- Por version, tests, lockfiles o compilacion: corregir en `main`, incrementar otra vez la version y crear un tag nuevo. No retaggear.
- Por intervencion manual que publico algo parcial: no confiar en ese canal. Retirarlo de `latest` si es imprescindible, investigar y publicar una version correctiva; clientes pueden haber descargado ya el manifiesto.
- Por secret o variable ausente: configurarlo en GitHub y reintentar; nunca agregarlo al repositorio.

Una release incompleta que sigue en draft no es visible mediante `/releases/latest` y no sustituye el `latest.json` estable anterior.

## Primera release con updater

Una aplicacion anterior al plugin updater no puede descubrir la primera release habilitada. Esa version se instala manualmente mediante AppImage o NSIS y sirve como N-1. La prueba real requiere publicar despues una segunda version habilitada N:

1. Publicar la primera release habilitada y comprobar que `latest.json` es publico.
2. Instalar su AppImage en Linux y su NSIS en Windows.
3. Crear datos representativos y cerrar/reabrir la aplicacion.
4. Preparar y publicar la siguiente version con el workflow.
5. Ejecutar la lista N-1 a N en ambas plataformas.

## Prueba real N-1 a N

1. Instalar N-1 desde sus assets publicados: AppImage en Linux y NSIS por usuario en Windows.
2. Probar rutas con espacios para el AppImage o descarga cuando la plataforma lo permita.
3. Crear propietario, productos, inventario y una venta; anotar totales y conteos.
4. Dejar un carrito no cobrado y comprobar su advertencia.
5. Confirmar que N-1 detecta N al iniciar, muestra versiones/notas y no descarga antes de `Actualizar ahora`.
6. Elegir `Despues`, seguir usando el POS y recuperar el flujo mediante la consulta manual.
7. Reiniciar N-1 y confirmar que vuelve a ofrecer N.
8. Actualizar sin checkout ni mutacion del carrito en curso.
9. En Windows aceptar que NSIS puede cerrar la aplicacion; `downloadAndInstall` no necesariamente regresa a la UI.
10. En Linux, si la llamada regresa, comprobar que se pide confirmacion antes de relanzar.
11. Abrir N, verificar su version y confirmar que SQLite, propietario, productos, inventario y ventas sobrevivieron.
12. Repetir con usuario estandar sin privilegios administrativos y registrar cualquier elevacion inesperada.

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

## Diagnostico

| Sintoma | Accion |
| --- | --- |
| `Invalid release tag` | Usar exactamente `vX.Y.Z`, sin prerelease ni espacios. |
| Versiones no coinciden | Igualar los tres manifests, regenerar `Cargo.lock` si corresponde y ejecutar `pnpm validate:version vX.Y.Z`. |
| Tag fuera de `main` | Integrar normalmente y crear un tag nuevo sobre `main`. |
| Falta la publica | Revisar `TAURI_UPDATER_PUBLIC_KEY`; no agregarla literal a la config base. |
| Falla la firma | Revisar ambos secrets, contrasena y pareja publica; no imprimir valores. |
| Se esperan archivos comprimidos | Son heredados; se exigen AppImage/NSIS nativos y sus `.sig`. |
| `latest.json` apunta a MSI | Cancelar; debe apuntar a `*-setup.exe`. MSI es manual. |
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
