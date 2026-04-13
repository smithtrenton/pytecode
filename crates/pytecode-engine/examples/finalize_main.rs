use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::model::ClassModel;
use pytecode_engine::transform::{
    Pipeline, method_is_public, method_is_static, method_named, on_methods,
};
use std::fs;
use std::io;
use std::path::PathBuf;

fn required_path_arg(index: usize, name: &str) -> io::Result<PathBuf> {
    std::env::args_os()
        .nth(index)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name} path")))
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let input = required_path_arg(1, "input")?;
    let output = required_path_arg(2, "output")?;
    let bytes = fs::read(&input)?;
    let mut model = ClassModel::from_bytes(&bytes)?;
    let mut transform = Pipeline::of(on_methods(
        |method, _owner| {
            method.access_flags |= MethodAccessFlags::FINAL;
            Ok(())
        },
        Some(method_named("main") & method_is_public() & method_is_static()),
        None,
    ));
    transform.apply(&mut model)?;
    fs::write(output, model.to_bytes()?)?;
    Ok(())
}
