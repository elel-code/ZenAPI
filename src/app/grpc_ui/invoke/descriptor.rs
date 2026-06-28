use anyhow::Result;
use std::path::Path;
use zenapi::grpc::{
    load_grpc_file_descriptor_set_proto, load_grpc_proto_file_descriptor_set,
    load_grpc_reflection_descriptor_set,
};

pub(in crate::app::grpc_ui) async fn grpc_descriptor_set_for_invoke(
    endpoint: &str,
    descriptor_path: &str,
    protoc_path: &str,
) -> Result<prost_types::FileDescriptorSet> {
    let descriptor_path = descriptor_path.trim();
    if descriptor_path.is_empty() {
        return load_grpc_reflection_descriptor_set(endpoint).await;
    }

    let path = Path::new(descriptor_path);
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("proto"))
    {
        let protoc_path = protoc_path.trim();
        let protoc_path = if protoc_path.is_empty() {
            "protoc"
        } else {
            protoc_path
        };
        return load_grpc_proto_file_descriptor_set(path, &[], protoc_path);
    }

    load_grpc_file_descriptor_set_proto(path)
}
