use std::borrow::Cow;
use std::collections::BTreeMap;

use flexstr::{SharedStr, ToOwnedFlexStr as _};
use uni_error::UniError;

use crate::error::{CodegenErrKind, CodegenResult};
use crate::internal::ir::DescriptorIr;

pub struct PackagePrefixNamer<'a> {
    suppress: bool,
    packages: Vec<&'a str>,
}

impl<'a> PackagePrefixNamer<'a> {
    pub fn new(ir: &'a DescriptorIr, suppress: bool) -> Self {
        let mut packages = ir
            .files
            .iter()
            .map(|file| file.package.as_ref())
            .filter(|package| !package.is_empty())
            .collect::<Vec<_>>();
        packages.sort_unstable_by(|left, right| {
            right.len().cmp(&left.len()).then_with(|| left.cmp(right))
        });
        packages.dedup();

        Self { suppress, packages }
    }

    pub fn component_name<'b>(&self, full_name: &'b str) -> Cow<'b, str> {
        if !self.suppress {
            return Cow::Borrowed(full_name);
        }

        let full_name = full_name.strip_prefix('.').unwrap_or(full_name);
        for package in &self.packages {
            if let Some(rest) = full_name.strip_prefix(*package)
                && let Some(rest) = rest.strip_prefix('.')
                && !rest.is_empty()
            {
                return Cow::Borrowed(rest);
            }
        }

        Cow::Borrowed(full_name)
    }
}

pub struct ComponentNameTracker {
    context: &'static str,
    names: BTreeMap<SharedStr, SharedStr>,
}

impl ComponentNameTracker {
    pub fn new(context: &'static str) -> Self {
        Self {
            context,
            names: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, source: &str, output: SharedStr) -> CodegenResult<SharedStr> {
        if let Some(existing) = self.names.get(&output) {
            if existing.as_ref() != source {
                return Err(UniError::from_kind_context(
                    CodegenErrKind::ApiNameCollision,
                    format!(
                        "{} {:?} would be generated for both {:?} and {source:?}; set suppress_pkg_prefix=false or rename one of the protobuf declarations",
                        self.context,
                        output.as_ref(),
                        existing.as_ref()
                    ),
                ));
            }
        } else {
            self.names.insert(output.clone(), source.to_owned_opt());
        }

        Ok(output)
    }
}
