fn main() {
    slint_build::compile("ui/overlay.slint").expect("no se pudo compilar ui/overlay.slint");

    // Ícono del .exe (Explorador, barra de tareas y bandeja). Es el recurso 1,
    // que win.rs también usa para el ícono de la bandeja.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("no se pudo agregar el ícono al .exe");
    }
}
