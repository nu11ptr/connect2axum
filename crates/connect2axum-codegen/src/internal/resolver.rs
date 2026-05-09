use buffa_codegen::CodeGenConfig;
use buffa_codegen::context::{CodeGenContext, SENTINEL_MOD};
use buffa_codegen::idents::{escape_mod_ident, make_field_ident};
use connectrpc_codegen::codegen::descriptor::FileDescriptorProto;
use flexstr::{SharedStr, ToOwnedFlexStr as _};
use heck::{ToSnakeCase, ToUpperCamelCase};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::ir::{DescriptorIr, Field, FieldKind, FieldLabel};
use crate::internal::options::CodegenOptions;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RustPath {
    pub path: SharedStr,
}

impl RustPath {
    pub fn new(path: impl AsRef<str>) -> Self {
        Self {
            path: path.as_ref().to_owned_opt(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.path.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct TypeResolver<'a> {
    connect_module: SharedStr,
    descriptor_files: &'a [FileDescriptorProto],
    files_to_generate: Vec<String>,
    buffa_config: CodeGenConfig,
}

impl<'a> TypeResolver<'a> {
    pub fn new(ir: &'a DescriptorIr, options: &CodegenOptions) -> Self {
        let mut buffa_config = CodeGenConfig::default();
        buffa_config
            .extern_paths
            .push((".".to_owned(), options.buffa_module.as_ref().to_owned()));

        Self {
            connect_module: options.connect_module.clone(),
            descriptor_files: &ir.descriptor_files,
            files_to_generate: ir
                .files_to_generate
                .iter()
                .map(|file_name| file_name.as_ref().to_owned())
                .collect(),
            buffa_config,
        }
    }

    pub fn owned_message_type(&self, proto_type: &str) -> CodegenResult<RustPath> {
        self.buffa_owned_path(proto_type)
    }

    pub fn view_message_type(&self, proto_type: &str) -> CodegenResult<RustPath> {
        let proto_fqn = dotted_proto_fqn(proto_type);
        let split = self
            .context()
            .rust_type_relative_split(&proto_fqn, "", 0)
            .ok_or_else(|| type_resolution_error(proto_type, "Buffa view type"))?;
        let prefix = if split.to_package.is_empty() {
            format!("{SENTINEL_MOD}::view")
        } else {
            format!("{}::{SENTINEL_MOD}::view", split.to_package)
        };

        Ok(RustPath::new(format!(
            "{prefix}::{}View",
            split.within_package
        )))
    }

    pub fn connect_service_trait(&self, service_full_name: &str) -> RustPath {
        let (package, service_name) = split_proto_type(service_full_name);
        let mut segments = package_to_modules(package);
        segments.push(rust_type_ident(service_name));

        RustPath::new(join_path(self.connect_module.as_ref(), segments))
    }

    pub fn field_rust_type(&self, field: &Field) -> CodegenResult<RustPath> {
        let base = match &field.kind {
            FieldKind::Double => "f64".to_owned(),
            FieldKind::Float => "f32".to_owned(),
            FieldKind::Int64 | FieldKind::Sint64 | FieldKind::Sfixed64 => "i64".to_owned(),
            FieldKind::Uint64 | FieldKind::Fixed64 => "u64".to_owned(),
            FieldKind::Int32 | FieldKind::Sint32 | FieldKind::Sfixed32 => "i32".to_owned(),
            FieldKind::Uint32 | FieldKind::Fixed32 => "u32".to_owned(),
            FieldKind::Bool => "bool".to_owned(),
            FieldKind::String => "::std::string::String".to_owned(),
            FieldKind::Bytes => "::buffa::bytes::Bytes".to_owned(),
            FieldKind::Group(type_name) | FieldKind::Message(type_name) => self
                .owned_message_type(type_name.as_ref())?
                .as_str()
                .to_owned(),
            FieldKind::Enum(type_name) => {
                let enum_type = self.proto_type_path(type_name.as_ref())?;
                format!("::buffa::EnumValue<{}>", enum_type.as_str())
            }
            FieldKind::Unknown => {
                return Err(UniError::from_kind_context(
                    CodegenErrKind::TypeResolutionFailed,
                    format!("field {} has no protobuf type", field.name.as_ref()),
                ));
            }
        };

        if field.label == Some(FieldLabel::Repeated) {
            Ok(RustPath::new(format!("::std::vec::Vec<{base}>")))
        } else {
            Ok(RustPath::new(base))
        }
    }

    pub fn proto_type_path(&self, proto_type: &str) -> CodegenResult<RustPath> {
        self.buffa_owned_path(proto_type)
    }

    pub fn method_fn_name(&self, method_name: &str) -> SharedStr {
        make_field_ident(&method_name.to_snake_case())
            .to_string()
            .to_owned_opt()
    }

    pub fn value_ident(&self, value_name: &str, options: &CodegenOptions) -> SharedStr {
        format!(
            "{}{}",
            make_field_ident(&value_name.to_snake_case()),
            options.value_suffix.as_ref()
        )
        .to_owned_opt()
    }

    fn buffa_owned_path(&self, proto_type: &str) -> CodegenResult<RustPath> {
        let proto_fqn = dotted_proto_fqn(proto_type);
        self.context()
            .rust_type_relative(&proto_fqn, "", 0)
            .map(RustPath::new)
            .ok_or_else(|| type_resolution_error(proto_type, "Buffa owned type"))
    }

    fn context(&self) -> CodeGenContext<'_> {
        CodeGenContext::for_generate(
            self.descriptor_files,
            &self.files_to_generate,
            &self.buffa_config,
        )
    }
}

fn dotted_proto_fqn(proto_type: &str) -> String {
    let proto_type = proto_type.trim();
    if proto_type.starts_with('.') {
        proto_type.to_owned()
    } else {
        format!(".{proto_type}")
    }
}

fn split_proto_type(proto_type: &str) -> (&str, &str) {
    let proto_type = proto_type.strip_prefix('.').unwrap_or(proto_type);
    proto_type.rsplit_once('.').unwrap_or(("", proto_type))
}

fn package_to_modules(package: &str) -> Vec<String> {
    package
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(|segment| escape_mod_ident(&segment.to_snake_case()))
        .collect()
}

fn rust_type_ident(proto_name: &str) -> String {
    make_field_ident(&proto_name.to_upper_camel_case()).to_string()
}

fn join_path(root: &str, segments: Vec<String>) -> String {
    let mut path = root.trim_end_matches("::").to_owned();
    for segment in segments {
        if path.is_empty() {
            path.push_str(&segment);
        } else {
            path.push_str("::");
            path.push_str(&segment);
        }
    }
    path
}

fn type_resolution_error(proto_type: &str, type_kind: &str) -> UniError<CodegenErrKind> {
    UniError::from_kind_context(
        CodegenErrKind::TypeResolutionFailed,
        format!("{type_kind} path for {proto_type} was not found in the descriptor set"),
    )
}

#[cfg(test)]
mod tests {
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };

    use super::TypeResolver;
    use crate::CodeGeneratorRequest;
    use crate::internal::ir::build_ir;
    use crate::internal::options::CodegenOptions;

    #[test]
    fn resolves_buffa_owned_and_view_paths() {
        with_resolver(vec![file("test/v1/test.proto", "test.v1")], |resolver| {
            assert_eq!(
                resolver
                    .owned_message_type("test.v1.TestRequest")
                    .unwrap()
                    .as_str(),
                "crate::proto::test::v1::TestRequest"
            );
            assert_eq!(
                resolver
                    .view_message_type(".test.v1.TestRequest")
                    .unwrap()
                    .as_str(),
                "crate::proto::test::v1::__buffa::view::TestRequestView"
            );
        });
    }

    #[test]
    fn resolves_cross_package_message_references() {
        with_resolver(
            vec![
                file("test/v1/test.proto", "test.v1"),
                file("other/v1/other.proto", "other.v1"),
            ],
            |resolver| {
                assert_eq!(
                    resolver
                        .owned_message_type(".other.v1.TestRequest")
                        .unwrap()
                        .as_str(),
                    "crate::proto::other::v1::TestRequest"
                );
            },
        );
    }

    #[test]
    fn resolves_enum_path_and_scalar_query_types() {
        let descriptor = file("test/v1/test.proto", "test.v1");
        let ir = build_ir(&request(vec![descriptor])).unwrap();
        let resolver = TypeResolver::new(&ir, &CodegenOptions::default());
        let field = &ir.files[0].messages[0].fields[1];

        assert_eq!(
            resolver.field_rust_type(field).unwrap().as_str(),
            "::buffa::EnumValue<crate::proto::test::v1::Tester>"
        );
    }

    #[test]
    fn resolves_connect_service_trait_path() {
        with_resolver(vec![file("test/v1/test.proto", "test.v1")], |resolver| {
            assert_eq!(
                resolver
                    .connect_service_trait("test.v1.TestService")
                    .as_str(),
                "crate::connect::test::v1::TestService"
            );
        });
    }

    fn with_resolver(files: Vec<FileDescriptorProto>, f: impl FnOnce(&TypeResolver<'_>)) {
        let request = request(files);
        let ir = build_ir(&request).unwrap();
        let resolver = TypeResolver::new(&ir, &CodegenOptions::default());
        f(&resolver);
    }

    fn request(files: Vec<FileDescriptorProto>) -> CodeGeneratorRequest {
        CodeGeneratorRequest {
            proto_file: files,
            ..Default::default()
        }
    }

    fn file(name: &str, package: &str) -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some(name.into()),
            package: Some(package.into()),
            message_type: vec![DescriptorProto {
                name: Some("TestRequest".into()),
                field: vec![
                    FieldDescriptorProto {
                        name: Some("name".into()),
                        number: Some(1),
                        label: Some(Label::LABEL_OPTIONAL),
                        r#type: Some(Type::TYPE_STRING),
                        json_name: Some("name".into()),
                        ..Default::default()
                    },
                    FieldDescriptorProto {
                        name: Some("tester".into()),
                        number: Some(2),
                        label: Some(Label::LABEL_OPTIONAL),
                        r#type: Some(Type::TYPE_ENUM),
                        type_name: Some(format!(".{package}.Tester")),
                        json_name: Some("tester".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            enum_type: vec![EnumDescriptorProto {
                name: Some("Tester".into()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}
