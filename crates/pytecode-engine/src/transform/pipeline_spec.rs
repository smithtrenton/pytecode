//! Declarative pipeline specification that Rust evaluates natively.
//!
//! A [`PipelineSpec`] is a list of [`PipelineStep`] values. Each step pairs
//! a matcher spec with a transform action. The pipeline iterates models and
//! evaluates matchers entirely in Rust; only custom Python callbacks cross FFI.

use crate::model::ClassModel;
use crate::transform::matcher_spec::{
    ClassMatcherSpec, CompiledClassMatcher, CompiledFieldMatcher, CompiledMethodMatcher,
    FieldMatcherSpec, MethodMatcherSpec,
};
use crate::transform::transform_spec::ClassTransformSpec;
use std::fmt;

/// The action to perform when a step's matcher matches.
#[derive(Debug, Clone)]
pub enum TransformAction {
    /// A built-in transform that Rust applies natively.
    BuiltIn(ClassTransformSpec),
}

/// A single step in a declarative pipeline.
#[derive(Debug, Clone)]
pub enum PipelineStep {
    /// Apply transform to matching classes.
    Class {
        matcher: ClassMatcherSpec,
        action: TransformAction,
    },
    /// Apply a field-level transform (wrapped as class transform with field filtering).
    Field {
        owner_matcher: ClassMatcherSpec,
        field_matcher: FieldMatcherSpec,
        action: TransformAction,
    },
    /// Apply a method-level transform (wrapped as class transform with method filtering).
    Method {
        owner_matcher: ClassMatcherSpec,
        method_matcher: MethodMatcherSpec,
        action: TransformAction,
    },
}

impl fmt::Display for PipelineStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Class { matcher, action } => {
                write!(f, "on_classes({matcher}, {action:?})")
            }
            Self::Field {
                owner_matcher,
                field_matcher,
                action,
            } => {
                write!(
                    f,
                    "on_fields({field_matcher}, owner={owner_matcher}, {action:?})"
                )
            }
            Self::Method {
                owner_matcher,
                method_matcher,
                action,
            } => {
                write!(
                    f,
                    "on_methods({method_matcher}, owner={owner_matcher}, {action:?})"
                )
            }
        }
    }
}

/// A compiled pipeline ready for repeated evaluation.
///
/// Pre-compiles all matcher regexes on construction so that per-model
/// evaluation is as fast as possible.
pub struct CompiledPipeline {
    steps: Vec<CompiledStep>,
}

enum CompiledStep {
    Class {
        matcher: CompiledClassMatcher,
        action: TransformAction,
    },
    Field {
        owner_matcher: CompiledClassMatcher,
        field_matcher: CompiledFieldMatcher,
        action: TransformAction,
    },
    Method {
        owner_matcher: CompiledClassMatcher,
        method_matcher: CompiledMethodMatcher,
        action: TransformAction,
    },
}

/// A declarative pipeline: a list of steps that Rust evaluates natively.
#[derive(Debug, Clone, Default)]
pub struct PipelineSpec {
    pub steps: Vec<PipelineStep>,
}

impl PipelineSpec {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a class-level step.
    pub fn on_classes(mut self, matcher: ClassMatcherSpec, action: TransformAction) -> Self {
        self.steps.push(PipelineStep::Class { matcher, action });
        self
    }

    /// Add a field-level step.
    pub fn on_fields(
        mut self,
        field_matcher: FieldMatcherSpec,
        owner_matcher: ClassMatcherSpec,
        action: TransformAction,
    ) -> Self {
        self.steps.push(PipelineStep::Field {
            owner_matcher,
            field_matcher,
            action,
        });
        self
    }

    /// Add a method-level step.
    pub fn on_methods(
        mut self,
        method_matcher: MethodMatcherSpec,
        owner_matcher: ClassMatcherSpec,
        action: TransformAction,
    ) -> Self {
        self.steps.push(PipelineStep::Method {
            owner_matcher,
            method_matcher,
            action,
        });
        self
    }

    /// Compile the pipeline for efficient repeated evaluation.
    pub fn compile(&self) -> CompiledPipeline {
        let steps = self
            .steps
            .iter()
            .map(|step| match step {
                PipelineStep::Class { matcher, action } => CompiledStep::Class {
                    matcher: CompiledClassMatcher::from_spec(matcher),
                    action: action.clone(),
                },
                PipelineStep::Field {
                    owner_matcher,
                    field_matcher,
                    action,
                } => CompiledStep::Field {
                    owner_matcher: CompiledClassMatcher::from_spec(owner_matcher),
                    field_matcher: CompiledFieldMatcher::from_spec(field_matcher),
                    action: action.clone(),
                },
                PipelineStep::Method {
                    owner_matcher,
                    method_matcher,
                    action,
                } => CompiledStep::Method {
                    owner_matcher: CompiledClassMatcher::from_spec(owner_matcher),
                    method_matcher: CompiledMethodMatcher::from_spec(method_matcher),
                    action: action.clone(),
                },
            })
            .collect();
        CompiledPipeline { steps }
    }

    /// Apply this pipeline to a single class model.
    pub fn apply(&self, model: &mut ClassModel) {
        let compiled = self.compile();
        compiled.apply(model);
    }

    /// Apply this pipeline to a batch of class models.
    pub fn apply_all(&self, models: &mut [ClassModel]) {
        let compiled = self.compile();
        for model in models {
            compiled.apply(model);
        }
    }
}

impl CompiledPipeline {
    /// Apply the compiled pipeline to a single class model.
    pub fn apply(&self, model: &mut ClassModel) {
        for step in &self.steps {
            match step {
                CompiledStep::Class { matcher, action } => {
                    if matcher.matches(model) {
                        Self::apply_action(action, model);
                    }
                }
                CompiledStep::Field {
                    owner_matcher,
                    field_matcher,
                    action,
                } => {
                    if !owner_matcher.matches(model) {
                        continue;
                    }
                    // Field steps: apply built-in class transforms if any field matches
                    let has_match = model.fields.iter().any(|f| field_matcher.matches(f));
                    if has_match {
                        Self::apply_action(action, model);
                    }
                }
                CompiledStep::Method {
                    owner_matcher,
                    method_matcher,
                    action,
                } => {
                    if !owner_matcher.matches(model) {
                        continue;
                    }
                    let has_match = model.methods.iter().any(|m| method_matcher.matches(m));
                    if has_match {
                        Self::apply_action(action, model);
                    }
                }
            }
        }
    }

    fn apply_action(action: &TransformAction, model: &mut ClassModel) {
        match action {
            TransformAction::BuiltIn(spec) => spec.apply(model),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
    use crate::model::DebugInfoState;
    use crate::model::{ClassModel, CodeModel, ConstantPoolBuilder, FieldModel, MethodModel};

    fn sample_class(name: &str) -> ClassModel {
        ClassModel {
            entry_name: format!("{name}.class"),
            original_byte_len: 0,
            version: (52, 0),
            constant_pool: ConstantPoolBuilder::new(),
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            name: name.to_string(),
            super_name: Some("java/lang/Object".to_string()),
            interfaces: vec![],
            debug_info_state: DebugInfoState::Fresh,
            fields: vec![FieldModel {
                access_flags: FieldAccessFlags::PRIVATE,
                name: "count".to_string(),
                descriptor: "I".to_string(),
                attributes: vec![],
            }],
            methods: vec![MethodModel::new(
                MethodAccessFlags::PUBLIC,
                "<init>".to_string(),
                "()V".to_string(),
                Some(CodeModel::new(1, 1, DebugInfoState::Fresh)),
                vec![],
            )],
            attributes: vec![],
        }
    }

    #[test]
    fn class_step_matches_and_transforms() {
        let pipeline = PipelineSpec::new().on_classes(
            ClassMatcherSpec::Named("test/Foo".into()),
            TransformAction::BuiltIn(ClassTransformSpec::AddAccessFlags(
                ClassAccessFlags::FINAL.bits(),
            )),
        );

        let mut foo = sample_class("test/Foo");
        let mut bar = sample_class("test/Bar");

        pipeline.apply(&mut foo);
        pipeline.apply(&mut bar);

        assert!(foo.access_flags.contains(ClassAccessFlags::FINAL));
        assert!(!bar.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn method_step_filters_by_owner_and_method() {
        let pipeline = PipelineSpec::new().on_methods(
            MethodMatcherSpec::IsConstructor,
            ClassMatcherSpec::Named("test/Foo".into()),
            TransformAction::BuiltIn(ClassTransformSpec::AddAccessFlags(
                ClassAccessFlags::FINAL.bits(),
            )),
        );

        let mut foo = sample_class("test/Foo");
        pipeline.apply(&mut foo);
        // Foo has <init>, so class transform fires
        assert!(foo.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn method_step_no_match_no_op() {
        let pipeline = PipelineSpec::new().on_methods(
            MethodMatcherSpec::Named("nonexistent".into()),
            ClassMatcherSpec::Any,
            TransformAction::BuiltIn(ClassTransformSpec::AddAccessFlags(
                ClassAccessFlags::FINAL.bits(),
            )),
        );

        let mut model = sample_class("test/Foo");
        pipeline.apply(&mut model);
        assert!(!model.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn apply_all_batch() {
        let pipeline = PipelineSpec::new().on_classes(
            ClassMatcherSpec::Any,
            TransformAction::BuiltIn(ClassTransformSpec::AddInterface(
                "java/io/Serializable".into(),
            )),
        );

        let mut models = vec![
            sample_class("test/A"),
            sample_class("test/B"),
            sample_class("test/C"),
        ];

        pipeline.apply_all(&mut models);

        for model in &models {
            assert!(
                model
                    .interfaces
                    .contains(&"java/io/Serializable".to_string())
            );
        }
    }

    #[test]
    fn multi_step_pipeline() {
        let pipeline = PipelineSpec::new()
            .on_classes(
                ClassMatcherSpec::Named("test/Foo".into()),
                TransformAction::BuiltIn(ClassTransformSpec::AddAccessFlags(
                    ClassAccessFlags::FINAL.bits(),
                )),
            )
            .on_classes(
                ClassMatcherSpec::Any,
                TransformAction::BuiltIn(ClassTransformSpec::AddInterface(
                    "java/lang/Runnable".into(),
                )),
            );

        let mut foo = sample_class("test/Foo");
        pipeline.apply(&mut foo);

        assert!(foo.access_flags.contains(ClassAccessFlags::FINAL));
        assert!(foo.interfaces.contains(&"java/lang/Runnable".to_string()));
    }

    #[test]
    fn compiled_reuse() {
        let pipeline = PipelineSpec::new().on_classes(
            ClassMatcherSpec::Any,
            TransformAction::BuiltIn(ClassTransformSpec::RenameClass("test/Same".into())),
        );

        let compiled = pipeline.compile();

        let mut a = sample_class("test/A");
        let mut b = sample_class("test/B");
        compiled.apply(&mut a);
        compiled.apply(&mut b);

        assert_eq!(a.name, "test/Same");
        assert_eq!(b.name, "test/Same");
    }

    #[test]
    fn display_formatting() {
        let step = PipelineStep::Class {
            matcher: ClassMatcherSpec::Named("test/Foo".into()),
            action: TransformAction::BuiltIn(ClassTransformSpec::RenameClass("test/Bar".into())),
        };
        let s = format!("{step}");
        assert!(s.contains("on_classes"));
        assert!(s.contains("class_named"));
    }
}
