use std::collections::HashMap;

use flexstr::{SharedStr, ToOwnedFlexStr as _};
use heck::{ToSnakeCase, ToUpperCamelCase};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::ir::{DescriptorIr, Field, FieldKind, FieldLabel, Message};
use crate::options::CodegenOptions;

const BUFFA_VIEW_MODULE: &str = "__buffa::view";
const GOOGLE_EMPTY: &str = "google.protobuf.Empty";

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
pub struct TypeResolver {
    buffa_module: SharedStr,
    connect_module: SharedStr,
    message_locations: HashMap<SharedStr, TypeLocation>,
    known_packages: Vec<SharedStr>,
}

impl TypeResolver {
    pub fn new(ir: &DescriptorIr, options: &CodegenOptions) -> Self {
        let mut message_locations = HashMap::new();
        let mut known_packages = Vec::new();

        for file in &ir.files {
            if !known_packages
                .iter()
                .any(|package: &SharedStr| package == &file.package)
            {
                known_packages.push(file.package.clone());
            }

            for message in &file.messages {
                index_message_location(message, file.package.as_ref(), &mut message_locations);
            }
        }

        known_packages.sort_by_key(|package| std::cmp::Reverse(package.len()));

        Self {
            buffa_module: options.buffa_module.clone(),
            connect_module: options.connect_module.clone(),
            message_locations,
            known_packages,
        }
    }

    pub fn owned_message_type(&self, proto_type: &str) -> CodegenResult<RustPath> {
        let location = self.resolve_location(proto_type);
        Ok(RustPath::new(join_path(
            self.buffa_module.as_ref(),
            location.owned_segments(),
        )))
    }

    pub fn view_message_type(&self, proto_type: &str) -> CodegenResult<RustPath> {
        let location = self.resolve_location(proto_type);
        let mut segments = location.package_module_segments();
        segments.extend(BUFFA_VIEW_MODULE.split("::").map(str::to_owned));
        segments.extend(location.view_segments());

        Ok(RustPath::new(join_path(
            self.buffa_module.as_ref(),
            segments,
        )))
    }

    pub fn connect_service_trait(&self, service_full_name: &str) -> RustPath {
        let location = self.resolve_location(service_full_name);
        let mut segments = location.package_module_segments();
        let service_name = location
            .type_segments
            .last()
            .map(|name| rust_type_name(name.as_ref()))
            .unwrap_or_else(|| rust_type_name(service_full_name));
        segments.push(keyword_safe_type_name(&service_name));

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
            FieldKind::Bytes => "::std::vec::Vec<u8>".to_owned(),
            FieldKind::Group(type_name) | FieldKind::Message(type_name) => self
                .owned_message_type(type_name.as_ref())?
                .as_str()
                .to_owned(),
            FieldKind::Enum(type_name) => {
                self.proto_type_path(type_name.as_ref()).as_str().to_owned()
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

    pub fn proto_type_path(&self, proto_type: &str) -> RustPath {
        let location = self.resolve_location(proto_type);
        RustPath::new(join_path(
            self.buffa_module.as_ref(),
            location.owned_segments(),
        ))
    }

    pub fn method_fn_name(&self, method_name: &str) -> SharedStr {
        keyword_safe_value_name(&method_name.to_snake_case()).to_owned_opt()
    }

    pub fn value_ident(&self, value_name: &str, options: &CodegenOptions) -> SharedStr {
        format!(
            "{}{}",
            keyword_safe_value_name(&value_name.to_snake_case()),
            options.value_suffix.as_ref()
        )
        .to_owned_opt()
    }
}

#[derive(Clone, Debug)]
struct TypeLocation {
    package: SharedStr,
    type_segments: Vec<SharedStr>,
}

impl TypeLocation {
    fn owned_segments(&self) -> Vec<String> {
        let mut segments = self.package_module_segments();
        segments.extend(type_segments_to_owned_path(&self.type_segments));
        segments
    }

    fn view_segments(&self) -> Vec<String> {
        view_segments(&self.type_segments)
    }

    fn package_module_segments(&self) -> Vec<String> {
        package_to_modules(self.package.as_ref())
    }
}

fn index_message_location(
    message: &Message,
    package: &str,
    message_locations: &mut HashMap<SharedStr, TypeLocation>,
) {
    let type_segments = type_segments_for(package, message.full_name.as_ref());
    message_locations.insert(
        message.full_name.clone(),
        TypeLocation {
            package: package.to_owned_opt(),
            type_segments,
        },
    );

    for nested in &message.messages {
        index_message_location(nested, package, message_locations);
    }
}

impl TypeResolver {
    fn resolve_location(&self, proto_type: &str) -> TypeLocation {
        let proto_type = normalize_proto_type(proto_type);
        if let Some(location) = self.message_locations.get(proto_type) {
            return location.clone();
        }

        if proto_type == GOOGLE_EMPTY {
            return TypeLocation {
                package: "google.protobuf".into(),
                type_segments: vec!["Empty".into()],
            };
        }

        let package = self
            .known_packages
            .iter()
            .find(|package| package_matches(package.as_ref(), proto_type))
            .map_or_else(|| fallback_package(proto_type).to_owned_opt(), Clone::clone);
        let type_segments = type_segments_for(package.as_ref(), proto_type);

        TypeLocation {
            package,
            type_segments,
        }
    }
}

fn normalize_proto_type(proto_type: &str) -> &str {
    proto_type.strip_prefix('.').unwrap_or(proto_type)
}

fn package_matches(package: &str, proto_type: &str) -> bool {
    !package.is_empty()
        && (proto_type == package
            || proto_type
                .strip_prefix(package)
                .is_some_and(|rest| rest.starts_with('.')))
}

fn fallback_package(proto_type: &str) -> &str {
    proto_type
        .rsplit_once('.')
        .map_or("", |(package, _)| package)
}

fn type_segments_for(package: &str, full_name: &str) -> Vec<SharedStr> {
    let type_path = full_name
        .strip_prefix(package)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(full_name);

    type_path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_owned_opt())
        .collect()
}

fn package_to_modules(package: &str) -> Vec<String> {
    package
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(|segment| keyword_safe_module_name(&segment.to_snake_case()))
        .collect()
}

fn type_segments_to_owned_path(type_segments: &[SharedStr]) -> Vec<String> {
    let Some((last, parents)) = type_segments.split_last() else {
        return Vec::new();
    };

    let mut path = parents
        .iter()
        .map(|segment| keyword_safe_module_name(&segment.to_snake_case()))
        .collect::<Vec<_>>();
    path.push(keyword_safe_type_name(&rust_type_name(last.as_ref())));
    path
}

fn view_segments(type_segments: &[SharedStr]) -> Vec<String> {
    let Some((last, parents)) = type_segments.split_last() else {
        return Vec::new();
    };

    let mut path = parents
        .iter()
        .map(|segment| keyword_safe_module_name(&segment.to_snake_case()))
        .collect::<Vec<_>>();
    path.push(format!("{}View", rust_type_name(last.as_ref())));
    path
}

fn rust_type_name(proto_name: &str) -> String {
    proto_name.to_upper_camel_case()
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

fn keyword_safe_module_name(name: &str) -> String {
    if is_rust_keyword(name) {
        if can_be_raw_ident(name) {
            format!("r#{name}")
        } else {
            format!("{name}_")
        }
    } else {
        name.to_owned()
    }
}

fn keyword_safe_type_name(name: &str) -> String {
    if name == "Self" {
        "Self_".to_owned()
    } else if is_rust_keyword(name) && can_be_raw_ident(name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    }
}

fn keyword_safe_value_name(name: &str) -> String {
    if is_rust_keyword(name) {
        if can_be_raw_ident(name) {
            format!("r#{name}")
        } else {
            format!("{name}_")
        }
    } else {
        name.to_owned()
    }
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "gen"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

fn can_be_raw_ident(name: &str) -> bool {
    !matches!(name, "self" | "super" | "Self" | "crate")
}

#[cfg(test)]
mod tests {
    use connectrpc_codegen::codegen::descriptor::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto,
        field_descriptor_proto::{Label, Type},
    };

    use super::TypeResolver;
    use crate::CodeGeneratorRequest;
    use crate::ir::build_ir;
    use crate::options::CodegenOptions;

    #[test]
    fn resolves_buffa_owned_and_view_paths() {
        let resolver = resolver_for(vec![file("test/v1/test.proto", "test.v1")]);

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
    }

    #[test]
    fn resolves_cross_package_message_references() {
        let resolver = resolver_for(vec![
            file("test/v1/test.proto", "test.v1"),
            file("other/v1/other.proto", "other.v1"),
        ]);

        assert_eq!(
            resolver
                .owned_message_type(".other.v1.TestRequest")
                .unwrap()
                .as_str(),
            "crate::proto::other::v1::TestRequest"
        );
    }

    #[test]
    fn resolves_enum_path_and_scalar_query_types() {
        let resolver = resolver_for(vec![file("test/v1/test.proto", "test.v1")]);
        let ir = build_ir(&request(vec![file("test/v1/test.proto", "test.v1")])).unwrap();
        let field = &ir.files[0].messages[0].fields[1];

        assert_eq!(
            resolver.field_rust_type(field).unwrap().as_str(),
            "crate::proto::test::v1::Tester"
        );
    }

    #[test]
    fn resolves_connect_service_trait_path() {
        let resolver = resolver_for(vec![file("test/v1/test.proto", "test.v1")]);

        assert_eq!(
            resolver
                .connect_service_trait("test.v1.TestService")
                .as_str(),
            "crate::connect::test::v1::TestService"
        );
    }

    fn resolver_for(files: Vec<FileDescriptorProto>) -> TypeResolver {
        let request = request(files);
        let ir = build_ir(&request).unwrap();
        TypeResolver::new(&ir, &CodegenOptions::default())
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
            ..Default::default()
        }
    }
}
