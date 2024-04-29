use {
    std::{
        env,
        io,
    },
    winres::WindowsResource,
};

fn main() -> io::Result<()> {
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        WindowsResource::new()
            // This path can be absolute, or relative to your crate root.
            // When building on Windows, comment out the first 3 lines of this block (just call set_icon)
            .set_toolkit_path("/usr/bin")
            .set_windres_path("x86_64-w64-mingw32-windres")
            .set_ar_path("x86_64-w64-mingw32-ar")
            .set_icon("./assets/icon.ico")
            .compile()?;
    }
    Ok(())
}