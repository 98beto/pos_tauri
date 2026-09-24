# Linea POS

Aplicacion de punto de venta de escritorio construida con Tauri 2, React y SQLite. El repositorio publica instaladores para Linux y Windows y ofrece actualizaciones firmadas para AppImage y NSIS.

## Requisitos

- Node.js 24.15.0.
- pnpm 12.4.1, fijado por `packageManager`.
- Rust 1.98.1 con `rustfmt` y `clippy`, fijado en `rust-toolchain.toml`.
- Dependencias de sistema de Tauri 2 para la plataforma de desarrollo.

## Desarrollo

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

Comprobaciones principales:

```bash
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Releases

La operacion de versiones, firma, publicacion y pruebas N-1 a N se describe en [`docs/releases.md`](docs/releases.md). No se deben crear tags de release sin completar sus prerrequisitos.

## Comprobacion manual de modales

Cuando una vista use `Modal`, comprobar antes de entregarla:

1. Abrir dos modales anidados o simultaneos y pulsar Escape: solo se cierra el superior.
2. Recorrer el modal superior con Tab y Shift+Tab: el foco no sale de su panel.
3. Cerrar el superior: el foco vuelve a su disparador dentro del modal anterior, o al primer control de este si el disparador ya no existe.
4. Cerrar el ultimo modal: se recuperan el foco original, el scroll y los valores previos de `inert` y `aria-hidden`.
5. Con un modal superior abierto, confirmar en las herramientas de accesibilidad que el contenido de fondo y los modales inferiores estan aislados, y que solo el superior tiene `aria-modal=true`.
6. Ejecutar la aplicacion Tauri y confirmar que no aparecen infracciones de `style-src` en la consola.
