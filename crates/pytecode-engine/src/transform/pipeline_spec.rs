//! Declarative pipeline specification that Rust evaluates natively.
//!
//! A [`PipelineSpec`] is a list of [`PipelineStep`] values. Each step pairs
//! a matcher spec with a transform action. The pipeline iterates models and
//! evaluates matchers entirely in Rust; only custom Python callbacks cross FFI.

use crate::Result;
use crate::model::{ClassModel, CodeModel, FieldModel, MethodModel};
use crate::transform::matcher_spec::{
    ClassMatcherSpec, CompiledClassMatcher, CompiledFieldMatcher, CompiledMethodMatcher,
    FieldMatcherSpec, MethodMatcherSpec,
};
use crate::transform::transform_spec::{
    ClassTransformSpec, CodeTransformSpec, CompiledClassTransform, CompiledCodeTransform,
    CompiledMethodTransform, FieldTransformSpec, MethodTransformSpec,
};
use crate::transform::{ClassContext, MethodContext};
use std::fmt;

/// Shared callback type for custom class transforms.
pub type ClassTransformCallback =
    std::sync::Arc<dyn Fn(&mut ClassModel) -> Result<()> + Send + Sync>;

/// Shared callback type for custom field transforms.
pub type FieldTransformCallback =
    std::sync::Arc<dyn Fn(&mut FieldModel, &ClassContext) -> Result<()> + Send + Sync>;

/// Shared callback type for custom method transforms.
pub type MethodTransformCallback =
    std::sync::Arc<dyn Fn(&mut MethodModel, &ClassContext) -> Result<()> + Send + Sync>;

/// Shared callback type for custom code transforms.
pub type CodeTransformCallback = std::sync::Arc<
    dyn Fn(&mut CodeModel, &MethodContext, &ClassContext) -> Result<()> + Send + Sync,
>;

/// The action to perform on a class when a step's matcher matches.
#[derive(Clone)]
pub enum TransformAction {
    /// A built-in class transform that Rust applies natively.
    BuiltIn(ClassTransformSpec),
    /// A fallible custom callback (e.g. from Python via PyO3).
    Custom(ClassTransformCallback),
}

impl fmt::Debug for TransformAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(spec) => f.debug_tuple("BuiltIn").field(spec).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<callback>").finish(),
        }
    }
}

/// The action to perform when a field step matches.
#[derive(Clone)]
pub enum FieldTransformAction {
    /// Apply a built-in transform to each matched field.
    BuiltIn(FieldTransformSpec),
    /// Apply a custom fallible callback to each matched field.
    Custom(FieldTransformCallback),
    /// Legacy class-level guard action: runs once if any field matches.
    ClassAction(TransformAction),
}

impl fmt::Debug for FieldTransformAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(spec) => f.debug_tuple("BuiltIn").field(spec).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<callback>").finish(),
            Self::ClassAction(action) => f.debug_tuple("ClassAction").field(action).finish(),
        }
    }
}

/// The action to perform when a method step matches.
#[derive(Clone)]
pub enum MethodTransformAction {
    /// Apply a built-in transform to each matched method.
    BuiltIn(MethodTransformSpec),
    /// Apply a custom fallible callback to each matched method.
    Custom(MethodTransformCallback),
    /// Legacy class-level guard action: runs once if any method matches.
    ClassAction(TransformAction),
}

impl fmt::Debug for MethodTransformAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(spec) => f.debug_tuple("BuiltIn").field(spec).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<callback>").finish(),
            Self::ClassAction(action) => f.debug_tuple("ClassAction").field(action).finish(),
        }
    }
}

/// The action to perform when a code step matches.
#[derive(Clone)]
pub enum CodeTransformAction {
    /// Apply a built-in code transform to each matched method's code.
    BuiltIn(CodeTransformSpec),
    /// Apply a custom fallible callback to each matched method's code.
    Custom(CodeTransformCallback),
}

impl fmt::Debug for CodeTransformAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(spec) => f.debug_tuple("BuiltIn").field(spec).finish(),
            Self::Custom(_) => f.debug_tuple("Custom").field(&"<callback>").finish(),
        }
    }
}

/// A single step in a declarative pipeline.
#[derive(Debug, Clone)]
pub enum PipelineStep {
    /// Apply transform to matching classes.
    Class {
        matcher: ClassMatcherSpec,
        action: TransformAction,
    },
    /// Apply a field-level transform to matching fields.
    Field {
        owner_matcher: ClassMatcherSpec,
        field_matcher: FieldMatcherSpec,
        action: FieldTransformAction,
    },
    /// Apply a method-level transform to matching methods.
    Method {
        owner_matcher: ClassMatcherSpec,
        method_matcher: MethodMatcherSpec,
        action: MethodTransformAction,
    },
    /// Apply a code-level transform to matching methods with code.
    Code {
        owner_matcher: ClassMatcherSpec,
        method_matcher: MethodMatcherSpec,
        action: CodeTransformAction,
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
            Self::Code {
                owner_matcher,
                method_matcher,
                action,
            } => {
                write!(
                    f,
                    "on_code({method_matcher}, owner={owner_matcher}, {action:?})"
                )
            }
        }
    }
}

enum CompiledClassAction {
    BuiltIn(CompiledClassTransform),
    Custom(ClassTransformCallback),
}

enum CompiledFieldAction {
    BuiltIn(FieldTransformSpec),
    Custom(FieldTransformCallback),
    ClassAction(CompiledClassAction),
}

enum CompiledMethodAction {
    BuiltIn(CompiledMethodTransform),
    Custom(MethodTransformCallback),
    ClassAction(CompiledClassAction),
}

enum CompiledCodeAction {
    BuiltIn(CompiledCodeTransform),
    Custom(CodeTransformCallback),
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
        action: CompiledClassAction,
    },
    Field {
        owner_matcher: CompiledClassMatcher,
        field_matcher: CompiledFieldMatcher,
        action: CompiledFieldAction,
    },
    Method {
        owner_matcher: CompiledClassMatcher,
        method_matcher: CompiledMethodMatcher,
        action: CompiledMethodAction,
    },
    Code {
        owner_matcher: CompiledClassMatcher,
        method_matcher: CompiledMethodMatcher,
        action: CompiledCodeAction,
    },
}

/// A declarative pipeline: a list of steps that Rust evaluates natively.
#[derive(Debug, Clone, Default)]
pub struct PipelineSpec {
    pub steps: Vec<PipelineStep>,
}

fn compile_class_action(
    action: &TransformAction,
) -> std::result::Result<CompiledClassAction, regex::Error> {
    Ok(match action {
        TransformAction::BuiltIn(spec) => CompiledClassAction::BuiltIn(spec.try_compile()?),
        TransformAction::Custom(cb) => CompiledClassAction::Custom(cb.clone()),
    })
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
        action: FieldTransformAction,
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
        action: MethodTransformAction,
    ) -> Self {
        self.steps.push(PipelineStep::Method {
            owner_matcher,
            method_matcher,
            action,
        });
        self
    }

    /// Add a code-level step.
    pub fn on_code(
        mut self,
        method_matcher: MethodMatcherSpec,
        owner_matcher: ClassMatcherSpec,
        action: CodeTransformAction,
    ) -> Self {
        self.steps.push(PipelineStep::Code {
            owner_matcher,
            method_matcher,
            action,
        });
        self
    }

    /// Compile the pipeline for efficient repeated evaluation, validating all regexes.
    pub fn try_compile(&self) -> std::result::Result<CompiledPipeline, regex::Error> {
        let steps = self
            .steps
            .iter()
            .map(|step| -> std::result::Result<CompiledStep, regex::Error> {
                Ok(match step {
                    PipelineStep::Class { matcher, action } => CompiledStep::Class {
                        matcher: CompiledClassMatcher::try_from_spec(matcher)?,
                        action: compile_class_action(action)?,
                    },
                    PipelineStep::Field {
                        owner_matcher,
                        field_matcher,
                        action,
                    } => CompiledStep::Field {
                        owner_matcher: CompiledClassMatcher::try_from_spec(owner_matcher)?,
                        field_matcher: CompiledFieldMatcher::try_from_spec(field_matcher)?,
                        action: match action {
                            FieldTransformAction::BuiltIn(spec) => {
                                CompiledFieldAction::BuiltIn(spec.clone())
                            }
                            FieldTransformAction::Custom(cb) => {
                                CompiledFieldAction::Custom(cb.clone())
                            }
                            FieldTransformAction::ClassAction(a) => {
                                CompiledFieldAction::ClassAction(compile_class_action(a)?)
                            }
                        },
                    },
                    PipelineStep::Method {
                        owner_matcher,
                        method_matcher,
                        action,
                    } => CompiledStep::Method {
                        owner_matcher: CompiledClassMatcher::try_from_spec(owner_matcher)?,
                        method_matcher: CompiledMethodMatcher::try_from_spec(method_matcher)?,
                        action: match action {
                            MethodTransformAction::BuiltIn(spec) => {
                                CompiledMethodAction::BuiltIn(spec.try_compile()?)
                            }
                            MethodTransformAction::Custom(cb) => {
                                CompiledMethodAction::Custom(cb.clone())
                            }
                            MethodTransformAction::ClassAction(a) => {
                                CompiledMethodAction::ClassAction(compile_class_action(a)?)
                            }
                        },
                    },
                    PipelineStep::Code {
                        owner_matcher,
                        method_matcher,
                        action,
                    } => CompiledStep::Code {
                        owner_matcher: CompiledClassMatcher::try_from_spec(owner_matcher)?,
                        method_matcher: CompiledMethodMatcher::try_from_spec(method_matcher)?,
                        action: match action {
                            CodeTransformAction::BuiltIn(spec) => {
                                CompiledCodeAction::BuiltIn(spec.try_compile()?)
                            }
                            CodeTransformAction::Custom(cb) => {
                                CompiledCodeAction::Custom(cb.clone())
                            }
                        },
                    },
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(CompiledPipeline { steps })
    }

    /// Compile the pipeline for efficient repeated evaluation.
    pub fn compile(&self) -> CompiledPipeline {
        self.try_compile().expect("invalid regex in PipelineSpec")
    }

    /// Apply this pipeline to a single class model.
    pub fn apply(&self, model: &mut ClassModel) -> Result<()> {
        self.compile().apply(model)
    }

    /// Apply this pipeline to a batch of class models.
    pub fn apply_all(&self, models: &mut [ClassModel]) -> Result<()> {
        let compiled = self.compile();
        for model in models {
            compiled.apply(model)?;
        }
        Ok(())
    }
}

impl CompiledPipeline {
    /// Apply the compiled pipeline to a single class model.
    pub fn apply(&self, model: &mut ClassModel) -> Result<()> {
        for step in &self.steps {
            match step {
                CompiledStep::Class { matcher, action } => {
                    if matcher.matches(model) {
                        Self::apply_class_action(action, model)?;
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
                    match action {
                        CompiledFieldAction::BuiltIn(spec) => {
                            model.fields.retain_mut(|field| {
                                if field_matcher.matches(field) {
                                    spec.apply_to_field(field)
                                } else {
                                    true
                                }
                            });
                        }
                        CompiledFieldAction::Custom(callback) => {
                            let owner = ClassContext::from_model(model);
                            for field in &mut model.fields {
                                if field_matcher.matches(field) {
                                    callback(field, &owner)?;
                                }
                            }
                        }
                        CompiledFieldAction::ClassAction(action) => {
                            if model.fields.iter().any(|f| field_matcher.matches(f)) {
                                Self::apply_class_action(action, model)?;
                            }
                        }
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
                    match action {
                        CompiledMethodAction::BuiltIn(spec) => {
                            model.methods.retain_mut(|method| {
                                if method_matcher.matches(method) {
                                    spec.apply_to_method(method)
                                } else {
                                    true
                                }
                            });
                        }
                        CompiledMethodAction::Custom(callback) => {
                            let owner = ClassContext::from_model(model);
                            for method in &mut model.methods {
                                if method_matcher.matches(method) {
                                    callback(method, &owner)?;
                                }
                            }
                        }
                        CompiledMethodAction::ClassAction(action) => {
                            if model.methods.iter().any(|m| method_matcher.matches(m)) {
                                Self::apply_class_action(action, model)?;
                            }
                        }
                    }
                }
                CompiledStep::Code {
                    owner_matcher,
                    method_matcher,
                    action,
                } => {
                    if !owner_matcher.matches(model) {
                        continue;
                    }
                    let owner = ClassContext::from_model(model);
                    for method in &mut model.methods {
                        if !method_matcher.matches(method) {
                            continue;
                        }
                        match action {
                            CompiledCodeAction::BuiltIn(spec) => {
                                if let Some(code) = method.code_mut() {
                                    spec.apply(code);
                                }
                            }
                            CompiledCodeAction::Custom(callback) => {
                                let method_ctx = MethodContext::from_method(method);
                                if let Some(code) = method.code_mut() {
                                    callback(code, &method_ctx, &owner)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_class_action(action: &CompiledClassAction, model: &mut ClassModel) -> Result<()> {
        match action {
            CompiledClassAction::BuiltIn(spec) => {
                spec.apply(model);
                Ok(())
            }
            CompiledClassAction::Custom(callback) => callback(model),
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

        pipeline.apply(&mut foo).unwrap();
        pipeline.apply(&mut bar).unwrap();

        assert!(foo.access_flags.contains(ClassAccessFlags::FINAL));
        assert!(!bar.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn method_step_filters_by_owner_and_method() {
        let pipeline = PipelineSpec::new().on_methods(
            MethodMatcherSpec::IsConstructor,
            ClassMatcherSpec::Named("test/Foo".into()),
            MethodTransformAction::ClassAction(TransformAction::BuiltIn(
                ClassTransformSpec::AddAccessFlags(ClassAccessFlags::FINAL.bits()),
            )),
        );

        let mut foo = sample_class("test/Foo");
        pipeline.apply(&mut foo).unwrap();
        // Foo has <init>, so class transform fires
        assert!(foo.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn method_step_no_match_no_op() {
        let pipeline = PipelineSpec::new().on_methods(
            MethodMatcherSpec::Named("nonexistent".into()),
            ClassMatcherSpec::Any,
            MethodTransformAction::ClassAction(TransformAction::BuiltIn(
                ClassTransformSpec::AddAccessFlags(ClassAccessFlags::FINAL.bits()),
            )),
        );

        let mut model = sample_class("test/Foo");
        pipeline.apply(&mut model).unwrap();
        assert!(!model.access_flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn field_step_mutates_and_removes_only_matching_fields() {
        let pipeline = PipelineSpec::new()
            .on_fields(
                FieldMatcherSpec::Named("count".into()),
                ClassMatcherSpec::Named("test/Foo".into()),
                FieldTransformAction::BuiltIn(FieldTransformSpec::Sequence(vec![
                    FieldTransformSpec::Rename("total".into()),
                    FieldTransformSpec::AddAccessFlags(FieldAccessFlags::FINAL.bits()),
                ])),
            )
            .on_fields(
                FieldMatcherSpec::Named("removeMe".into()),
                ClassMatcherSpec::Any,
                FieldTransformAction::BuiltIn(FieldTransformSpec::Remove),
            );

        let mut model = sample_class("test/Foo");
        model.fields.push(FieldModel {
            access_flags: FieldAccessFlags::PRIVATE,
            name: "removeMe".to_string(),
            descriptor: "Z".to_string(),
            attributes: vec![],
        });
        model.fields.push(FieldModel {
            access_flags: FieldAccessFlags::PUBLIC,
            name: "keepMe".to_string(),
            descriptor: "Ljava/lang/String;".to_string(),
            attributes: vec![],
        });

        pipeline.apply(&mut model).unwrap();
        assert_eq!(model.fields.len(), 2);
        assert_eq!(model.fields[0].name, "total");
        assert!(
            model.fields[0]
                .access_flags
                .contains(FieldAccessFlags::FINAL)
        );
        assert_eq!(model.fields[1].name, "keepMe");
    }

    #[test]
    fn member_scoped_method_step_and_custom_error_propagation() {
        let pipeline = PipelineSpec::new().on_methods(
            MethodMatcherSpec::Named("helper".into()),
            ClassMatcherSpec::Named("test/Foo".into()),
            MethodTransformAction::BuiltIn(MethodTransformSpec::Rename("renamedHelper".into())),
        );

        let mut model = sample_class("test/Foo");
        model.methods.push(MethodModel::new(
            MethodAccessFlags::PUBLIC,
            "helper".to_string(),
            "()V".to_string(),
            None,
            vec![],
        ));
        pipeline.apply(&mut model).unwrap();
        assert!(model.methods.iter().any(|m| m.name == "renamedHelper"));
        assert!(model.methods.iter().any(|m| m.name == "<init>"));

        let failing_pipeline = PipelineSpec::new().on_classes(
            ClassMatcherSpec::Any,
            TransformAction::Custom(std::sync::Arc::new(|_| {
                Err(crate::EngineError::new(
                    0,
                    crate::EngineErrorKind::InvalidModelState {
                        reason: "boom".into(),
                    },
                ))
            })),
        );
        let err = failing_pipeline.apply(&mut model).unwrap_err();
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn try_compile_rejects_invalid_regex_in_nested_code_transform() {
        let pipeline = PipelineSpec::new().on_code(
            MethodMatcherSpec::Any,
            ClassMatcherSpec::Any,
            CodeTransformAction::BuiltIn(CodeTransformSpec::RemoveInsn {
                matcher: crate::transform::matcher_spec::InsnMatcherSpec::MethodNameMatches(
                    "[unclosed".into(),
                ),
            }),
        );
        assert!(pipeline.try_compile().is_err());
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

        pipeline.apply_all(&mut models).unwrap();

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
        pipeline.apply(&mut foo).unwrap();

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
        compiled.apply(&mut a).unwrap();
        compiled.apply(&mut b).unwrap();

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
