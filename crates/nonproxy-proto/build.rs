use std::{
    env,
    error::Error,
    io,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_directory = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("缺少 CARGO_MANIFEST_DIR"))?;
    let repository_root = manifest_directory
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::other("无法定位仓库根目录"))?;
    let proto_directory = repository_root.join("proto");
    let control_proto = proto_directory.join("nonproxy/control/v1/control.proto");
    let provider_proto = proto_directory.join("nonproxy/provider/v1/provider.proto");
    let adapter_proto = proto_directory.join("nonproxy/adapter/v1/adapter.proto");
    println!("cargo:rerun-if-changed={}", proto_directory.display());

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .boxed(".nonproxy.events.v1.EventEnvelope.payload.decision_observed")
        .emit_rerun_if_changed(true)
        .compile_protos(
            &[control_proto, provider_proto, adapter_proto],
            &[proto_directory],
        )?;

    Ok(())
}
