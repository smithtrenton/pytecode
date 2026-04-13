use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::transform::{
    Pipeline, class_named, method_is_public, method_is_static, method_named, on_methods,
};
use std::io;
use std::path::PathBuf;

fn required_path_arg(index: usize, name: &str) -> io::Result<PathBuf> {
    std::env::args_os()
        .nth(index)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name} path")))
}

fn optional_class_name() -> String {
    std::env::args()
        .nth(3)
        .unwrap_or_else(|| "HelloWorld".to_owned())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let input = required_path_arg(1, "input")?;
    let output = required_path_arg(2, "output")?;
    let class_name = optional_class_name();
    let mut jar = JarFile::open(&input)?;
    let mut transform = Pipeline::of(on_methods(
        |method, _owner| {
            method.access_flags |= MethodAccessFlags::FINAL;
            Ok(())
        },
        Some(method_named("main") & method_is_public() & method_is_static()),
        Some(class_named(class_name)),
    ));
    jar.rewrite(
        Some(&output),
        Some(&mut transform),
        RewriteOptions::default(),
    )?;
    Ok(())
}
